//! A `.skema` project on disk: which files it consists of, and what is in
//! them.
//!
//! The filesystem access `skema-syntax` and `skema-hir` deliberately lack,
//! plus the step that turns their output into an answer a front end shows.
//!
//! ```text
//! &Path ──[load]──► Project ──[analyze]──► ExportedModel
//!                                        + Vec<Diagnostic>
//! ```
//!
//! Project membership is reachability-defined (`docs/02`): a file belongs to
//! the project if the root file reaches it, directly or via `import`.
//! [`load`] is exactly that breadth-first walk.
//!
//! # Why a separate crate instead of part of `skema-cli`
//!
//! All three front ends need this logic with different text sources: the CLI
//! and the Python bindings read only from disk, the language server prefers
//! the unsaved editor buffer for open files. [`load_with`] takes a swappable
//! read function; [`load`] is the common case on top of it.
//!
//! [`analyze`] is here for the same reason: all three then describe a model
//! identically, in `skema-api`'s types (see [`analysis`]).

pub mod analysis;

pub use analysis::{ProjectAnalysis, analyze, display_path, load_error_diagnostic};

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::{fmt, io};

use skema_hir::FileId;
use skema_syntax::ast::SourceFile;

#[derive(Debug)]
pub struct LoadedFile {
    pub path: PathBuf,
    /// Kept for the byte-offset → line/column conversion in messages; the
    /// core knows only byte offsets (see `skema_syntax::Span`).
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
                let rel = import.path()?;
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

/// Assigns a `FileId` if `path` has none yet and queues the file. Entry and
/// import targets share this function so `FileId` assignment and queue order
/// cannot diverge — what `skema_hir::analyze` requires of its callers.
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
