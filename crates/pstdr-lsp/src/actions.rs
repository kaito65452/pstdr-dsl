use crate::diagnostics::{byte_offset_to_utf16, utf16_col_to_byte_offset};
use crate::state::ServerState;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic as LspDiagnostic,
    NumberOrString, Position, Range, TextEdit, Uri, WorkspaceEdit,
};
use std::collections::HashMap;

pub fn quick_fixes(state: &ServerState, params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let uri = &params.text_document.uri;
    let Some(text) = state.docs.get(uri) else {
        return Vec::new();
    };
    quick_fixes_for(uri, text, &params.context.diagnostics)
}

pub fn quick_fixes_for(
    uri: &Uri,
    text: &str,
    diagnostics: &[LspDiagnostic],
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        let code = match &diag.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => continue,
        };

        match code {
            "PSTDR-E001" => {
                if let Some(action) = fix_e001(uri, text, diag) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
            "PSTDR-E002" => {
                if let Some(action) = fix_e002(uri, text, diag) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
            _ => {}
        }
    }

    actions
}

#[allow(clippy::mutable_key_type)]
fn fix_e001(uri: &Uri, text: &str, diag: &LspDiagnostic) -> Option<CodeAction> {
    let line_idx = diag.range.start.line as usize;
    let line_text = text.lines().nth(line_idx)?;
    let start_b = utf16_col_to_byte_offset(line_text, diag.range.start.character);
    let end_b = utf16_col_to_byte_offset(line_text, diag.range.end.character);
    let snippet = line_text.get(start_b..end_b)?.trim();

    let (replacement, title) = match snippet {
        "let" => ("mut", "Replace 'let' with 'mut'"),
        "var" => ("mut", "Replace 'var' with 'mut'"),
        "function" => ("func", "Replace 'function' with 'func'"),
        "nil" => ("null", "Replace 'nil' with 'null'"),
        "~=" => ("!=", "Replace '~=' with '!='"),
        _ => return None,
    };

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: diag.range,
            new_text: replacement.to_string(),
        }],
    );

    Some(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        is_preferred: Some(true),
        ..Default::default()
    })
}

#[allow(clippy::mutable_key_type)]
fn fix_e002(uri: &Uri, text: &str, diag: &LspDiagnostic) -> Option<CodeAction> {
    // Validator hint format: "Change the declaration on line N to 'mut <name> = ...'"
    let decl_line_1based = extract_decl_line(&diag.message)?;
    let line_idx = decl_line_1based.saturating_sub(1);
    let line_text = text.lines().nth(line_idx)?;

    let const_byte = find_keyword_const(line_text)?;
    let start_u16 = byte_offset_to_utf16(line_text, const_byte);
    let end_u16 = byte_offset_to_utf16(line_text, const_byte + "const".len());

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range::new(
                Position::new(line_idx as u32, start_u16),
                Position::new(line_idx as u32, end_u16),
            ),
            new_text: "mut".to_string(),
        }],
    );

    Some(CodeAction {
        title: format!("Change 'const' to 'mut' on line {decl_line_1based}"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        is_preferred: Some(true),
        ..Default::default()
    })
}

fn extract_decl_line(message: &str) -> Option<usize> {
    let marker = "on line ";
    let idx = message.find(marker)?;
    let rest = &message[idx + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn find_keyword_const(line_text: &str) -> Option<usize> {
    let trimmed = line_text.trim_start();
    if trimmed.starts_with("const ") || trimmed.starts_with("const\t") {
        let indent = line_text.len() - trimmed.len();
        return Some(indent);
    }
    None
}
