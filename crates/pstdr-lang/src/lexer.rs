use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};

pub const FORBIDDEN_KEYWORDS: &[&str] = &[
    "class",
    "extends",
    "super",
    "new",
    "this",
    "constructor",
    "var",
    "let",
    "function",
    "nil",
    "then",
    "end",
    "import",
    "export",
    "require",
    "eval",
];

pub const RESERVED_KEYWORDS: &[&str] = &[
    "const",
    "mut",
    "state",
    "func",
    "unsafe",
    "bind_target",
    "on",
    "if",
    "else",
    "for",
    "while",
    "return",
    "and",
    "or",
    "not",
    "is",
    "in",
    "within",
    "null",
    "true",
    "false",
    "cancel",
    "damage",
    "ignite",
    "apply",
    "send",
    "command",
    "run_fn",
    "mcfunction",
    "after",
    "every",
];

pub fn is_forbidden_keyword(s: &str) -> bool {
    FORBIDDEN_KEYWORDS.contains(&s)
}

pub fn is_reserved_keyword(s: &str) -> bool {
    RESERVED_KEYWORDS.contains(&s)
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number(f64),
    Duration { raw: String, ms: f64, ticks: f64 },
    Distance(f64),
    StringLit(String),
    ForeignString(String),

    At,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDot,
    Question,
    QuestionDot,
    QuestionQuestion,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    FatArrow,

    EqEq,
    Bang,
    BangEq,
    TildeEq,
    AmpAmp,
    PipePipe,
    Lt,
    Le,
    Gt,
    Ge,

    Unknown(char),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub start_byte: usize,
    pub end_byte: usize,
}

pub fn tokenize(source: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    tokenize_with_offset(source, file, 1, 1)
}

pub fn tokenize_with_offset(
    source: &str,
    file: &str,
    start_line: usize,
    start_col: usize,
) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diags = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line = start_line;
    let mut col = start_col;

    let peek_char = |idx: usize| -> Option<char> { source[idx..].chars().next() };

    while i < len {
        let ch = match peek_char(i) {
            Some(c) => c,
            None => break,
        };

        if ch == ' ' || ch == '\t' || ch == '\r' {
            i += ch.len_utf8();
            col += 1;
            continue;
        }
        if ch == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }

        // Comments
        if ch == '/' && i + 1 < len && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                let c = peek_char(i).unwrap();
                i += c.len_utf8();
                col += 1;
            }
            continue;
        }
        if ch == '/' && i + 1 < len && bytes[i + 1] == b'*' {
            let s_line = line;
            let s_col = col;
            i += 2;
            col += 2;
            let mut closed = false;
            while i < len {
                if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    i += 2;
                    col += 2;
                    closed = true;
                    break;
                }
                let c = peek_char(i).unwrap();
                i += c.len_utf8();
                if c == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
            if !closed {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E016,
                    &Span::new(file, s_line, s_col, s_col + 2),
                    "Unclosed block comment '/*'.",
                    "Add '*/' to close the block comment.",
                ));
            }
            continue;
        }

        let start_byte = i;
        let tok_line = line;
        let tok_col = col;

        // String literal "..." (tracks { ... } interpolation braces)
        if ch == '"' {
            i += 1;
            col += 1;
            let inner_start = i;
            let mut brace_depth = 0usize;
            let mut closed = false;
            while i < len {
                let c = peek_char(i).unwrap();
                if c == '\\' {
                    i += 1;
                    col += 1;
                    if i < len {
                        let esc = peek_char(i).unwrap();
                        i += esc.len_utf8();
                        if esc == '\n' {
                            line += 1;
                            col = 1;
                        } else {
                            col += 1;
                        }
                    }
                    continue;
                }
                if c == '{' {
                    brace_depth += 1;
                } else if c == '}' && brace_depth > 0 {
                    brace_depth -= 1;
                } else if c == '"' && brace_depth == 0 {
                    let raw_inner = source[inner_start..i].to_string();
                    i += 1;
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::StringLit(raw_inner),
                        span: Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                        start_byte,
                        end_byte: i,
                    });
                    closed = true;
                    break;
                } else if c == '\n' && brace_depth == 0 {
                    break;
                }
                i += c.len_utf8();
                if c == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
            if !closed {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E016,
                    &Span::new(file, tok_line, tok_col, tok_col),
                    "Unclosed string literal '\"'.",
                    "Add a closing '\"' quote on the same line.",
                ));
            }
            continue;
        }

        // Single-quoted or backtick strings (legal inside `unsafe`, rejected outside)
        if ch == '\'' || ch == '`' {
            let quote = ch;
            i += 1;
            col += 1;
            let inner_start = i;
            while i < len {
                let c = peek_char(i).unwrap();
                if c == '\\' {
                    i += 1;
                    col += 1;
                    if i < len {
                        let esc = peek_char(i).unwrap();
                        i += esc.len_utf8();
                        if esc == '\n' {
                            line += 1;
                            col = 1;
                        } else {
                            col += 1;
                        }
                    }
                    continue;
                }
                if c == quote {
                    let raw_inner = source[inner_start..i].to_string();
                    i += 1;
                    col += 1;
                    tokens.push(Token {
                        kind: TokenKind::ForeignString(raw_inner),
                        span: Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                        start_byte,
                        end_byte: i,
                    });
                    break;
                }
                i += c.len_utf8();
                if c == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
            continue;
        }

        // Numbers + unit literals (50ms, 3s, 2m, 5b, 5blocks)
        if ch.is_ascii_digit() {
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            if i + 1 < len && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                i += 1;
                col += 1;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                    col += 1;
                }
            }
            let num_str = &source[start_byte..i];
            let val: f64 = match num_str.parse() {
                Ok(v) => v,
                Err(_) => {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E016,
                        &Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                        format!("Invalid number '{num_str}'."),
                        "Use standard decimal number format, e.g. 5 or 1.5.",
                    ));
                    continue;
                }
            };

            let suf_start = i;
            while i < len {
                let c = peek_char(i).unwrap();
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += c.len_utf8();
                    col += 1;
                } else {
                    break;
                }
            }
            let suffix = &source[suf_start..i];
            let kind = match suffix {
                "" => TokenKind::Number(val),
                "ms" => TokenKind::Duration {
                    raw: source[start_byte..i].to_string(),
                    ms: val,
                    ticks: val / 50.0,
                },
                "s" => TokenKind::Duration {
                    raw: source[start_byte..i].to_string(),
                    ms: val * 1000.0,
                    ticks: val * 20.0,
                },
                "m" => TokenKind::Duration {
                    raw: source[start_byte..i].to_string(),
                    ms: val * 60_000.0,
                    ticks: val * 1200.0,
                },
                "b" | "blocks" => TokenKind::Distance(val),
                other => {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E016,
                        &Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                        format!("Unknown unit suffix '{other}' on '{num_str}{other}'."),
                        "Use time units (ms, s, m) or distance units (b, blocks).",
                    ));
                    continue;
                }
            };

            tokens.push(Token {
                kind,
                span: Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                start_byte,
                end_byte: i,
            });
            continue;
        }

        // Identifiers & keywords
        if ch.is_ascii_alphabetic() || ch == '_' {
            while i < len {
                let c = peek_char(i).unwrap();
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += c.len_utf8();
                    col += 1;
                } else {
                    break;
                }
            }
            let word = source[start_byte..i].to_string();
            tokens.push(Token {
                kind: TokenKind::Ident(word),
                span: Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
                start_byte,
                end_byte: i,
            });
            continue;
        }

        // Operators & punctuation
        let next = if i + 1 < len {
            Some(bytes[i + 1] as char)
        } else {
            None
        };

        let (kind, advance) = match (ch, next) {
            ('?', Some('?')) => (TokenKind::QuestionQuestion, 2),
            ('?', Some('.')) => (TokenKind::QuestionDot, 2),
            ('?', _) => (TokenKind::Question, 1),
            ('.', Some('.')) => (TokenKind::DotDot, 2),
            ('.', _) => (TokenKind::Dot, 1),
            ('~', Some('=')) => (TokenKind::TildeEq, 2),
            ('&', Some('&')) => (TokenKind::AmpAmp, 2),
            ('|', Some('|')) => (TokenKind::PipePipe, 2),
            ('=', Some('=')) => (TokenKind::EqEq, 2),
            ('=', Some('>')) => (TokenKind::FatArrow, 2),
            ('=', _) => (TokenKind::Eq, 1),
            ('+', Some('=')) => (TokenKind::PlusEq, 2),
            ('+', _) => (TokenKind::Plus, 1),
            ('-', Some('=')) => (TokenKind::MinusEq, 2),
            ('-', _) => (TokenKind::Minus, 1),
            ('*', Some('=')) => (TokenKind::StarEq, 2),
            ('*', _) => (TokenKind::Star, 1),
            ('/', Some('=')) => (TokenKind::SlashEq, 2),
            ('/', _) => (TokenKind::Slash, 1),
            ('%', _) => (TokenKind::Percent, 1),
            ('!', Some('=')) => (TokenKind::BangEq, 2),
            ('!', _) => (TokenKind::Bang, 1),
            ('<', Some('=')) => (TokenKind::Le, 2),
            ('<', _) => (TokenKind::Lt, 1),
            ('>', Some('=')) => (TokenKind::Ge, 2),
            ('>', _) => (TokenKind::Gt, 1),
            ('@', _) => (TokenKind::At, 1),
            ('(', _) => (TokenKind::LParen, 1),
            (')', _) => (TokenKind::RParen, 1),
            ('{', _) => (TokenKind::LBrace, 1),
            ('}', _) => (TokenKind::RBrace, 1),
            ('[', _) => (TokenKind::LBracket, 1),
            (']', _) => (TokenKind::RBracket, 1),
            (',', _) => (TokenKind::Comma, 1),
            (':', _) => (TokenKind::Colon, 1),
            (';', _) => (TokenKind::Semi, 1),
            (other, _) => (TokenKind::Unknown(other), other.len_utf8()),
        };

        let tok_cols = if advance == 2 && ch.len_utf8() == 1 {
            2
        } else {
            1
        };
        i += advance;
        col += tok_cols;
        tokens.push(Token {
            kind,
            span: Span::new(file, tok_line, tok_col, col.saturating_sub(1)),
            start_byte,
            end_byte: i,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(file, line, col, col),
        start_byte: len,
        end_byte: len,
    });

    (tokens, diags)
}
