use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position,
    PublishDiagnosticsParams, Range, Uri,
};
use pstdr_lang::diagnostics::{Diagnostic, Severity};
use pstdr_lang::spec::BedrockSpec;

pub fn check_document(uri: &Uri, text: &str, file: &str, spec: &BedrockSpec) -> PublishDiagnosticsParams {
    let (ast, mut diags) = pstdr_lang::parse_source(text, file);
    if let Some(program) = ast {
        let mut val_diags = pstdr_lang::validate_program(&program, text, file, spec);
        diags.append(&mut val_diags);
    }

    let lsp_diags = diags.into_iter().map(to_lsp_diag).collect();
    PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: lsp_diags,
        version: None,
    }
}

fn to_lsp_diag(d: Diagnostic) -> LspDiagnostic {
    let line = d.line.saturating_sub(1) as u32;
    let col = d.column.saturating_sub(1) as u32;
    let end_col = col + d.length.max(1) as u32;

    let severity = match d.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };

    LspDiagnostic {
        range: Range {
            start: Position::new(line, col),
            end: Position::new(line, end_col),
        },
        severity: Some(severity),
        code: Some(NumberOrString::String(d.code)),
        source: Some("pstdr".to_string()),
        message: format!("{}\nHelp: {}", d.message, d.solution),
        ..Default::default()
    }
}

