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
    let lines: Vec<&str> = text.split('\n').collect();
    let last_line_idx = lines.len().saturating_sub(1) as u32;
    let last_col_u16 = lines
        .last()
        .map(|l| l.chars().map(|c| c.len_utf16() as u32).sum())
        .unwrap_or(0);
    Some(vec![TextEdit {
        range: Range::new(Position::new(0, 0), Position::new(last_line_idx, last_col_u16)),
        new_text: formatted,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{CodeActionOrCommand, HoverContents, MarkupContent, Uri};
    use pstdr_lang::spec::BedrockSpec;
    use std::str::FromStr;

    #[test]
    fn utf16_column_conversion_with_section_sign_and_emoji() {
        // '§' is 2 UTF-8 bytes, 1 char, 1 UTF-16 code unit.
        // '🔥' is 4 UTF-8 bytes, 1 char, 2 UTF-16 code units.
        let line = "send \"§a🔥 hi\" to player";
        // char 1='s', ..., char 7='§', char 8='a', char 9='🔥', char 10=' '
        assert_eq!(diagnostics::char_col_to_utf16(line, 7), 6);
        assert_eq!(diagnostics::char_col_to_utf16(line, 8), 7);
        assert_eq!(diagnostics::char_col_to_utf16(line, 10), 10); // +2 for 🔥

        let byte_after_emoji = diagnostics::utf16_col_to_byte_offset(line, 10);
        assert_eq!(&line[byte_after_emoji..], " hi\" to player");
    }

    #[test]
    fn completion_effects_sounds_members_predicates_and_events() {
        let spec = BedrockSpec::bundled();

        // After `apply `
        let eff = completion::complete_at("on join of player {\n    apply ", 1, 10, &spec);
        assert!(eff.iter().any(|i| i.label == "speed"));
        assert!(eff.iter().any(|i| i.label == "slowness"));

        // Inside `play_sound("`
        let snd = completion::complete_at("on join of player {\n    player.play_sound(\"", 1, 23, &spec);
        assert!(snd.iter().any(|i| i.label == "random.orb"));

        // After `.`
        let mem = completion::complete_at("on join of player {\n    player.", 1, 11, &spec);
        assert!(mem.iter().any(|i| i.label == "actionbar"));
        assert!(mem.iter().any(|i| i.label == "spawn_particle"));
        assert!(mem.iter().any(|i| i.label == "teleport"));

        // After `is `
        let pred = completion::complete_at("on join of player {\n    if player is ", 1, 17, &spec);
        assert!(pred.iter().any(|i| i.label == "sneaking"));
        assert!(pred.iter().any(|i| i.label == "holding"));

        // Top-level snippets include Wave 1 + Wave 2 events
        let top = completion::complete_at("", 0, 0, &spec);
        for ev in [
            "on join",
            "on leave",
            "on chat",
            "on hurt",
            "on die",
            "on consume",
            "on interact",
            "on tick",
            "on projectile hit",
            "on spawn",
            "spawn particle",
        ] {
            assert!(
                top.iter().any(|i| i.label == ev),
                "Missing top-level completion for '{ev}'"
            );
        }

        let dot = completion::complete_at("player.", 0, 7, &spec);
        for helper in ["give_item", "spawn_item", "get_score", "set_score", "add_score"] {
            assert!(
                dot.iter().any(|i| i.label == helper),
                "Missing member completion for '{helper}'"
            );
        }
    }

    #[test]
    fn hover_duration_ticks_snake_case_and_state() {
        // Duration literal hover
        let h_dur = hover::hover_at("after 3s {}", 0, 7).expect("duration hover");
        if let HoverContents::Markup(MarkupContent { value, .. }) = h_dur.contents {
            assert!(value.contains("60 ticks"), "got: {value}");
        } else {
            panic!("expected markup hover");
        }

        // snake_case method hover
        let h_meth = hover::hover_at("player.actionbar(\"hi\")", 0, 10).expect("method hover");
        if let HoverContents::Markup(MarkupContent { value, .. }) = h_meth.contents {
            assert!(value.contains("onScreenDisplay.setActionBar"), "got: {value}");
        } else {
            panic!("expected markup hover");
        }

        // state hover
        let src = "state mana[player] = 100\non join of player {\n    mana[player] -= 5\n}";
        let h_state = hover::hover_at(src, 2, 5).expect("state hover");
        if let HoverContents::Markup(MarkupContent { value, .. }) = h_state.contents {
            assert!(value.contains("getDynamicProperty(\"pstdr:mana\")"), "got: {value}");
        } else {
            panic!("expected markup hover");
        }

        // give_item and scoreboard helper hovers
        let h_give = hover::hover_at("player.give_item(\"minecraft:stick\")", 0, 10)
            .expect("give_item hover");
        if let HoverContents::Markup(MarkupContent { value, .. }) = h_give.contents {
            assert!(value.contains("__pstdr_give_item"), "got: {value}");
        } else {
            panic!("expected markup hover");
        }

        let h_score = hover::hover_at("player.get_score(\"kills\")", 0, 10)
            .expect("get_score hover");
        if let HoverContents::Markup(MarkupContent { value, .. }) = h_score.contents {
            assert!(value.contains("__pstdr_get_score"), "got: {value}");
        } else {
            panic!("expected markup hover");
        }
    }

    #[test]
    fn quick_fixes_for_e001_and_e002() {
        let spec = BedrockSpec::bundled();
        let uri = Uri::from_str("file:///test.pstdr").unwrap();

        // E001: `let` -> `mut`
        let src_e001 = "on join of player {\n    let x = 1\n}\n";
        let params1 = diagnostics::check_document(&uri, src_e001, "test.pstdr", &spec);
        let fixes1 = actions::quick_fixes_for(&uri, src_e001, &params1.diagnostics);
        assert!(
            fixes1.iter().any(|a| matches!(
                a,
                CodeActionOrCommand::CodeAction(ca) if ca.title.contains("Replace 'let' with 'mut'")
            )),
            "Expected E001 quick-fix for 'let', got: {fixes1:?}"
        );

        // E002: `const` -> `mut` at declaration line
        let src_e002 = "on join of player {\n    const x = 1\n    x = 2\n}\n";
        let params2 = diagnostics::check_document(&uri, src_e002, "test.pstdr", &spec);
        let fixes2 = actions::quick_fixes_for(&uri, src_e002, &params2.diagnostics);
        assert!(
            fixes2.iter().any(|a| matches!(
                a,
                CodeActionOrCommand::CodeAction(ca) if ca.title.contains("Change 'const' to 'mut' on line 2")
            )),
            "Expected E002 quick-fix for 'const' on line 2, got: {fixes2:?}"
        );
    }
}

