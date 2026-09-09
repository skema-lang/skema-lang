//! Which file is the project's root file.
//!
//! `docs/02` is explicit that a single file cannot be fully checked in
//! isolation: `mode` and `dialect` come from the one `Project` block, and
//! project membership is reachability from the file that carries it. The
//! first version of this server sidestepped that by treating whatever file
//! the editor opened as the root, which makes every non-root file in a real
//! project report a missing `Project` block and lose its `strict`-only
//! lints.
//!
//! So the root is *elected*, not assumed: every `.skema` file in the
//! workspace that declares a `Project` block is a candidate, and a file
//! belongs to the candidate whose import graph actually reaches it - the
//! language's own membership rule, asked rather than guessed. A file no
//! candidate reaches falls back to being its own root, which is both the old
//! behaviour and the only sensible answer for a scratch file.
//!
//! The election is cached per file, because it costs a parse of every
//! candidate's project. It is dropped whenever something could have changed
//! the import graph on disk - a save, a create, a delete - rather than on
//! every keystroke: typing inside a file cannot move it into another
//! project, and typing an `import` does so only once the file is saved.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// Directories that never hold a hand-written model but can hold thousands
/// of files. Skipped by name, since a workspace scan that walks into
/// `node_modules` is the difference between instant and unusable.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    ".direnv",
    ".devenv",
];

/// How deep to walk below a workspace folder. Deep enough for any model
/// layout, shallow enough that a mistakenly opened home directory does not
/// turn into an unbounded walk.
const MAX_DEPTH: usize = 12;

#[derive(Debug, Default)]
pub struct Roots {
    folders: Vec<PathBuf>,
    /// Files declaring a `Project` block, from the last scan.
    candidates: Option<Vec<PathBuf>>,
    /// file → its elected root, memoized.
    elected: HashMap<PathBuf, PathBuf>,
}

impl Roots {
    pub fn new(folders: Vec<PathBuf>) -> Self {
        Self {
            folders,
            ..Self::default()
        }
    }

    /// Forgets both the candidate list and every election – called when a
    /// file is written, created or removed.
    pub fn invalidate(&mut self) {
        self.candidates = None;
        self.elected.clear();
    }

    /// The root file to analyze `file` as part of, and whether it was
    /// actually elected or `file` is standing in for itself.
    pub fn root_for(
        &mut self,
        file: &Path,
        read: &mut impl FnMut(&Path) -> io::Result<String>,
    ) -> Election {
        if let Some(root) = self.elected.get(file) {
            return Election {
                root: root.clone(),
                is_own_root: root == file,
            };
        }

        let root = self.elect(file, read).unwrap_or_else(|| file.to_owned());
        self.elected.insert(file.to_owned(), root.clone());
        Election {
            is_own_root: root == file,
            root,
        }
    }

    fn elect(
        &mut self,
        file: &Path,
        read: &mut impl FnMut(&Path) -> io::Result<String>,
    ) -> Option<PathBuf> {
        let candidates = match &self.candidates {
            Some(c) => c.clone(),
            None => {
                let scanned = self.scan(read);
                self.candidates = Some(scanned.clone());
                scanned
            }
        };

        // A file that declares `Project` itself is its own root, without
        // loading anything: that is the common single-file case and the one
        // an editor hits first.
        if candidates.iter().any(|c| c == file) {
            return Some(file.to_owned());
        }

        // Otherwise: the candidate whose import graph reaches this file.
        // Several may (a file imported by two projects is legal), so the
        // shortest path wins for stability – an arbitrary but deterministic
        // choice, and the caller can override it with a setting.
        let mut reaching: Vec<PathBuf> = candidates
            .into_iter()
            .filter(|candidate| reaches(candidate, file, read))
            .collect();
        reaching.sort();
        reaching.into_iter().next()
    }

    /// Every `.skema` file under the workspace folders that declares a
    /// `Project` block. Parsed with the real parser rather than matched with
    /// a regex: "does this file declare a Project block" is a question about
    /// the language, and the parser is cheap and already here.
    fn scan(&self, read: &mut impl FnMut(&Path) -> io::Result<String>) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for folder in &self.folders {
            collect(folder, 0, &mut found);
        }
        found.retain(|path| {
            read(path).is_ok_and(|src| {
                let (file, _) = skema_syntax::parse(&src);
                file.blocks()
                    .any(|b| b.kind == skema_syntax::ast::BlockKind::Project)
            })
        });
        found.sort();
        found
    }
}

#[derive(Debug, Clone)]
pub struct Election {
    pub root: PathBuf,
    /// `true` when no `Project`-declaring file in the workspace reaches this
    /// one, so it is being analyzed as its own root. The client shows this
    /// as a hint: the model is being checked without its `Project` settings.
    pub is_own_root: bool,
}

fn reaches(root: &Path, file: &Path, read: &mut impl FnMut(&Path) -> io::Result<String>) -> bool {
    // `load_with` is fail-fast, which is right here: a project whose imports
    // cannot be read is not a project this file provably belongs to.
    let Ok(project) = skema_loader::load_with(root, |p| read(p)) else {
        return false;
    };
    project.files.iter().any(|f| f.path == file)
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_ref()) {
                collect(&path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|e| e == "skema") {
            out.push(path.canonicalize().unwrap_or(path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A workspace on disk: `root.skema` declares `Project` and imports
    /// `part.skema`; `loose.skema` is reachable from nothing.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        // Canonicalized once here: on macOS the temp dir is a symlink, and
        // every path this module compares has been through `canonicalize`.
        let base = dir.path().canonicalize().expect("canonical temp dir");
        fs::write(
            base.join("root.skema"),
            "spec 1;\nDocument root {}\nimport \"./part.skema\";\nProject { mode = draft; dialect = generic; }\n",
        )
        .expect("write root");
        fs::write(base.join("part.skema"), "spec 1;\nDocument part {}\n").expect("write part");
        fs::write(base.join("loose.skema"), "spec 1;\nDocument loose {}\n").expect("write loose");
        (dir, base)
    }

    fn read(path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn canonical(path: PathBuf) -> PathBuf {
        path.canonicalize().unwrap_or(path)
    }

    #[test]
    fn an_imported_file_is_analyzed_as_part_of_its_root() {
        let (_dir, base) = workspace();
        let mut roots = Roots::new(vec![base.clone()]);
        let election = roots.root_for(&canonical(base.join("part.skema")), &mut read);
        assert_eq!(election.root, canonical(base.join("root.skema")));
        assert!(!election.is_own_root);
    }

    #[test]
    fn the_root_file_itself_elects_itself() {
        let (_dir, base) = workspace();
        let mut roots = Roots::new(vec![base.clone()]);
        let election = roots.root_for(&canonical(base.join("root.skema")), &mut read);
        assert_eq!(election.root, canonical(base.join("root.skema")));
        assert!(election.is_own_root);
    }

    #[test]
    fn a_file_no_project_reaches_stands_in_for_itself() {
        let (_dir, base) = workspace();
        let mut roots = Roots::new(vec![base.clone()]);
        let loose = canonical(base.join("loose.skema"));
        let election = roots.root_for(&loose, &mut read);
        assert_eq!(election.root, loose);
        assert!(
            election.is_own_root,
            "the client needs to know the Project block is missing"
        );
    }

    #[test]
    fn a_new_import_is_picked_up_after_invalidation() {
        let (_dir, base) = workspace();
        let mut roots = Roots::new(vec![base.clone()]);
        let loose = canonical(base.join("loose.skema"));
        assert!(roots.root_for(&loose, &mut read).is_own_root);

        fs::write(
            base.join("root.skema"),
            "spec 1;\nDocument root {}\nimport \"./part.skema\";\nimport \"./loose.skema\";\nProject { mode = draft; dialect = generic; }\n",
        )
        .expect("rewrite root");

        assert!(
            roots.root_for(&loose, &mut read).is_own_root,
            "still the memoized answer until something says the graph moved"
        );
        roots.invalidate();
        assert_eq!(
            roots.root_for(&loose, &mut read).root,
            canonical(base.join("root.skema"))
        );
    }
}
