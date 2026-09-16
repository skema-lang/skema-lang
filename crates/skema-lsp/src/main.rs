//! `skema-lsp` – language server for the `.skema` DSL (stdio, synchronous).
//!
//! Why `lsp-server` instead of `tower-lsp`: synchronous, without `tokio` –
//! fewer concepts at once, and it's the same library behind rust-analyzer.
//!
//! # What it serves
//!
//! Diagnostics, hover, go-to-definition, completion, semantic tokens and the
//! document outline over standard LSP - plus three `skema/` messages a live
//! ER diagram needs and LSP has no word for ([`protocol`]).
//!
//! Every answer past the load comes from `skema-api`, the same crate the
//! browser editor's WASM build serves, so a model looks the same in both.
//!
//! # The shape of the work
//!
//! One analysis per edit, not one per request. A `didChange` re-loads the
//! project around the changed file, re-analyzes it, publishes diagnostics
//! and tells any open diagram the model moved; later requests read the
//! cached result. The file is re-parsed in full on every keystroke anyway
//! (`TextDocumentSyncKind::FULL`), and the parser is fast enough.

mod analysis;
mod completion;
mod convert;
mod definition;
mod folding;
mod hover;
mod protocol;
mod roots;
mod symbols;
mod tokens;
mod workspace_symbols;

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, FoldingRangeRequest, GotoDefinition, HoverRequest,
    Request as _, SemanticTokensFullRequest, WorkspaceSymbolRequest,
};
use lsp_types::{
    CompletionOptions, FoldingRangeProviderCapability, HoverProviderCapability, InitializeParams,
    OneOf, PublishDiagnosticsParams, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkDoneProgressOptions,
};

use analysis::Analysis;
use roots::Roots;

type DynError = Box<dyn Error + Sync + Send>;

fn main() -> Result<(), DynError> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            // The characters after which something is worth offering with
            // nothing typed yet: a relation arrow, a qualifier's dot, the
            // opening of a flag list. Everything else waits for a letter.
            trigger_characters: Some(
                [".", ">", "<", "-", "[", ","]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            ),
            ..CompletionOptions::default()
        }),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: tokens::legend(),
                    token_modifiers: Vec::new(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            },
        )),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    })?;

    let initialize = connection.initialize(capabilities)?;
    let folders = workspace_folders(&initialize);

    // `connection` moves by value into the server – it must be dropped
    // before `io_threads.join()`, otherwise its `sender` keeps the writer
    // thread alive and `join()` hangs forever (a known lsp-server pitfall).
    Server::new(connection, folders).run()?;
    io_threads.join()?;
    Ok(())
}

/// The folders to look for a root file in. `workspaceFolders` when the
/// client sends them, the deprecated `rootUri` when it is an older one, and
/// nothing at all for a client that opened a single file - which then falls
/// back to that file being its own root, exactly as before.
fn workspace_folders(initialize: &serde_json::Value) -> Vec<PathBuf> {
    let Ok(params) = serde_json::from_value::<InitializeParams>(initialize.clone()) else {
        return Vec::new();
    };
    if let Some(folders) = params.workspace_folders {
        return folders
            .iter()
            .filter_map(|f| f.uri.to_file_path().ok())
            .collect();
    }
    #[allow(deprecated)]
    params
        .root_uri
        .and_then(|uri| uri.to_file_path().ok())
        .into_iter()
        .collect()
}

struct Server {
    connection: Connection,
    /// Unsaved editor content, keyed by canonical path – see
    /// `skema_loader::load_with`.
    overlay: HashMap<PathBuf, String>,
    roots: Roots,
    /// The most recent analysis per root file.
    analyses: HashMap<PathBuf, Analysis>,
    /// Which root a file was last analyzed under.
    of_file: HashMap<PathBuf, PathBuf>,
    /// Every URI that currently carries diagnostics, so markers for a file
    /// that has left the project can be cleared rather than left behind.
    published: HashSet<Url>,
}

impl Server {
    fn new(connection: Connection, folders: Vec<PathBuf>) -> Self {
        Self {
            connection,
            overlay: HashMap::new(),
            roots: Roots::new(folders),
            analyses: HashMap::new(),
            of_file: HashMap::new(),
            published: HashSet::new(),
        }
    }

    fn run(mut self) -> Result<(), DynError> {
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => self.handle_notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    // ----------------------------------------------------------------- state

    /// Reads a file the way every analysis must: the editor's unsaved buffer
    /// first, the disk otherwise.
    fn reader(overlay: &HashMap<PathBuf, String>) -> impl FnMut(&Path) -> io::Result<String> + '_ {
        move |path: &Path| match overlay.get(path) {
            Some(text) => Ok(text.clone()),
            None => std::fs::read_to_string(path),
        }
    }

    /// Re-analyzes the project `path` belongs to, publishes its diagnostics
    /// and tells any open diagram.
    fn refresh(&mut self, path: &Path) -> Result<(), DynError> {
        // Destructured so the root election can borrow `roots` mutably while
        // the reader borrows `overlay` immutably.
        let Server { overlay, roots, .. } = self;
        let mut read = Self::reader(overlay);
        let election = roots.root_for(path, &mut read);

        let analysis = match Analysis::build(&election.root, election.is_own_root, read) {
            Ok(analysis) => analysis,
            Err(error) => {
                // A root whose imports cannot be read is not analyzable at
                // all; say so on the file the editor is actually showing
                // rather than failing silently.
                self.publish_load_error(path, &error.to_string())?;
                return Ok(());
            }
        };

        for file in &analysis.project.files {
            self.of_file
                .insert(file.path.clone(), election.root.clone());
        }
        self.of_file.insert(path.to_owned(), election.root.clone());
        self.publish(&analysis)?;
        self.analyses.insert(election.root, analysis);

        self.notify(protocol::MODEL_CHANGED_NOTIFICATION, ())
    }

    /// The analysis covering `path`, running one if none is current.
    fn analysis_for(&mut self, path: &Path) -> Option<&Analysis> {
        if !self.of_file.contains_key(path) {
            self.refresh(path).ok()?;
        }
        let root = self.of_file.get(path)?.clone();
        self.analyses.get(&root)
    }

    // -------------------------------------------------------------- messages

    fn handle_notification(&mut self, notification: Notification) -> Result<(), DynError> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                if let Some(path) = convert::path_of(&params.text_document.uri) {
                    self.overlay.insert(path.clone(), params.text_document.text);
                    self.refresh(&path)?;
                }
            }
            DidChangeTextDocument::METHOD => {
                let params: lsp_types::DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                if let Some(path) = convert::path_of(&params.text_document.uri) {
                    // Only `TextDocumentSyncKind::FULL` is advertised: every
                    // change carries the entire new content, not a patch.
                    if let Some(change) = params.content_changes.into_iter().next_back() {
                        self.overlay.insert(path.clone(), change.text);
                        self.refresh(&path)?;
                    }
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: lsp_types::DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                if let Some(path) = convert::path_of(&params.text_document.uri) {
                    // A save can move a file into or out of a project - a new
                    // `import`, a new `Project` block - so the election is no
                    // longer trustworthy.
                    self.roots.invalidate();
                    self.of_file.clear();
                    self.refresh(&path)?;
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                if let Some(path) = convert::path_of(&params.text_document.uri) {
                    // The buffer is gone; the file on disk takes over again.
                    self.overlay.remove(&path);
                    self.refresh(&path)?;
                }
            }
            DidChangeWatchedFiles::METHOD => {
                self.roots.invalidate();
                self.of_file.clear();
                let open: Vec<PathBuf> = self.overlay.keys().cloned().collect();
                for path in open {
                    self.refresh(&path)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<(), DynError> {
        let id = request.id.clone();
        match request.method.as_str() {
            HoverRequest::METHOD => {
                let (id, params) = cast::<HoverRequest>(request)?;
                let result = self.hover(&params);
                self.respond(id, &result)
            }
            GotoDefinition::METHOD => {
                let (id, params) = cast::<GotoDefinition>(request)?;
                let result = self.definition(&params);
                self.respond(id, &result.map(lsp_types::GotoDefinitionResponse::Scalar))
            }
            Completion::METHOD => {
                let (id, params) = cast::<Completion>(request)?;
                let result = self.completion(&params);
                self.respond(id, &result)
            }
            SemanticTokensFullRequest::METHOD => {
                let (id, params) = cast::<SemanticTokensFullRequest>(request)?;
                let result = self.semantic_tokens(&params);
                self.respond(id, &result)
            }
            DocumentSymbolRequest::METHOD => {
                let (id, params) = cast::<DocumentSymbolRequest>(request)?;
                let result = self.document_symbols(&params);
                self.respond(id, &result)
            }
            FoldingRangeRequest::METHOD => {
                let (id, params) = cast::<FoldingRangeRequest>(request)?;
                let result = self.folding_ranges(&params);
                self.respond(id, &result)
            }
            WorkspaceSymbolRequest::METHOD => {
                let (id, params) = cast::<WorkspaceSymbolRequest>(request)?;
                let result = self.workspace_symbols(&params);
                self.respond(id, &result)
            }
            protocol::MODEL_REQUEST => {
                let params: protocol::ModelParams = serde_json::from_value(request.params)?;
                let result = self.model(&params);
                self.respond(id, &result)
            }
            _ => self.respond(id, &serde_json::Value::Null),
        }
    }

    // -------------------------------------------------------------- features

    fn hover(&mut self, params: &lsp_types::HoverParams) -> Option<lsp_types::Hover> {
        let position = &params.text_document_position_params;
        let path = convert::path_of(&position.text_document.uri)?;
        let analysis = self.analysis_for(&path)?;
        let file = analysis.file(&path)?;
        let offset = convert::offset_of(&file.src, position.position) as u32;
        let display = analysis::display_path(file).to_owned();
        hover::hover(analysis, &display, offset)
    }

    fn definition(
        &mut self,
        params: &lsp_types::GotoDefinitionParams,
    ) -> Option<lsp_types::Location> {
        let position = &params.text_document_position_params;
        let path = convert::path_of(&position.text_document.uri)?;
        let analysis = self.analysis_for(&path)?;
        let file = analysis.file(&path)?;
        let offset = convert::offset_of(&file.src, position.position) as u32;
        definition::definition(analysis, &path, offset)
    }

    fn completion(
        &mut self,
        params: &lsp_types::CompletionParams,
    ) -> Vec<lsp_types::CompletionItem> {
        let position = &params.text_document_position;
        let Some(path) = convert::path_of(&position.text_document.uri) else {
            return Vec::new();
        };
        let Some(analysis) = self.analysis_for(&path) else {
            return Vec::new();
        };
        let Some(file) = analysis.file(&path) else {
            return Vec::new();
        };
        let offset = convert::offset_of(&file.src, position.position) as u32;
        completion::completion(analysis, &path, offset)
    }

    fn semantic_tokens(
        &mut self,
        params: &lsp_types::SemanticTokensParams,
    ) -> Option<lsp_types::SemanticTokens> {
        let path = convert::path_of(&params.text_document.uri)?;
        // Colouring must work for a file that is not part of any project -
        // the buffer's own text is all this needs.
        let source = self.source_of(&path)?;
        Some(tokens::semantic_tokens(&source))
    }

    fn document_symbols(
        &mut self,
        params: &lsp_types::DocumentSymbolParams,
    ) -> Option<lsp_types::DocumentSymbolResponse> {
        let path = convert::path_of(&params.text_document.uri)?;
        let source = self.source_of(&path)?;
        let (ast, _) = skema_syntax::parse(&source);
        let symbols = symbols::declarations(&ast)
            .iter()
            .map(|decl| symbol_of(decl, &source))
            .collect();
        Some(lsp_types::DocumentSymbolResponse::Nested(symbols))
    }

    /// Unlike every other handler, this answers from `self` rather than one
    /// file's `Analysis` - there is no "current file" for a workspace-wide
    /// search.
    fn workspace_symbols(
        &self,
        params: &lsp_types::WorkspaceSymbolParams,
    ) -> lsp_types::WorkspaceSymbolResponse {
        lsp_types::WorkspaceSymbolResponse::Flat(workspace_symbols::workspace_symbols(
            &self.analyses,
            &params.query,
        ))
    }

    fn folding_ranges(
        &mut self,
        params: &lsp_types::FoldingRangeParams,
    ) -> Option<Vec<lsp_types::FoldingRange>> {
        let path = convert::path_of(&params.text_document.uri)?;
        // Purely syntactic, like `document_symbols` - works without a
        // project, and for a file that has parse errors elsewhere.
        let source = self.source_of(&path)?;
        let (ast, _) = skema_syntax::parse(&source);
        Some(folding::folding_ranges(&source, &ast))
    }

    fn model(&mut self, params: &protocol::ModelParams) -> Option<protocol::ModelResponse> {
        let path = convert::path_of(&params.uri)?;
        let analysis = self.analysis_for(&path)?;

        let files: Vec<protocol::ProjectFile> = analysis
            .project
            .files
            .iter()
            .filter_map(|f| {
                Some(protocol::ProjectFile {
                    uri: convert::url_of(&f.path)?,
                    path: analysis::display_path(f).to_owned(),
                })
            })
            .collect();

        let metas = analysis
            .project
            .files
            .iter()
            .map(|f| protocol::FileMetas {
                path: analysis::display_path(f).to_owned(),
                tables: skema_api::table_metas(&f.src).tables,
                enums: skema_api::enum_metas(&f.src).enums,
                groups: skema_api::group_metas(&f.src).groups,
            })
            .collect();

        let styles = analysis
            .project
            .files
            .iter()
            .map(|f| protocol::FileStyles {
                path: analysis::display_path(f).to_owned(),
                blocks: skema_api::style_blocks(&f.src).blocks,
            })
            .collect();

        Some(protocol::ModelResponse {
            root_uri: convert::url_of(&analysis.root),
            is_own_root: analysis.is_own_root,
            files,
            model: analysis.model.clone(),
            diagnostics: analysis.diagnostics.clone(),
            metas,
            styles,
        })
    }

    /// The current text of a file: the editor's buffer, the last analysis,
    /// or the disk - in that order of freshness.
    fn source_of(&mut self, path: &Path) -> Option<String> {
        if let Some(text) = self.overlay.get(path) {
            return Some(text.clone());
        }
        if let Some(analysis) = self.analysis_for(path) {
            if let Some(file) = analysis.file(path) {
                return Some(file.src.clone());
            }
        }
        std::fs::read_to_string(path).ok()
    }

    // ------------------------------------------------------------- reporting

    fn publish(&mut self, analysis: &Analysis) -> Result<(), DynError> {
        let by_file = analysis.lsp_diagnostics();
        let mut current: HashSet<Url> = HashSet::new();

        for (path, diagnostics) in by_file {
            let Some(uri) = convert::url_of(&path) else {
                continue;
            };
            current.insert(uri.clone());
            self.send_diagnostics(uri, diagnostics)?;
        }

        // Anything previously reported and no longer part of this project
        // keeps its markers otherwise – a file removed from an `import`
        // would stay red forever.
        let stale: Vec<Url> = self
            .published
            .difference(&current)
            .filter(|uri| {
                uri.to_file_path()
                    .ok()
                    .is_some_and(|path| !self.analyses.values().any(|a| a.contains(&path)))
            })
            .cloned()
            .collect();
        for uri in stale {
            self.send_diagnostics(uri.clone(), Vec::new())?;
            self.published.remove(&uri);
        }
        self.published.extend(current);
        Ok(())
    }

    fn publish_load_error(&mut self, path: &Path, message: &str) -> Result<(), DynError> {
        let Some(uri) = convert::url_of(path) else {
            return Ok(());
        };
        let diagnostic = lsp_types::Diagnostic {
            range: lsp_types::Range::default(),
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            message: message.to_owned(),
            source: Some("skema".to_owned()),
            ..lsp_types::Diagnostic::default()
        };
        self.published.insert(uri.clone());
        self.send_diagnostics(uri, vec![diagnostic])
    }

    fn send_diagnostics(
        &self,
        uri: Url,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> Result<(), DynError> {
        self.notify(
            PublishDiagnostics::METHOD,
            PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            },
        )
    }

    fn notify(&self, method: &str, params: impl serde::Serialize) -> Result<(), DynError> {
        self.connection
            .sender
            .send(Message::Notification(Notification {
                method: method.to_owned(),
                params: serde_json::to_value(params)?,
            }))?;
        Ok(())
    }

    fn respond(&self, id: RequestId, result: &impl serde::Serialize) -> Result<(), DynError> {
        self.connection
            .sender
            .send(Message::Response(Response::new_ok(
                id,
                serde_json::to_value(result)?,
            )))?;
        Ok(())
    }
}

fn cast<R>(request: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    request.extract(R::METHOD)
}

#[allow(deprecated)] // `DocumentSymbol::deprecated` is required by the struct.
fn symbol_of(decl: &symbols::Decl, source: &str) -> lsp_types::DocumentSymbol {
    lsp_types::DocumentSymbol {
        name: decl.name.clone(),
        detail: decl.detail.clone(),
        kind: symbols::symbol_kind(decl.kind),
        tags: None,
        deprecated: None,
        range: convert::range_of(source, decl.span),
        selection_range: convert::range_of(source, decl.name_span),
        children: Some(
            decl.children
                .iter()
                .map(|child| symbol_of(child, source))
                .collect(),
        ),
    }
}
