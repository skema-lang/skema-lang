//! WASM bindings for skema: `check`/`export` over an in-memory source
//! string, for a future browser editor and ER-diagram ("map") tool.
//!
//! Deliberately single-file only for v1: no `skema-loader` dependency, no
//! filesystem, no `import` resolution. An `import` statement in the source
//! is not followed – it surfaces as an ordinary "import target not found"
//! diagnostic, the same as any other unresolved reference.
//!
//! Neither `check` nor `export` can fail (there is no I/O in this API), so
//! neither returns a JS exception – consistent with the core's "errors are
//! data, not control flow" stance (`skema_syntax::parse`/`skema_hir::analyze`
//! never return `Result` either).

mod dto;
mod location;

use skema_hir::{FileId, InputFile, model};
use wasm_bindgen::prelude::*;

pub use dto::{
    CheckResult, Diagnostic, ExportResult, ExportedEnum, ExportedEnumEntry, ExportedField,
    ExportedGroup, ExportedGroupMember, ExportedIndex, ExportedModel, ExportedPhysical,
    ExportedRelation, ExportedTable, ExportedTypeRef, FlagValue, Flags, RelationKind, Severity,
    Span,
};
pub use location::Location;

const DEFAULT_FILENAME: &str = "input.skema";

/// Readable panic messages in the browser console instead of an opaque
/// "unreachable" trap – defense in depth, even though the core is designed
/// to never panic on any input (`docs/01`: "a parser must never panic").
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn check(source: &str, filename: Option<String>) -> CheckResult {
    let filename = filename.unwrap_or_else(|| DEFAULT_FILENAME.to_owned());
    let (_model, diagnostics) = run_analysis(source, &filename);
    CheckResult {
        ok: is_ok(&diagnostics),
        diagnostics,
    }
}

/// Named `exportModel` rather than `export` on the JS side: `export` is a
/// reserved word in JavaScript/TypeScript, so a function literally named
/// `export` cannot be imported as a named binding (`import { export } from
/// …` is a syntax error) – wasm-bindgen itself would silently rename the
/// generated binding to `_export` to route around this, which is a
/// confusing, undocumented surprise for anyone reading the `.d.ts`.
#[wasm_bindgen(js_name = "exportModel")]
pub fn export_model_js(
    source: &str,
    filename: Option<String>,
    with_locations: bool,
) -> ExportResult {
    let filename = filename.unwrap_or_else(|| DEFAULT_FILENAME.to_owned());
    let (model, diagnostics) = run_analysis(source, &filename);
    let ok = is_ok(&diagnostics);
    let ctx = dto::LocateCtx {
        src: source,
        filename: &filename,
        with_locations,
    };
    ExportResult {
        ok,
        model: dto::export_model(&model, &ctx),
        diagnostics,
    }
}

fn run_analysis(source: &str, filename: &str) -> (model::Model, Vec<Diagnostic>) {
    let (ast, syntax_diagnostics) = skema_syntax::parse(source);
    let import_targets = vec![None; ast.imports().count()];
    let input = InputFile {
        id: FileId(0),
        display_path: filename.to_owned(),
        ast: &ast,
        import_targets,
    };
    let (model, hir_diagnostics) = skema_hir::analyze(std::slice::from_ref(&input));

    let mut diagnostics: Vec<Diagnostic> = syntax_diagnostics
        .iter()
        .map(|d| dto::syntax_diagnostic(source, filename, d))
        .collect();
    diagnostics.extend(
        hir_diagnostics
            .iter()
            .map(|d| dto::hir_diagnostic(source, filename, d)),
    );
    (model, diagnostics)
}

fn is_ok(diagnostics: &[Diagnostic]) -> bool {
    !diagnostics.iter().any(|d| d.severity == Severity::Error)
}
