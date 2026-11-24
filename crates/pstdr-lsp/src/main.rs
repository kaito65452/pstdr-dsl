mod actions;
mod completion;
mod diagnostics;
mod hover;
mod state;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _},
    request::{CodeActionRequest, Completion, Formatting, HoverRequest, Request as _},
    CodeActionProviderCapability, CompletionOptions, HoverProviderCapability, OneOf,
    Position, Range, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
};
use state::ServerState;

fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".into(), " ".into(), "\"".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    };

    let _init_params = connection.initialize(serde_json::to_value(caps)?)?;
    let mut state = ServerState::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                handle_request(&connection, &state, req)?;
            }
            Message::Notification(not) => {
                handle_notification(&connection, &mut state, not)?;
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join()?;
    Ok(())
}

fn handle_notification(
    conn: &Connection,
    state: &mut ServerState,
    not: Notification,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    match not.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: lsp_types::DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
            let uri = p.text_document.uri;
            let text = p.text_document.text;
            state.docs.insert(uri.clone(), text.clone());
            publish(conn, state, &uri, &text)?;
        }
        DidChangeTextDocument::METHOD => {
            let p: lsp_types::DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
            if let Some(change) = p.content_changes.into_iter().last() {
                let uri = p.text_document.uri;
                state.docs.insert(uri.clone(), change.text.clone());
                publish(conn, state, &uri, &change.text)?;
            }
        }
        DidCloseTextDocument::METHOD => {
            let p: lsp_types::DidCloseTextDocumentParams = serde_json::from_value(not.params)?;
            state.docs.remove(&p.text_document.uri);
        }
        _ => {}
    }
    Ok(())
}

fn handle_request(
    conn: &Connection,
    state: &ServerState,
    req: Request,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let id = req.id.clone();
    let result = match req.method.as_str() {
        Completion::METHOD => {
            let p: lsp_types::CompletionParams = serde_json::from_value(req.params)?;
            let items = completion::complete(state, &p);
            serde_json::to_value(items)?
        }
        HoverRequest::METHOD => {
            let p: lsp_types::HoverParams = serde_json::from_value(req.params)?;
            serde_json::to_value(hover::hover(state, &p))?
        }
        Formatting::METHOD => {
            let p: lsp_types::DocumentFormattingParams = serde_json::from_value(req.params)?;
            let edits = format_doc(state, &p.text_document.uri);
            serde_json::to_value(edits)?
        }
        CodeActionRequest::METHOD => {
            let p: lsp_types::CodeActionParams = serde_json::from_value(req.params)?;
            serde_json::to_value(actions::quick_fixes(state, &p))?
        }
        _ => serde_json::Value::Null,
    };

    conn.sender.send(Message::Response(Response::new_ok(id, result)))?;
    Ok(())
}

fn publish(
    conn: &Connection,
    state: &ServerState,
    uri: &lsp_types::Uri,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let file = ServerState::uri_to_filename(uri);
    let params = diagnostics::check_document(uri, text, &file, &state.spec);
    let not = Notification::new(
        lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
        params,
    );
    conn.sender.send(Message::Notification(not))?;
    Ok(())
}

fn format_doc(state: &ServerState, uri: &lsp_types::Uri) -> Option<Vec<TextEdit>> {
    let text = state.docs.get(uri)?;
    let file = ServerState::uri_to_filename(uri);
    let formatted = pstdr_lang::format_source(text, &file).ok()?;
    let last_line = text.lines().count() as u32;
    Some(vec![TextEdit {
        range: Range::new(Position::new(0, 0), Position::new(last_line + 1, 0)),
        new_text: formatted,
    }])
}

