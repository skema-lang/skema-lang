//! `skema-lsp` – language server for the `.skema` DSL (stdio, synchronous).
//!
//! Why `lsp-server` instead of `tower-lsp`: synchronous, without `tokio` –
//! fewer concepts at once, and it's the same library behind rust-analyzer.
//!
//! # What this server does not (yet) do
//!
//! If an editor opens a file that is not the root file of its project, it
//! lacks `mode`/`dialect` from the `Project` block – `docs/02` says
//! explicitly: "A single file cannot be fully checked in isolation." The
//! server therefore treats the currently opened file itself as the root and
//! loads whatever it reaches via `import`. A workspace-wide search for the
//! real root file is a sensible next step, but is not (yet) part of this
//! first version.

mod diagnostics;

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use lsp_server::{Connection, Message, Notification};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::{
    PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

type DynError = Box<dyn Error + Sync + Send>;

fn main() -> Result<(), DynError> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    })?;
    connection.initialize(capabilities)?;

    // `connection` moves by value into `main_loop` – it must be dropped
    // before `io_threads.join()`, otherwise its `sender` keeps the writer
    // thread alive and `join()` hangs forever (a known lsp-server pitfall).
    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

/// Unsaved editor content, keyed by canonical path – see
/// `skema_loader::load_with`.
type Overlay = HashMap<PathBuf, String>;

fn main_loop(connection: Connection) -> Result<(), DynError> {
    let mut overlay: Overlay = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                // No other requests (hover, completion, …) in this first version.
            }
            Message::Notification(not) => handle_notification(&connection, &mut overlay, not)?,
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn handle_notification(
    connection: &Connection,
    overlay: &mut Overlay,
    not: Notification,
) -> Result<(), DynError> {
    match not.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: lsp_types::DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
            if let Some(path) = canonical_path(&params.text_document.uri) {
                overlay.insert(path.clone(), params.text_document.text);
                publish_for(connection, overlay, &path)?;
            }
        }
        DidChangeTextDocument::METHOD => {
            let params: lsp_types::DidChangeTextDocumentParams =
                serde_json::from_value(not.params)?;
            if let Some(path) = canonical_path(&params.text_document.uri) {
                // We only advertise `TextDocumentSyncKind::FULL`: every
                // change carries the entire new content, not a `range` patch.
                if let Some(change) = params.content_changes.into_iter().last() {
                    overlay.insert(path.clone(), change.text);
                    publish_for(connection, overlay, &path)?;
                }
            }
        }
        DidSaveTextDocument::METHOD => {
            let params: lsp_types::DidSaveTextDocumentParams = serde_json::from_value(not.params)?;
            // Saved import targets may have changed – reload.
            if let Some(path) = canonical_path(&params.text_document.uri) {
                publish_for(connection, overlay, &path)?;
            }
        }
        DidCloseTextDocument::METHOD => {
            let params: lsp_types::DidCloseTextDocumentParams = serde_json::from_value(not.params)?;
            if let Some(path) = canonical_path(&params.text_document.uri) {
                overlay.remove(&path);
                clear_diagnostics(connection, &params.text_document.uri)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn canonical_path(uri: &lsp_types::Url) -> Option<PathBuf> {
    let path = uri.to_file_path().ok()?;
    // A new file that has never been saved doesn't exist on disk – in that
    // case the uncanonicalized path remains the best available identifier.
    Some(path.canonicalize().unwrap_or(path))
}

fn publish_for(
    connection: &Connection,
    overlay: &Overlay,
    entry: &std::path::Path,
) -> Result<(), DynError> {
    let overlay = overlay.clone();
    let by_file = diagnostics::analyze(entry, move |p| {
        if let Some(text) = overlay.get(p) {
            Ok(text.clone())
        } else {
            std::fs::read_to_string(p)
        }
    });

    for (path, diags) in by_file {
        let Some(uri) = diagnostics::file_path_url(&path) else {
            continue;
        };
        send_publish(connection, uri, diags)?;
    }
    Ok(())
}

fn clear_diagnostics(connection: &Connection, uri: &lsp_types::Url) -> Result<(), DynError> {
    send_publish(connection, uri.clone(), Vec::new())
}

fn send_publish(
    connection: &Connection,
    uri: lsp_types::Url,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), DynError> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    connection.sender.send(Message::Notification(Notification {
        method: PublishDiagnostics::METHOD.to_owned(),
        params: serde_json::to_value(params)?,
    }))?;
    Ok(())
}
