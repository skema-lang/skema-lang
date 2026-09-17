//! Orchestrates the individual checks and builds the [`crate::model::Model`] from them.

mod dialect;
mod group;
mod identifiers;
mod keys;
mod lint;
mod placement;
mod project;
mod resolve;
mod style;

use skema_syntax::Severity;
use skema_syntax::Span;
use skema_syntax::ast;
use smol_str::SmolStr;

use crate::model::Model;
use crate::namespace::{self, Namespaces};
use crate::symbols::{self, Decl, FileFacts};
use crate::{Diagnostic, FileId, InputFile};

/// The shared state every rule needs: the collected facts of each file plus
/// the namespace table. Only visible within this crate – from the outside,
/// only [`crate::analyze`] counts.
pub(crate) struct Ctx {
    /// Index = `FileId.0 as usize`; `analyze` requires this of its callers
    /// (see the docs there).
    pub facts: Vec<FileFacts>,
    pub namespaces: Namespaces,
}

impl Ctx {
    fn file(&self, id: FileId) -> Option<&FileFacts> {
        self.facts.get(id.0 as usize)
    }

    fn lookup_decl(&self, owner: FileId, name: &str) -> Option<&Decl> {
        self.file(owner)?.declarations.get(name)
    }

    /// Resolves `[namespace.]Name(.rest…)` as it occurs at every reference
    /// site in the language (`docs/02`, "Referencing within vs. across
    /// files"): the first segment is a namespace alias if it is one –
    /// otherwise the whole path is meant locally. Returns the owning file of
    /// the **declaration**, its name with span, and the remaining path after
    /// it (e.g. the target field of a relationship).
    ///
    /// A local declaration takes priority over a same-named namespace (the
    /// file's own `Document` name, or an import alias): inside its own file
    /// a declaration never needs the namespace step, so a `Table` sharing
    /// its name with the enclosing `Document` (or with an import alias)
    /// must still resolve to itself instead of being shadowed by it.
    fn resolve_decl_path(
        &self,
        file: FileId,
        path: &ast::Path,
    ) -> Option<(FileId, SmolStr, Span, Vec<ast::Ident>)> {
        let mut segments = path.segments();
        let first = segments.next()?;
        if self.lookup_decl(file, first.text()).is_none() {
            if let Some(target) = self.namespaces.resolve_namespace(file, first.text()) {
                let name = segments.next()?;
                return Some((
                    target,
                    SmolStr::new(name.text()),
                    name.span(),
                    segments.collect(),
                ));
            }
        }
        Some((
            file,
            SmolStr::new(first.text()),
            first.span(),
            segments.collect(),
        ))
    }
}

pub(crate) fn run(
    files: &[InputFile<'_>],
) -> (Model, Vec<crate::model::ReferenceLink>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let mut links = Vec::new();

    let facts: Vec<FileFacts> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            debug_assert_eq!(
                f.id.0 as usize, i,
                "skema_hir::analyze requires contiguous FileIds (0..len), matching the order in `files`"
            );
            symbols::collect_file_facts(
                f.id,
                f.ast.syntax(),
                f.import_targets.clone(),
                &mut diags,
            )
        })
        .collect();

    let namespaces = namespace::build(&facts, &mut diags);

    for file in &facts {
        placement::check(file.id, &file.ast, &mut diags);
        keys::check(file.id, &file.ast, &mut diags);
        dialect::check(file.id, &file.ast, &mut diags);
        style::check(file.id, &file.ast, &mut diags);
    }

    let project = project::check(&facts, &mut diags);

    let ctx = Ctx { facts, namespaces };

    let dangling_ref_severity = dangling_ref_severity(project.as_ref(), &mut diags);

    let groups = group::check(&ctx, dangling_ref_severity, &mut links, &mut diags);
    let (tables, enums) = resolve::run(
        &ctx,
        project.as_ref(),
        dangling_ref_severity,
        &mut links,
        &mut diags,
    );

    // Always-on, unlike `lint::check` below – not a style choice a team can
    // opt out of (`decisions.md`, "Lint catalog expansion (tasks 66–70)").
    resolve::check_required_fk_cycles(&tables, &mut diags);

    identifiers::check(project.as_ref(), &tables, &mut diags);

    lint::check(&ctx, project.as_ref(), &tables, &mut diags);

    diags.sort_by_key(|d| (d.span.start, d.span.end));

    (
        Model {
            tables,
            enums,
            groups,
            dialect: project
                .as_ref()
                .and_then(|p| p.dialect.engine_name())
                .map(str::to_owned),
        },
        links,
        diags,
    )
}

/// `mode` and `dialect` from the one `Project` block – or a conservative
/// fallback value if it's missing or occurs more than once (the error
/// message for that comes from [`project::check`]; subsequent rules should
/// still be able to keep running).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Draft,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Generic,
    Postgres,
    Mysql,
    Oracledb,
}

impl Dialect {
    /// The `Dialect @engine` selector this dialect expects a `Datatype` to
    /// cover, or `None` for `Generic` – it selects no engine, so there is
    /// nothing to be missing.
    pub(crate) const fn engine_name(self) -> Option<&'static str> {
        match self {
            Self::Generic => None,
            Self::Postgres => Some("postgres"),
            Self::Mysql => Some("mysql"),
            Self::Oracledb => Some("oracledb"),
        }
    }
}

/// `Database { Dialect @oracledb { identifierLength = …; } }` (`docs/11`,
/// Project-level only) – Oracle's identifier byte limit is configurable,
/// unlike Postgres's and MySQL's fixed ones (task 82).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleIdentifierLength {
    /// The default – no `identifierLength` key, or an absent `Project`.
    Standard,
    Extended,
}

impl OracleIdentifierLength {
    pub(crate) const fn limit_bytes(self) -> usize {
        match self {
            Self::Standard => crate::catalog::ORACLEDB_IDENTIFIER_LIMIT_STANDARD,
            Self::Extended => crate::catalog::ORACLEDB_IDENTIFIER_LIMIT_EXTENDED,
        }
    }
}

/// `Lint.danglingRef`'s configured severity (`docs/04`, "Other rules") – same
/// opt-in shape as every other `Lint` rule: applies only under `mode =
/// strict` and only when the key is written, otherwise every dangling
/// reference stays an unconditional `Error`. Governs every unresolved- or
/// wrong-kind-reference diagnostic in `group` and `resolve` (task 90);
/// computed once per analysis so they all agree.
fn dangling_ref_severity(
    project: Option<&ProjectSettings>,
    diags: &mut Vec<Diagnostic>,
) -> Severity {
    let Some(project) = project else {
        return Severity::Error;
    };
    if project.mode != Mode::Strict {
        return Severity::Error;
    }
    let Some(lint) = &project.lint else {
        return Severity::Error;
    };
    let Some((word, span)) = project::read_word(lint, "danglingRef") else {
        return Severity::Error;
    };
    match word.as_str() {
        "warn" => Severity::Warning,
        "error" => Severity::Error,
        other => {
            diags.push(Diagnostic::error(
                project.file,
                span,
                format!("unknown severity `{other}` – allowed: warn, error"),
            ));
            Severity::Error
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectSettings {
    /// The file in which the one canonical `Project` block lives.
    pub file: FileId,
    pub mode: Mode,
    pub dialect: Dialect,
    pub lint: Option<ast::Block>,
    pub database: Option<ast::Block>,
    pub oracle_identifier_length: OracleIdentifierLength,
}
