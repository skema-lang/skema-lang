//! One analysis of one project, and everything the features read from it.
//!
//! The server caches the most recent analysis per root file instead of
//! re-running it per request. Hover, go-to-definition, completion, the
//! diagram and the diagnostics all ask the same three questions — what does
//! this project contain, where is each part written, what is wrong with it —
//! and answering them once per edit is what keeps a large model responsive.
//!
//! Everything past the load is `skema_loader::analyze`'s, in `skema-api`'s
//! types: the same resolved model the browser editor draws from and
//! `skema check` prints. What stays here is server-specific — the per-root
//! cache and the translation into LSP's diagnostic type.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use skema_api::{Diagnostic as ApiDiagnostic, ExportedModel};
use skema_loader::{LoadError, LoadedFile, Project};

/// Re-exported rather than reimplemented: how a file is named in everything
/// that leaves this server is the same question `skema-cli` and `skema-py`
/// answer, and `skema-loader` is where the paths were interned.
pub use skema_loader::display_path;

use crate::convert;

#[derive(Debug)]
pub struct Analysis {
    /// The file the project was loaded from.
    pub root: PathBuf,
    /// No `Project`-declaring file in the workspace reaches the file that
    /// triggered this analysis, so it is standing in as its own root and is
    /// being checked without `mode`/`dialect` (`docs/02`).
    pub is_own_root: bool,
    pub project: Project,
    pub model: ExportedModel,
    /// In `skema-api`'s shape, for the custom requests - the same diagnostic
    /// list the browser editor's status bar renders.
    pub diagnostics: Vec<ApiDiagnostic>,
}

impl Analysis {
    pub fn build(
        root: &Path,
        is_own_root: bool,
        read: impl FnMut(&Path) -> io::Result<String>,
    ) -> Result<Self, LoadError> {
        let project = skema_loader::load_with(root, read)?;
        // With locations: a hover card and a diagram both want to know where
        // an entity is written, and the cost is paid once per edit here
        // rather than per request.
        let analysis = skema_loader::analyze(&project, true);

        Ok(Self {
            root: root.to_owned(),
            is_own_root,
            project,
            model: analysis.model,
            diagnostics: analysis.diagnostics,
        })
    }

    pub fn file(&self, path: &Path) -> Option<&LoadedFile> {
        self.project.files.iter().find(|f| f.path == path)
    }

    /// Every file in the project, keyed the way the client addresses them.
    pub fn contains(&self, path: &Path) -> bool {
        self.file(path).is_some()
    }

    /// LSP diagnostics per file, **including files with none**: an entry
    /// with an empty list is what makes a fixed error's marker disappear.
    pub fn lsp_diagnostics(&self) -> HashMap<PathBuf, Vec<lsp_types::Diagnostic>> {
        let mut by_file: HashMap<PathBuf, Vec<lsp_types::Diagnostic>> = self
            .project
            .files
            .iter()
            .map(|f| (f.path.clone(), Vec::new()))
            .collect();

        for diagnostic in &self.diagnostics {
            // The api diagnostics carry a display path; map it back to the
            // file it came from, which is the one the client knows.
            let Some(file) = self
                .project
                .files
                .iter()
                .find(|f| display_path(f) == diagnostic.location.file)
            else {
                continue;
            };
            by_file
                .entry(file.path.clone())
                .or_default()
                .push(convert::diagnostic(
                    &file.src,
                    skema_syntax::Span {
                        start: diagnostic.span.start,
                        end: diagnostic.span.end,
                    },
                    severity(diagnostic.severity),
                    &diagnostic.message,
                ));
        }
        by_file
    }
}

fn severity(severity: skema_api::Severity) -> skema_syntax::Severity {
    match severity {
        skema_api::Severity::Error => skema_syntax::Severity::Error,
        skema_api::Severity::Warning => skema_syntax::Severity::Warning,
        skema_api::Severity::Info => skema_syntax::Severity::Info,
        skema_api::Severity::Hint => skema_syntax::Severity::Hint,
    }
}
