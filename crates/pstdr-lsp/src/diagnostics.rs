use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position,
    PublishDiagnosticsParams, Range, Uri,
};
use pstdr_lang::diagnostics::{Diagnostic, Severity};
use pstdr_lang::spec::BedrockSpec;
use pstdr_lang::{find_workspace_root, validate_program, ValidationContext};
use std::path::Path;

/// Converts a 1-based Unicode scalar (`char`) column from `pstdr-lang` into a
/// 0-based UTF-16 code unit offset required by the LSP specification.
pub(crate) fn char_col_to_utf16(line_text: &str, col_1based: usize) -> u32 {
    let target_chars = col_1based.saturating_sub(1);
    line_text
        .chars()
        .take(target_chars)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// Converts a 0-based UTF-16 code unit offset from an LSP `Position` into a
/// safe UTF-8 byte index within `line_text`.
pub(crate) fn utf16_col_to_byte_offset(line_text: &str, utf16_col: u32) -> usize {
    let mut acc = 0u32;
    for (byte_idx, ch) in line_text.char_indices() {
        if acc >= utf16_col {
            return byte_idx;
        }
        acc += ch.len_utf16() as u32;
    }
    line_text.len()
}

/// Converts a UTF-8 byte index within `line_text` into a 0-based UTF-16 code unit offset.
pub(crate) fn byte_offset_to_utf16(line_text: &str, byte_offset: usize) -> u32 {
    let clamped = byte_offset.min(line_text.len());
    line_text[..clamped]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum()
}

pub fn check_document(
    uri: &Uri,
    text: &str,
    file: &str,
    spec: &BedrockSpec,
) -> PublishDiagnosticsParams {
    let (ast, mut diags) = pstdr_lang::parse_source(text, file);
    if let Some(program) = ast {
        let ws_root = find_workspace_root(Path::new(file));
        let ctx = ValidationContext::simple(ws_root.as_deref(), spec, true);
        let (_info, mut val_diags) = validate_program(&program, &ctx);
        diags.append(&mut val_diags);
    }

    let lsp_diags = diags.into_iter().map(|d| to_lsp_diag(d, text)).collect();
    PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: lsp_diags,
        version: None,
    }
}

pub(crate) fn to_lsp_diag(d: Diagnostic, text: &str) -> LspDiagnostic {
    let line_idx = d.line.saturating_sub(1);
    let line_text = text.lines().nth(line_idx).unwrap_or("");
    let start_col = char_col_to_utf16(line_text, d.column);
    let raw_end_exclusive = d.end_column.max(d.column) + 1;
    let end_col = char_col_to_utf16(line_text, raw_end_exclusive).max(start_col + 1);

    let severity = match d.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };

    LspDiagnostic {
        range: Range {
            start: Position::new(line_idx as u32, start_col),
            end: Position::new(line_idx as u32, end_col),
        },
        severity: Some(severity),
        code: Some(NumberOrString::String(d.code)),
        source: Some("pstdr".to_string()),
        message: format!("{}\nHelp: {}", d.message, d.solution),
        ..Default::default()
    }
}
