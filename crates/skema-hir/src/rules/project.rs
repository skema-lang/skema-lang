//! The one `Project` block: existence, uniqueness, `mode`/`dialect`
//! (`docs/02`).

use skema_syntax::Span;
use skema_syntax::ast::{self, BlockKind, ValueKind};

use super::{Dialect, Mode, ProjectSettings};
use crate::symbols::FileFacts;
use crate::{Diagnostic, FileId};

pub(crate) fn check<'a>(
    facts: &[FileFacts<'a>],
    diags: &mut Vec<Diagnostic>,
) -> Option<ProjectSettings<'a>> {
    let all: Vec<(FileId, Span, &'a ast::Block)> = facts
        .iter()
        .flat_map(|f| f.projects.iter().map(move |&(span, b)| (f.id, span, b)))
        .collect();

    let (owner, first_span, block) = match all.first() {
        Some(&x) => x,
        None => {
            let anchor_file = facts.first().map(|f| f.id).unwrap_or(FileId(0));
            let anchor_span = facts.first().map(|f| f.ast.span).unwrap_or_default();
            diags.push(Diagnostic::error(
                anchor_file,
                anchor_span,
                "no `Project` block found – exactly one is needed per project (docs/02)",
            ));
            return None;
        }
    };

    for &(file, span, _) in &all[1..] {
        diags.push(Diagnostic::error(
            file,
            span,
            format!("second `Project` block – the first is at {first_span:?} (docs/02)"),
        ));
    }

    let mode = read_word(block, "mode").and_then(|(word, span)| match word {
        "draft" => Some(Mode::Draft),
        "strict" => Some(Mode::Strict),
        other => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!("unknown `mode` value `{other}` – allowed: draft, strict"),
            ));
            None
        }
    });
    let dialect = read_word(block, "dialect").and_then(|(word, span)| match word {
        "generic" => Some(Dialect::Generic),
        "postgres" => Some(Dialect::Postgres),
        "mysql" => Some(Dialect::Mysql),
        "oracledb" => Some(Dialect::Oracledb),
        other => {
            diags.push(Diagnostic::error(
                owner,
                span,
                format!(
                    "unknown `dialect` value `{other}` – allowed: generic, postgres, mysql, oracledb"
                ),
            ));
            None
        }
    });

    if mode.is_none() && read_word(block, "mode").is_none() {
        diags.push(Diagnostic::error(
            owner,
            first_span,
            "`Project.mode` is missing – draft or strict (docs/02)",
        ));
    }
    if dialect.is_none() && read_word(block, "dialect").is_none() {
        diags.push(Diagnostic::error(
            owner,
            first_span,
            "`Project.dialect` is missing – generic, postgres, mysql, or oracledb (docs/02)",
        ));
    }

    let lint = block.child(BlockKind::Lint);
    let database = block.child(BlockKind::Database);

    Some(ProjectSettings {
        file: owner,
        mode: mode.unwrap_or(Mode::Draft),
        dialect: dialect.unwrap_or(Dialect::Generic),
        lint,
        database,
    })
}

/// Reads `key = word;` directly in the body of `block` – the form in which
/// `mode`, `dialect`, and the rules in `Lint` are written.
pub(crate) fn read_word<'a>(block: &'a ast::Block, key: &str) -> Option<(&'a str, Span)> {
    block.items.iter().find_map(|item| match &item.kind {
        ast::ItemKind::Setting(a) if a.key.text == key => match &a.value {
            Some(v) => match &v.kind {
                ValueKind::Path(p) => p.as_ident().map(|i| (i.text.as_str(), v.span)),
                _ => None,
            },
            None => None,
        },
        _ => None,
    })
}
