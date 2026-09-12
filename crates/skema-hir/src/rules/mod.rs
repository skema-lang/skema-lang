//! Orchestrates the individual checks and builds the [`crate::model::Model`] from them.

mod dialect;
mod group;
mod keys;
mod lint;
mod placement;
mod project;
mod resolve;

use skema_syntax::Span;
use skema_syntax::ast;

use crate::model::Model;
use crate::namespace::{self, Namespaces};
use crate::symbols::{self, Decl, FileFacts};
use crate::{Diagnostic, FileId, InputFile};

/// The shared state every rule needs: the collected facts of each file plus
/// the namespace table. Only visible within this crate – from the outside,
/// only [`crate::analyze`] counts.
pub(crate) struct Ctx<'a> {
    /// Index = `FileId.0 as usize`; `analyze` requires this of its callers
    /// (see the docs there).
    pub facts: Vec<FileFacts<'a>>,
    pub namespaces: Namespaces,
}

impl<'a> Ctx<'a> {
    fn file(&self, id: FileId) -> Option<&FileFacts<'a>> {
        self.facts.get(id.0 as usize)
    }

    fn lookup_decl(&self, owner: FileId, name: &str) -> Option<&Decl<'a>> {
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
    fn resolve_decl_path<'p>(
        &self,
        file: FileId,
        path: &'p ast::Path,
    ) -> Option<(FileId, &'p str, Span, &'p [ast::Ident])> {
        let (first, rest) = path.segments.split_first()?;
        if self.lookup_decl(file, &first.text).is_some() {
            return Some((file, first.text.as_str(), first.span, rest));
        }
        if let Some(target) = self.namespaces.resolve_namespace(file, &first.text) {
            let (name, rest2) = rest.split_first()?;
            Some((target, name.text.as_str(), name.span, rest2))
        } else {
            Some((file, first.text.as_str(), first.span, rest))
        }
    }
}

pub(crate) fn run(files: &[InputFile<'_>]) -> (Model, Vec<Diagnostic>) {
    let mut diags = Vec::new();

    let facts: Vec<FileFacts<'_>> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            debug_assert_eq!(
                f.id.0 as usize, i,
                "skema_hir::analyze requires contiguous FileIds (0..len), matching the order in `files`"
            );
            symbols::collect_file_facts(f.id, f.ast, &f.import_targets, &mut diags)
        })
        .collect();

    let namespaces = namespace::build(&facts, &mut diags);

    for file in &facts {
        placement::check(file.id, file.ast, &mut diags);
        keys::check(file.id, file.ast, &mut diags);
        dialect::check(file.id, file.ast, &mut diags);
    }

    let project = project::check(&facts, &mut diags);

    let ctx = Ctx { facts, namespaces };

    let groups = group::check(&ctx, &mut diags);
    let (tables, enums) = resolve::run(&ctx, project.as_ref(), &mut diags);

    lint::check(&ctx, project.as_ref(), &mut diags);

    diags.sort_by_key(|d| (d.span.start, d.span.end));

    (
        Model {
            tables,
            enums,
            groups,
        },
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectSettings<'a> {
    /// The file in which the one canonical `Project` block lives.
    pub file: FileId,
    pub mode: Mode,
    pub dialect: Dialect,
    pub lint: Option<&'a ast::Block>,
    pub database: Option<&'a ast::Block>,
}
