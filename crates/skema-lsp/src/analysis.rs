//! One analysis of one project, and everything the features read from it.
//!
//! The server keeps the most recent analysis per root file rather than
//! re-running it per request: hover, go-to-definition, completion, the
//! diagram and the diagnostics all ask the same three questions - what does
//! this project contain, where is each part written, and what is wrong with
//! it - and answering them once per edit instead of once per keystroke-plus-
//! hover is the difference between a server that keeps up and one that
//! stutters on a large model.
//!
//! Everything past the load is `skema-api`'s: the same resolved model, with
//! the same locations, that the browser editor draws its diagram from.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use skema_api::{Diagnostic as ApiDiagnostic, ExportedModel, FileEntry, LocateCtx, dto};
use skema_loader::{LoadError, LoadedFile, Project};

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
        let inputs = project.input_files();
        let (model, hir_diagnostics) = skema_hir::analyze(&inputs);

        let entries: Vec<FileEntry<'_>> = project
            .files
            .iter()
            .map(|f| FileEntry {
                path: display_path(f),
                src: &f.src,
            })
            .collect();
        let ctx = LocateCtx {
            files: &entries,
            with_locations: true,
        };
        let exported = dto::export_model(&model, &ctx);

        let mut diagnostics: Vec<ApiDiagnostic> = project
            .files
            .iter()
            .flat_map(|f| {
                f.parse_diagnostics
                    .iter()
                    .map(|d| dto::syntax_diagnostic(&f.src, display_path(f), d))
            })
            .collect();
        diagnostics.extend(hir_diagnostics.iter().filter_map(|d| {
            let file = project.files.get(d.file.0 as usize)?;
            Some(dto::hir_diagnostic(&file.src, display_path(file), d))
        }));
        // By file, then position: `skema_hir` sorts by span alone, which
        // interleaves files by coincidental byte offset once more than one
        // is in play.
        diagnostics.sort_by(|a, b| {
            (&a.location.file, a.span.start, a.span.end).cmp(&(
                &b.location.file,
                b.span.start,
                b.span.end,
            ))
        });

        Ok(Self {
            root: root.to_owned(),
            is_own_root,
            project,
            model: exported,
            diagnostics,
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

/// How a file is named in everything that leaves this server: the absolute
/// path, as `skema_loader` interned it. The client turns it back into a URI,
/// so it must be a path a URI can be built from - not a pretty relative one.
pub fn display_path(file: &LoadedFile) -> &str {
    file.path.to_str().unwrap_or_default()
}

fn severity(severity: skema_api::Severity) -> skema_syntax::Severity {
    match severity {
        skema_api::Severity::Error => skema_syntax::Severity::Error,
        skema_api::Severity::Warning => skema_syntax::Severity::Warning,
        skema_api::Severity::Info => skema_syntax::Severity::Info,
        skema_api::Severity::Hint => skema_syntax::Severity::Hint,
    }
}
