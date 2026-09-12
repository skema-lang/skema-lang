//! The two messages LSP has no word for.
//!
//! Everything an editor already asks about a language - diagnostics, hover,
//! completion, definition, symbols - goes over standard LSP. A live ER
//! diagram does not: it needs the *resolved model* with the `Meta` and
//! `Style` blocks around it, and to be told when that model changed. Those
//! are two custom messages, namespaced `skema/`. Where a dragged position
//! lands is not among them - a layout file is the client's business.
//!
//! They deliberately carry `skema-api`'s own types, serialized by the same
//! serde derives that produce the browser's JSON. A client can therefore
//! type these responses with the `.d.ts` that `skema-wasm` generates,
//! instead of a second, hand-written schema that drifts.

use serde::{Deserialize, Serialize};
use skema_api::{Diagnostic, EnumMeta, ExportedModel, GroupMeta, StyleBlock, TableMeta};

/// Request: the whole project around one file, as a diagram needs it.
pub const MODEL_REQUEST: &str = "skema/model";
/// Notification: this project was re-analyzed. Carries no payload, so a
/// client with no diagram open pays nothing; one with a diagram open
/// answers by asking for the model.
pub const MODEL_CHANGED_NOTIFICATION: &str = "skema/modelChanged";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelParams {
    /// Any file of the project - the server elects the root itself.
    pub uri: lsp_types::Url,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelResponse {
    pub root_uri: Option<lsp_types::Url>,
    /// No `Project`-declaring file reaches this one, so it is being analyzed
    /// as its own root and lacks `mode`/`dialect` (`docs/02`). A client shows
    /// this rather than letting the model look quietly under-checked.
    pub is_own_root: bool,
    pub files: Vec<ProjectFile>,
    pub model: ExportedModel,
    pub diagnostics: Vec<Diagnostic>,
    /// Per file, the `Meta` blocks that carry stored positions - and the
    /// offsets an edit would go at.
    pub metas: Vec<FileMetas>,
    /// Per file, its `Style` blocks, uncascaded: which one wins is a
    /// rendering decision (`docs/03`), so the renderer makes it.
    pub styles: Vec<FileStyles>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub uri: lsp_types::Url,
    /// The same string the model's `file` fields use, so a client can join
    /// the two without knowing how paths are spelled.
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetas {
    pub path: String,
    pub tables: Vec<TableMeta>,
    pub enums: Vec<EnumMeta>,
    pub groups: Vec<GroupMeta>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStyles {
    pub path: String,
    pub blocks: Vec<StyleBlock>,
}
