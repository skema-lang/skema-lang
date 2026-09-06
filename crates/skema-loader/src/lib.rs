//! Loads a `.skema` root file and follows every `import` – the piece of
//! filesystem access that `skema-syntax` and `skema-hir` deliberately do not
//! have themselves (see their module documentation).
//!
//! Per `docs/02`, project membership is reachability-defined: a file belongs
//! to the project if it is reachable from the root file, directly or via
//! `import`. [`load`] mirrors exactly that: a breadth-first search over the
//! import graph, starting from the given file.
//!
//! # Why a separate crate instead of part of `skema-cli`
//!
//! Both `skema-cli` and `skema-lsp` need this logic, but with different text
//! sources: the CLI reads exclusively from disk, while the language server
//! should use the unsaved editor content for open files and disk for
//! everything else. [`load_with`] takes a swappable read function for that;
//! [`load`] is the common case built on top of it.

pub mod report;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::{fmt, io};

use skema_hir::FileId;
use skema_syntax::ast::SourceFile;

#[derive(Debug)]
pub struct LoadedFile {
    pub path: PathBuf,
    /// For converting byte offsets to line/column in messages – the core
    /// itself only knows byte offsets (see `skema_syntax::Span`).
    pub src: String,
    pub ast: SourceFile,
    pub parse_diagnostics: Vec<skema_syntax::Diagnostic>,
    /// In the same order as `ast.imports()`.
    pub import_targets: Vec<Option<FileId>>,
}

#[derive(Debug)]
pub struct Project {
    /// Index = `FileId.0 as usize` – the same guarantee that
    /// `skema_hir::analyze` requires from its callers.
    pub files: Vec<LoadedFile>,
}

impl Project {
    /// Builds the `InputFile` list that `skema_hir::analyze` expects.
    pub fn input_files(&self) -> Vec<skema_hir::InputFile<'_>> {
        self.files
            .iter()
            .enumerate()
            .map(|(i, f)| skema_hir::InputFile {
                id: FileId(i as u32),
                display_path: f.path.display().to_string(),
                ast: &f.ast,
                import_targets: f.import_targets.clone(),
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct LoadError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for LoadError {}

/// Loads `entry` and every file reachable via `import` from disk.
pub fn load(entry: &Path) -> Result<Project, LoadError> {
    load_with(entry, |p| std::fs::read_to_string(p))
}

/// Like [`load`], but reads content through `read` instead of hardcoding
/// `std::fs::read_to_string` – the language server plugs in a function here
/// that checks the unsaved editor buffer first.
pub fn load_with(
    entry: &Path,
    mut read: impl FnMut(&Path) -> io::Result<String>,
) -> Result<Project, LoadError> {
    let entry = canonicalize(entry)?;

    let mut ids: HashMap<PathBuf, FileId> = HashMap::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    intern(entry, &mut ids, &mut queue);

    let mut files = Vec::new();
    while let Some(path) = queue.pop_front() {
        let src = read(&path).map_err(|source| LoadError {
            path: path.clone(),
            source,
        })?;
        let (ast, parse_diagnostics) = skema_syntax::parse(&src);

        let import_targets: Vec<Option<FileId>> = ast
            .imports()
            .map(|import| {
                let rel = import.path.as_ref()?;
                let joined = path.parent()?.join(rel);
                let target = canonicalize(&joined).ok()?;
                Some(intern(target, &mut ids, &mut queue))
            })
            .collect();

        files.push(LoadedFile {
            path,
            src,
            ast,
            parse_diagnostics,
            import_targets,
        });
    }

    Ok(Project { files })
}

/// Assigns a `FileId` if `path` doesn't have one yet, and queues it for
/// processing. Entry and import targets both go through this same function
/// so that `FileId` assignment and queue order never diverge – exactly what
/// `skema_hir::analyze` requires from its callers.
fn intern(
    path: PathBuf,
    ids: &mut HashMap<PathBuf, FileId>,
    queue: &mut VecDeque<PathBuf>,
) -> FileId {
    if let Some(&id) = ids.get(&path) {
        return id;
    }
    let id = FileId(ids.len() as u32);
    ids.insert(path.clone(), id);
    queue.push_back(path);
    id
}

fn canonicalize(path: &Path) -> Result<PathBuf, LoadError> {
    path.canonicalize().map_err(|source| LoadError {
        path: path.to_owned(),
        source,
    })
}
