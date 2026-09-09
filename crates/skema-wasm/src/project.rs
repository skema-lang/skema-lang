//! `analyzeProject`: analysis across several in-memory files, resolving
//! `import` against a virtual file set – the browser's counterpart to
//! `skema-loader`, which needs a real filesystem and cannot run here.
//!
//! # Why this doesn't reuse `skema_loader::load_with`
//!
//! That function exists precisely to swap out how a file's text is read,
//! so reusing it is the obvious first instinct. Three things make it the
//! wrong fit anyway, and all three are semantic, not cosmetic:
//!
//! * It is **fail-fast**: `Result<Project, LoadError>`, aborting the whole
//!   load the moment one import can't be read. That is right for
//!   `skema check main.skema`, where a missing import is a terminal user
//!   error. An editor needs the opposite – a half-typed import path must
//!   degrade to that single edge resolving to `None` while every other
//!   reachable file is still analyzed. `skema_hir` already models exactly
//!   that (`InputFile::import_targets: Vec<Option<FileId>>`) and already
//!   reports it as an ordinary "import target not found" diagnostic
//!   (`namespace::build`), so nothing needs inventing here.
//! * It resolves paths with `Path::canonicalize`, i.e. against a real
//!   filesystem – there is none in a browser, and a virtual path set needs
//!   plain lexical normalization instead.
//! * `skema-wasm` deliberately depends only on the platform-neutral crates
//!   (`skema-syntax`, `skema-hir`); pulling in the `std::fs`-based loader
//!   would cross the architecture rule the core is built around.
//!
//! Unifying the two would mean parameterizing over the path type *and* the
//! error semantics *and* the resolution method – three of the four things
//! the function does, for a ~30-line breadth-first walk. So this mirrors
//! `load_with`'s *structure* (intern → queue → resolve targets, one
//! `FileId` per file in queue order) without sharing its code, and says so
//! here rather than leaving the next reader to wonder.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use skema_hir::{FileId, InputFile};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use crate::dto::{self, Diagnostic, ExportedModel, FileEntry, LocateCtx, Severity};

#[derive(Debug, Clone, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    /// A virtual path, `/`-separated. Only ever compared and joined as
    /// text – there is no filesystem behind it.
    pub path: String,
    pub source: String,
}

/// One object rather than three loose arguments: the file set is a
/// structured value, and `tsify` gives the TypeScript side a named type for
/// it instead of a positional argument list to keep straight.
#[derive(Debug, Clone, Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub files: Vec<ProjectFile>,
    /// The file carrying the one `Project` block (`docs/02`, "that file is
    /// the root file"). Everything reachable from it via `import` forms the
    /// project.
    pub root_path: String,
    pub with_locations: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileStatus {
    pub path: String,
    /// Reachable from the root file, and therefore part of the analysis
    /// (`docs/02`, "project membership is reachability-defined"). A file
    /// that is `false` here still gets its **syntax** checked – so a
    /// not-yet-imported file being edited isn't silently unchecked – but no
    /// cross-file rule is applied to it.
    pub in_project: bool,
}

#[derive(Debug, Clone, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResult {
    pub ok: bool,
    /// `false` when `rootPath` names no file in `files` – a project-level
    /// state with no source location to attach a diagnostic to, so it is
    /// reported as its own flag rather than as a diagnostic pointing at a
    /// file that isn't there.
    pub root_found: bool,
    pub files: Vec<ProjectFileStatus>,
    pub model: ExportedModel,
    /// Sorted by file, then position. `skema_hir` sorts by span alone,
    /// which interleaves files by coincidental byte offset once more than
    /// one is in play.
    pub diagnostics: Vec<Diagnostic>,
}

#[wasm_bindgen(js_name = "analyzeProject")]
pub fn analyze_project(input: ProjectInput) -> ProjectResult {
    let ProjectInput {
        files,
        root_path,
        with_locations,
    } = input;

    let by_path: HashMap<&str, &str> = files
        .iter()
        .map(|f| (f.path.as_str(), f.source.as_str()))
        .collect();

    let root = normalize_path(&root_path);
    if !by_path.contains_key(root.as_str()) {
        return ProjectResult {
            ok: false,
            root_found: false,
            files: files
                .iter()
                .map(|f| ProjectFileStatus {
                    path: f.path.clone(),
                    in_project: false,
                })
                .collect(),
            model: dto::empty_model(),
            diagnostics: orphan_diagnostics(&files, &[]),
        };
    }

    let parsed = load_project(&root, &by_path);
    let reachable: Vec<String> = parsed.iter().map(|f| f.path.clone()).collect();

    // `import_targets` must line up with `ast.imports()` positionally, and
    // a `FileId` must be the index of its file in this same list – both are
    // contracts of `skema_hir::analyze`.
    let ids: HashMap<&str, FileId> = parsed
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), FileId(i as u32)))
        .collect();
    let inputs: Vec<InputFile<'_>> = parsed
        .iter()
        .enumerate()
        .map(|(i, file)| InputFile {
            id: FileId(i as u32),
            display_path: file.path.clone(),
            ast: &file.ast,
            import_targets: file
                .ast
                .imports()
                .map(|import| {
                    let target = resolve_import_path(&file.path, import.path.as_ref()?);
                    ids.get(target.as_str()).copied()
                })
                .collect(),
        })
        .collect();

    let (model, hir_diagnostics) = skema_hir::analyze(&inputs);

    let entries: Vec<FileEntry<'_>> = parsed
        .iter()
        .map(|f| FileEntry {
            path: &f.path,
            src: f.source,
        })
        .collect();
    let ctx = LocateCtx {
        files: &entries,
        with_locations,
    };

    let mut diagnostics: Vec<Diagnostic> = parsed
        .iter()
        .flat_map(|f| {
            f.syntax_diagnostics
                .iter()
                .map(|d| dto::syntax_diagnostic(f.source, &f.path, d))
        })
        .collect();
    diagnostics.extend(hir_diagnostics.iter().map(|d| {
        let file = &parsed[d.file.0 as usize];
        dto::hir_diagnostic(file.source, &file.path, d)
    }));
    diagnostics.extend(orphan_diagnostics(&files, &reachable));
    diagnostics.sort_by(|a, b| {
        (&a.location.file, a.span.start, a.span.end).cmp(&(
            &b.location.file,
            b.span.start,
            b.span.end,
        ))
    });

    ProjectResult {
        ok: !diagnostics.iter().any(|d| d.severity == Severity::Error),
        root_found: true,
        files: files
            .iter()
            .map(|f| ProjectFileStatus {
                path: f.path.clone(),
                in_project: reachable.contains(&normalize_path(&f.path)),
            })
            .collect(),
        model: dto::export_model(&model, &ctx),
        diagnostics,
    }
}

struct ParsedFile<'a> {
    path: String,
    source: &'a str,
    ast: skema_syntax::ast::SourceFile,
    syntax_diagnostics: Vec<skema_syntax::Diagnostic>,
}

/// Syntax-only checking for files the project can't reach: they are not
/// part of the model (`docs/02`), but a file open in an editor should not
/// look error-free just because nothing imports it yet.
fn orphan_diagnostics(files: &[ProjectFile], reachable: &[String]) -> Vec<Diagnostic> {
    files
        .iter()
        .filter(|f| !reachable.contains(&normalize_path(&f.path)))
        .flat_map(|f| {
            let (_, diagnostics) = skema_syntax::parse(&f.source);
            diagnostics
                .into_iter()
                .map(|d| dto::syntax_diagnostic(&f.source, &f.path, &d))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Breadth-first over the import graph from `root`, parsing each reachable
/// file exactly once and returning them in `FileId` order (index = id).
///
/// A path that isn't in `files` never enters the queue – it isn't part of
/// the project, and the importer's `import_targets` entry stays `None`,
/// which `skema_hir::namespace` reports on its own. Cycles need no special
/// case: an already-queued path is not queued twice, and `docs/02` allows
/// import cycles anyway.
///
/// `root` must exist in `files`; the caller checks that first, because a
/// missing root is a project-level state rather than a per-file one.
fn load_project<'a>(root: &str, files: &HashMap<&'a str, &'a str>) -> Vec<ParsedFile<'a>> {
    let mut parsed: Vec<ParsedFile<'a>> = Vec::new();
    let mut queued: Vec<String> = vec![root.to_owned()];
    let mut queue: VecDeque<String> = VecDeque::from([root.to_owned()]);

    while let Some(path) = queue.pop_front() {
        let Some(&source) = files.get(path.as_str()) else {
            continue;
        };
        let (ast, syntax_diagnostics) = skema_syntax::parse(source);
        for import in ast.imports() {
            let Some(raw) = import.path.as_ref() else {
                continue; // a syntax error, already reported
            };
            let target = resolve_import_path(&path, raw);
            if files.contains_key(target.as_str()) && !queued.contains(&target) {
                queued.push(target.clone());
                queue.push_back(target);
            }
        }
        parsed.push(ParsedFile {
            path,
            source,
            ast,
            syntax_diagnostics,
        });
    }
    parsed
}

/// Resolves `raw` (as written in an `import`) relative to the importing
/// file's directory.
///
/// An absolute `raw` replaces the base entirely – the same behavior
/// `Path::join` has, and therefore what `skema-loader` inherits from
/// `std::fs`; matching it deliberately keeps a model's imports meaning the
/// same in the browser as on disk.
fn resolve_import_path(importer: &str, raw: &str) -> String {
    if raw.starts_with('/') {
        return normalize_path(raw);
    }
    let dir = importer.rsplit_once('/').map_or("", |(dir, _)| dir);
    normalize_path(&format!("{dir}/{raw}"))
}

/// Lexical `.`/`..` collapsing, POSIX-style. No filesystem is consulted –
/// there is none – so this is purely textual: two spellings that normalize
/// to the same string are the same file, and nothing else is.
fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => match stack.last() {
                Some(&last) if last != ".." => {
                    stack.pop();
                }
                // Climbing above a relative root is kept literally (it may
                // still match a file spelled the same way); above an
                // absolute root it has nowhere to go.
                _ if !absolute => stack.push(".."),
                _ => {}
            },
            segment => stack.push(segment),
        }
    }
    format!("{}{}", if absolute { "/" } else { "" }, stack.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, source: &str) -> ProjectFile {
        ProjectFile {
            path: path.to_owned(),
            source: source.to_owned(),
        }
    }

    fn analyze(files: Vec<ProjectFile>, root: &str) -> ProjectResult {
        analyze_project(ProjectInput {
            files,
            root_path: root.to_owned(),
            with_locations: true,
        })
    }

    #[test]
    fn normalizes_lexically() {
        assert_eq!(normalize_path("/a/./b/../c.skema"), "/a/c.skema");
        assert_eq!(normalize_path("a//b.skema"), "a/b.skema");
        assert_eq!(normalize_path("../x.skema"), "../x.skema");
        assert_eq!(normalize_path("/../x.skema"), "/x.skema");
    }

    #[test]
    fn resolves_imports_against_the_importing_files_directory() {
        assert_eq!(
            resolve_import_path("/model/project.skema", "./types.skema"),
            "/model/types.skema"
        );
        assert_eq!(
            resolve_import_path("/model/sub/a.skema", "../types.skema"),
            "/model/types.skema"
        );
        assert_eq!(
            resolve_import_path("/model/a.skema", "/other/b.skema"),
            "/other/b.skema"
        );
    }

    #[test]
    fn resolves_a_type_across_an_import() {
        let result = analyze(
            vec![
                file(
                    "/project.skema",
                    r#"spec 1;
                       Document root {}
                       import "./types.skema";
                       Project { mode = draft; dialect = generic; }
                       Table Customer { email types.Email [required]; }"#,
                ),
                file(
                    "/types.skema",
                    r#"spec 1;
                       Document types {}
                       Datatype Email { Field { type = string; } }"#,
                ),
            ],
            "/project.skema",
        );
        assert!(result.root_found);
        assert!(
            result.ok,
            "expected a clean analysis, got: {:?}",
            result.diagnostics
        );
        assert!(result.files.iter().all(|f| f.in_project));
        let table = &result.model.tables[0];
        assert_eq!(table.file, "/project.skema");
        assert_eq!(table.fields[0].ty.name, "types.Email");
    }

    #[test]
    fn a_missing_import_is_one_diagnostic_not_a_dead_analysis() {
        let result = analyze(
            vec![file(
                "/project.skema",
                r#"spec 1;
                   Document root {}
                   import "./gone.skema";
                   Project { mode = draft; dialect = generic; }
                   Table Customer { id int [pk]; }"#,
            )],
            "/project.skema",
        );
        assert!(result.root_found);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("import target not found")),
            "diagnostics: {:?}",
            result.diagnostics
        );
        // The rest of the file still analyzed.
        assert_eq!(result.model.tables.len(), 1);
    }

    #[test]
    fn a_cycle_terminates() {
        let result = analyze(
            vec![
                file(
                    "/a.skema",
                    r#"spec 1; Document a {} import "./b.skema";
                       Project { mode = draft; dialect = generic; }"#,
                ),
                file("/b.skema", r#"spec 1; Document b {} import "./a.skema";"#),
            ],
            "/a.skema",
        );
        assert!(result.files.iter().all(|f| f.in_project));
    }

    #[test]
    fn an_unreachable_file_is_flagged_but_still_syntax_checked() {
        let result = analyze(
            vec![
                file(
                    "/project.skema",
                    r#"spec 1; Document root {}
                       Project { mode = draft; dialect = generic; }"#,
                ),
                file("/orphan.skema", "Table {{{"),
            ],
            "/project.skema",
        );
        let orphan = result
            .files
            .iter()
            .find(|f| f.path == "/orphan.skema")
            .expect("orphan listed");
        assert!(!orphan.in_project);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.location.file == "/orphan.skema"),
            "the orphan's syntax errors should still surface"
        );
    }

    #[test]
    fn a_missing_root_is_its_own_state() {
        let result = analyze(vec![file("/a.skema", "spec 1;")], "/nope.skema");
        assert!(!result.root_found);
        assert!(!result.ok);
    }

    #[test]
    fn diagnostics_are_grouped_by_file() {
        let result = analyze(
            vec![
                file(
                    "/project.skema",
                    r#"spec 1; Document root {} import "./other.skema";
                       Project { mode = draft; dialect = generic; }
                       Table A { x nope; }"#,
                ),
                file(
                    "/other.skema",
                    "spec 1; Document other {} Table B { y nope; }",
                ),
            ],
            "/project.skema",
        );
        let files: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|d| d.location.file.as_str())
            .collect();
        let mut sorted = files.clone();
        sorted.sort_unstable();
        assert_eq!(files, sorted, "diagnostics must not interleave files");
    }
}
