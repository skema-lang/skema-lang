//! One analysis of one loaded project, in the shape every front end shows.
//!
//! [`crate::load`] answers "which files are in this project"; this answers
//! "and what is in them" – by running `skema_hir::analyze` and converting
//! its output into `skema-api`'s types, the same ones `skema-wasm` hands to
//! a browser.
//!
//! # Why this lives here and not in each front end
//!
//! `skema-cli`, `skema-py` and `skema-lsp` all need the same sequence: run
//! the analysis, pair every `FileId` back with its text and display path,
//! merge the parser's per-file diagnostics with the checker's project-wide
//! ones. Three copies agreed only by accident — and did not fully agree:
//! the CLI and the Python bindings used to emit `skema_hir::model::Model`
//! directly, whose `file` is a raw `FileId` a caller cannot resolve.
//!
//! `skema-api` cannot host it: pairing a `FileId` with a path needs a
//! filesystem, which that crate deliberately has not (its in-memory
//! counterpart is `skema_api::project`). This crate has both.

use skema_api::{Diagnostic, ExportedModel, FileEntry, LocateCtx, dto};

use crate::{LoadError, LoadedFile, Project};

/// The result of checking a loaded project: what the model is, and what is
/// wrong with it. As everywhere else in the workspace, both are always
/// present – errors are data, not control flow.
#[derive(Debug)]
pub struct ProjectAnalysis {
    pub model: ExportedModel,
    pub diagnostics: Vec<Diagnostic>,
}

impl ProjectAnalysis {
    pub fn ok(&self) -> bool {
        skema_api::is_ok(&self.diagnostics)
    }
}

/// Analyzes an already-loaded project.
///
/// `with_locations` gates the per-entity `location` field on the model,
/// which is large on a big model and useless to a caller that only wants
/// the structure. Diagnostics always carry theirs – that list is short, and
/// a message without a position is barely a message.
pub fn analyze(project: &Project, with_locations: bool) -> ProjectAnalysis {
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
        with_locations,
    };

    let mut diagnostics: Vec<Diagnostic> = project
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
    // interleaves files by coincidental byte offset once more than one is in
    // play. This is also the reading order a terminal wants.
    diagnostics.sort_by(|a, b| {
        (&a.location.file, a.span.start, a.span.end).cmp(&(
            &b.location.file,
            b.span.start,
            b.span.end,
        ))
    });

    ProjectAnalysis {
        model: dto::export_model(&model, &ctx),
        diagnostics,
    }
}

/// How a file is named in everything that leaves this workspace: the
/// absolute path, as [`crate::load`] interned it.
///
/// `skema-lsp` needs it to be a path a URI can be built from, rather than a
/// prettier relative one, and there is no second convention worth having.
pub fn display_path(file: &LoadedFile) -> &str {
    file.path.to_str().unwrap_or_default()
}

/// A failed load as a diagnostic, so callers report it through
/// [`ProjectAnalysis::diagnostics`] instead of keeping a second error path
/// with its own JSON shape.
///
/// The span is empty at the file start — nothing was parsed, so there is no
/// better position.
///
/// The message is the I/O error alone, without [`LoadError`]'s `path: `
/// prefix: a diagnostic names its file in `location` and every renderer
/// puts that in front, so the prefix would print the path twice.
pub fn load_error_diagnostic(error: &LoadError) -> Diagnostic {
    dto::synthetic_diagnostic(
        &error.path.to_string_lossy(),
        skema_api::Severity::Error,
        &error.source.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::load;

    /// Writes a two-file project and returns `(dir, root path)`. The root
    /// imports the other file and uses a type from it, so the model spans
    /// both and each half has a mistake of its own.
    fn two_file_project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("types.skema"),
            "Document types;\nDatatype Id { base = int; }\nTable Extra { bad wrongtype; }\n",
        )
        .expect("write types");
        let root = dir.path().join("project.skema");
        fs::write(
            &root,
            "Project { mode = draft; dialect = generic; }\nDocument main;\nimport \"./types.skema\" as t;\nTable Customer { id t.Id [pk]; }\n",
        )
        .expect("write root");
        (dir, root)
    }

    #[test]
    fn model_entities_name_a_path_not_a_file_index() {
        // The whole reason this function exists: `skema_hir::model` carries
        // a `FileId`, which is an index into a load order the caller cannot
        // see. Anything leaving the workspace must name the file instead.
        let (_dir, root) = two_file_project();
        let project = load(&root).expect("load");
        let analysis = analyze(&project, false);

        let table = analysis
            .model
            .tables
            .iter()
            .find(|t| t.name == "Customer")
            .expect("Customer in model");
        assert!(
            table.file.ends_with("project.skema"),
            "expected a path, got {:?}",
            table.file
        );
    }

    #[test]
    fn diagnostics_group_by_file_then_position() {
        // `skema_hir` sorts by span alone, which interleaves files by
        // coincidental byte offset – the sort here is what makes a terminal
        // report readable, so it is worth pinning.
        let (_dir, root) = two_file_project();
        let project = load(&root).expect("load");
        let analysis = analyze(&project, false);

        assert!(
            !analysis.diagnostics.is_empty(),
            "the fixture has a bad type, so there is something to report"
        );
        let keyed: Vec<_> = analysis
            .diagnostics
            .iter()
            .map(|d| (&d.location.file, d.span.start))
            .collect();
        let mut sorted = keyed.clone();
        sorted.sort();
        assert_eq!(keyed, sorted);
    }

    #[test]
    fn locations_are_opt_in_on_the_model_but_never_on_diagnostics() {
        let (_dir, root) = two_file_project();
        let project = load(&root).expect("load");

        let without = analyze(&project, false);
        assert!(
            without.model.tables.iter().all(|t| t.location.is_none()),
            "with_locations = false must leave the model's locations out"
        );
        assert!(
            without
                .diagnostics
                .iter()
                .all(|d| !d.location.file.is_empty()),
            "a diagnostic without a position is barely a message"
        );

        let with = analyze(&project, true);
        assert!(with.model.tables.iter().all(|t| t.location.is_some()));
    }

    #[test]
    fn a_load_error_names_its_file_once() {
        // The path belongs in `location`; repeating it in the message made
        // `skema-cli` print it twice.
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.skema");
        let error = load(&missing).expect_err("missing file must fail");
        let diagnostic = load_error_diagnostic(&error);

        assert!(diagnostic.location.file.ends_with("nope.skema"));
        assert!(
            !diagnostic.message.contains("nope.skema"),
            "message should be the io error alone, got {:?}",
            diagnostic.message
        );
    }
}
