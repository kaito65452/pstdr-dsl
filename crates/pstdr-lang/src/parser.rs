use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};
use crate::lexer::{is_forbidden_keyword, is_reserved_keyword, tokenize_with_offset, Token, TokenKind};

pub fn parse_source(source: &str, file: &str) -> (Option<Program>, Vec<Diagnostic>) {
    let (tokens, lex_diags) = tokenize_with_offset(source, file, 1, 1);
    if !lex_diags.is_empty() {
        return (None, lex_diags);
    }
    let mut parser = Parser {
        source,
        file: file.to_string(),
        tokens,
        pos: 0,
    };
    match parser.parse_program() {
        Ok(prog) => (Some(prog), Vec::new()),
        Err(diag) => (None, vec![*diag]),
    }
}

struct Parser<'a> {
    source: &'a str,
    file: String,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn prev_line(&self) -> usize {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span.line
        } else {
            1
        }
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn is_ident_word(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(s) if s == word)
    }

    fn check_not_forbidden(&self, tok: &Token) -> Result<(), Box<Diagnostic>> {
        match &tok.kind {
            TokenKind::Ident(s) if is_forbidden_keyword(s) => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E001,
                &tok.span,
                format!("Kata kunci terlarang '{s}' ditemukan di mode aman."),
                "Ganti dengan padanan .pstdr (const/mut/func/null/{ ... }) atau pindahkan ke fungsi 'unsafe'.",
            ))),
            TokenKind::TildeEq => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E001,
                &tok.span,
                "Kata kunci terlarang '~=' ditemukan di mode aman.",
                "Gunakan operator '!=' sebagai pengganti '~='.",
            ))),
            TokenKind::DotDot => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E001,
                &tok.span,
                "Kata kunci terlarang '..' ditemukan di mode aman.",
                "Gunakan interpolasi string \"{a}{b}\" sebagai pengganti '..'.",
            ))),
            TokenKind::ForeignString(_) => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                "Tanda kutip tunggal/backtick hanya diizinkan di dalam blok 'unsafe'.",
                "Gunakan tanda kutip ganda \"...\" dengan interpolasi \"{expr}\".",
            ))),
            TokenKind::Unknown(c) => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                format!("Karakter tidak dikenal '{c}'."),
                "Hapus karakter tersebut atau pindahkan kode mentah ke dalam fungsi 'unsafe'.",
            ))),
            _ => Ok(()),
        }
    }

    fn expect_ident_any(&mut self, ctx: &str) -> Result<(String, Span), Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;
        if let TokenKind::Ident(name) = tok.kind {
            self.advance();
            Ok((name, tok.span))
        } else {
            Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                format!("Diharapkan identifier untuk {ctx}."),
                "Periksa kembali sintaks pada baris ini.",
            )))
        }
    }

    fn expect_user_ident(&mut self, ctx: &str) -> Result<(String, Span), Box<Diagnostic>> {
        let (name, span) = self.expect_ident_any(ctx)?;
        if is_reserved_keyword(&name) {
            return Err(Box::new(Diagnostic::error(
                DiagnosticCode::E003,
                &span,
                format!("'{name}' adalah kata kunci cadangan dan tidak boleh dipakai sebagai identifier."),
                format!("Ganti '{name}' dengan nama lain yang tidak masuk daftar reserved keywords."),
            )));
        }
        Ok((name, span))
    }

    fn expect_word(&mut self, word: &str) -> Result<Span, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;
        if let TokenKind::Ident(s) = &tok.kind {
            if s == word {
                self.advance();
                return Ok(tok.span);
            }
        }
        Err(Box::new(Diagnostic::error(
            DiagnosticCode::E016,
            &tok.span,
            format!("Diharapkan kata kunci '{word}'."),
            format!("Tambahkan '{word}' sesuai struktur kalimat .pstdr."),
        )))
    }

    fn expect_kind(&mut self, expected: &TokenKind, label: &str) -> Result<Token, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;
        if &tok.kind == expected {
            Ok(self.advance())
        } else {
            Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                format!("Diharapkan '{label}'."),
                format!("Periksa tanda '{label}' pada baris {}.", tok.span.line),
            )))
        }
    }

    fn consume_optional_semi(&mut self) {
        if matches!(self.peek().kind, TokenKind::Semi) {
            self.advance();
        }
    }

    fn parse_program(&mut self) -> Result<Program, Box<Diagnostic>> {
        let mut header = None;
        let mut bind_targets = Vec::new();
        let mut items = Vec::new();

        if matches!(self.peek().kind, TokenKind::At) {
            header = Some(self.parse_addon_directive()?);
        }

        while !self.is_eof() {
            let tok = self.peek().clone();
            self.check_not_forbidden(&tok)?;

            if matches!(tok.kind, TokenKind::At) {
                if header.is_none() {
                    header = Some(self.parse_addon_directive()?);
                    continue;
                } else {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &tok.span,
                        "Direktif '@addon' ganda tidak diizinkan.",
                        "Sisakan hanya satu '@addon(...)' di bagian paling atas file.",
                    )));
                }
            }

            if self.is_ident_word("bind_target") {
                bind_targets.push(self.parse_bind_target()?);
                continue;
            }

            items.push(self.parse_top_level_item()?);
        }

        Ok(Program {
            header,
            bind_targets,
            items,
        })
    }

    fn parse_addon_directive(&mut self) -> Result<AddonDirective, Box<Diagnostic>> {
        let at_tok = self.advance();
        self.expect_word("addon")?;
        self.expect_kind(&TokenKind::LParen, "(")?;
        // Default versi mengacu ke konstanta terpusat dari spec bundled.
        // Tidak dihardcode secara kaku; ambil dari JSON spec yang disertakan saat kompilasi.
        let (default_target, default_api) = {
            let s = crate::spec::BedrockSpec::bundled();
            (s.bedrock_version, s.script_api_version)
        };
        let mut target = default_target;
        let mut api = default_api;

        while !matches!(self.peek().kind, TokenKind::RParen | TokenKind::Eof) {
            let (key, key_span) = self.expect_ident_any("atribut @addon")?;
            self.expect_kind(&TokenKind::Eq, "=")?;
            let val_tok = self.peek().clone();
            let val = match val_tok.kind {
                TokenKind::StringLit(s) => {
                    self.advance();
                    s
                }
                _ => {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &val_tok.span,
                        "Nilai atribut @addon harus berupa string literal.",
                        "Contoh: @addon(target = \"1.21.50\", api = \"1.16.0\")",
                    )));
                }
            };
            match key.as_str() {
                "target" => target = val,
                "api" => api = val,
                _ => {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &key_span,
                        format!("Atribut @addon tidak dikenal '{key}'."),
                        "Atribut yang didukung hanya 'target' dan 'api'.",
                    )));
                }
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_kind(&TokenKind::RParen, ")")?;
        self.consume_optional_semi();
        Ok(AddonDirective {
            target,
            api,
            span: at_tok.span,
        })
    }

    fn parse_bind_target(&mut self) -> Result<BindTargetStmt, Box<Diagnostic>> {
        let bt_span = self.expect_word("bind_target")?;
        self.expect_kind(&TokenKind::LParen, "(")?;
        let tok = self.peek().clone();
        let path = match tok.kind {
            TokenKind::StringLit(s) => {
                self.advance();
                s
            }
            _ => {
                return Err(Box::new(Diagnostic::error(
                    DiagnosticCode::E016,
                    &tok.span,
                    "Argumen bind_target() harus berupa string path relatif.",
                    "Contoh: bind_target(\"BP/items/frost_blade.json\")",
                )));
            }
        };
        self.expect_kind(&TokenKind::RParen, ")")?;
        self.consume_optional_semi();
        Ok(BindTargetStmt { path, span: bt_span })
    }

    fn parse_top_level_item(&mut self) -> Result<TopLevelItem, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;

        if self.is_ident_word("const") {
            return Ok(TopLevelItem::ConstDecl(self.parse_var_decl(true)?));
        }
        if self.is_ident_word("mut") {
            return Ok(TopLevelItem::MutDecl(self.parse_var_decl(false)?));
        }
        if self.is_ident_word("state") {
            return Ok(TopLevelItem::StateDecl(self.parse_state_decl()?));
        }
        if self.is_ident_word("func") {
            return Ok(TopLevelItem::FuncDecl(self.parse_func_decl()?));
        }
        if self.is_ident_word("unsafe") {
            return self.parse_unsafe_top_level();
        }
        if self.is_ident_word("on") {
            return Ok(TopLevelItem::EventHandler(self.parse_event_handler()?));
        }
        if self.is_ident_word("after") {
            let start = self.advance().span;
            let duration = self.parse_duration_lit()?;
            let body = self.parse_block()?;
            return Ok(TopLevelItem::AfterTimer {
                duration,
                body,
                span: start,
            });
        }
        if self.is_ident_word("every") {
            let start = self.advance().span;
            let duration = self.parse_duration_lit()?;
            let body = self.parse_block()?;
            return Ok(TopLevelItem::EveryTimer {
                duration,
                body,
                span: start,
            });
        }

        Err(Box::new(Diagnostic::error(
            DiagnosticCode::E016,
            &tok.span,
            "Deklarasi level atas tidak dikenal.",
            "Gunakan 'const', 'mut', 'state', 'func', 'unsafe', 'on', 'after', atau 'every'.",
        )))
    }

    fn parse_var_decl(&mut self, is_const: bool) -> Result<VarDecl, Box<Diagnostic>> {
        let kw = self.advance();
        let (name, name_span) =
            self.expect_user_ident(if is_const { "konstanta" } else { "variabel mut" })?;
        self.expect_kind(&TokenKind::Eq, "=")?;
        let value = self.parse_expr(true)?;
        self.consume_optional_semi();
        Ok(VarDecl {
            name,
            name_span,
            value,
            span: kw.span,
        })
    }

    fn parse_state_decl(&mut self) -> Result<StateDecl, Box<Diagnostic>> {
        let kw = self.advance();
        let (name, _) = self.expect_user_ident("state")?;
        self.expect_kind(&TokenKind::LBracket, "[")?;
        let (owner_hint, _) = self.expect_user_ident("owner state")?;
        self.expect_kind(&TokenKind::RBracket, "]")?;
        self.expect_kind(&TokenKind::Eq, "=")?;
        let default_value = self.parse_expr(true)?;
        self.consume_optional_semi();
        Ok(StateDecl {
            name,
            owner_hint,
            default_value,
            span: kw.span,
        })
    }

    fn parse_func_decl(&mut self) -> Result<FuncDecl, Box<Diagnostic>> {
        let kw = self.advance();
        let (name, _) = self.expect_user_ident("nama fungsi")?;
        self.expect_kind(&TokenKind::LParen, "(")?;
        let mut params = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RParen | TokenKind::Eof) {
            params.push(self.expect_user_ident("parameter fungsi")?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_kind(&TokenKind::RParen, ")")?;
        let body = self.parse_block()?;
        Ok(FuncDecl {
            name,
            params,
            body,
            span: kw.span,
        })
    }

    fn parse_unsafe_top_level(&mut self) -> Result<TopLevelItem, Box<Diagnostic>> {
        let unsafe_tok = self.advance();

        if matches!(self.peek().kind, TokenKind::LBrace) {
            return Err(Box::new(Diagnostic::error(
                DiagnosticCode::E006,
                &unsafe_tok.span,
                "'unsafe' tidak boleh dipakai sebagai blok anonim atau nilai ekspresi.",
                "Deklarasikan sebagai 'unsafe nama(param) { ... }' atau 'unsafe const/mut x = ...;'.",
            )));
        }

        // Single-line unsafe variable: unsafe const/mut/var/let ...
        if let TokenKind::Ident(kw) = &self.peek().kind {
            if matches!(kw.as_str(), "const" | "mut" | "var" | "let") {
                let kw_str = kw.clone();
                self.advance();
                let name_tok = self.peek().clone();
                let name = match &name_tok.kind {
                    TokenKind::Ident(n) => n.clone(),
                    _ => {
                        return Err(Box::new(Diagnostic::error(
                            DiagnosticCode::E016,
                            &name_tok.span,
                            "Diharapkan nama variabel setelah 'unsafe const/mut/var/let'.",
                            "Contoh: unsafe const RAW_WORLD = world;",
                        )));
                    }
                };
                let rest_start = name_tok.start_byte;
                let (raw_rest, end_byte) = scan_js_statement_end(self.source, rest_start);
                self.skip_tokens_before(end_byte);
                let js_kw = if kw_str == "mut" { "let" } else { kw_str.as_str() };
                return Ok(TopLevelItem::UnsafeVarDecl(UnsafeVarDecl {
                    keyword: js_kw.to_string(),
                    name,
                    raw_rest,
                    span: unsafe_tok.span,
                }));
            }
        }

        // Unsafe function: unsafe nama(p1, p2) { ...raw JS/TS... }
        let (name, _) = self.expect_user_ident("nama fungsi unsafe")?;
        self.expect_kind(&TokenKind::LParen, "(")?;
        let mut params = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RParen | TokenKind::Eof) {
            let (p, _) = self.expect_ident_any("parameter unsafe")?;
            params.push(p);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_kind(&TokenKind::RParen, ")")?;
        let lbrace = self.expect_kind(&TokenKind::LBrace, "{")?;
        let (raw_body, close_end_byte) =
            scan_js_brace_block(self.source, lbrace.end_byte, &lbrace.span)?;
        self.skip_tokens_before(close_end_byte);

        Ok(TopLevelItem::UnsafeFuncDecl(UnsafeFuncDecl {
            name,
            params,
            raw_body,
            span: unsafe_tok.span,
        }))
    }

    fn skip_tokens_before(&mut self, byte_limit: usize) {
        while !self.is_eof() && self.peek().start_byte < byte_limit {
            self.advance();
        }
    }

    fn parse_event_handler(&mut self) -> Result<EventHandler, Box<Diagnostic>> {
        let on_tok = self.advance();
        let kind_tok = self.peek().clone();
        self.check_not_forbidden(&kind_tok)?;

        let kind = match &kind_tok.kind {
            TokenKind::Ident(w) if w == "right" => {
                self.advance();
                self.expect_word("click")?;
                self.expect_word("with")?;
                let item = self.parse_expr(false)?;
                self.expect_word("by")?;
                let (actor_var, _) = self.expect_user_ident("aktor event")?;
                EventKind::RightClick { item, actor_var }
            }
            TokenKind::Ident(w) if w == "hit" => {
                self.advance();
                let (target_var, _) = self.expect_user_ident("target event hit")?;
                self.expect_word("with")?;
                let item = self.parse_expr(false)?;
                self.expect_word("by")?;
                let (actor_var, _) = self.expect_user_ident("penyerang event hit")?;
                EventKind::HitTarget {
                    target_var,
                    item,
                    actor_var,
                }
            }
            TokenKind::Ident(w) if w == "break" => {
                self.advance();
                self.expect_word("of")?;
                let block_id = self.parse_expr(false)?;
                self.expect_word("by")?;
                let (actor_var, _) = self.expect_user_ident("pelaku event break")?;
                EventKind::BreakBlock {
                    block_id,
                    actor_var,
                }
            }
            TokenKind::Ident(w) if w == "join" => {
                self.advance();
                self.expect_word("of")?;
                let (actor_var, _) = self.expect_user_ident("pemain event join")?;
                EventKind::PlayerJoin { actor_var }
            }
            _ => {
                return Err(Box::new(Diagnostic::error(
                    DiagnosticCode::E016,
                    &kind_tok.span,
                    "Jenis event 'on' tidak dikenal.",
                    "Gunakan 'on right click with ... by player', 'on hit target with ... by player', 'on break of ... by player', atau 'on join of player'.",
                )));
            }
        };

        let body = self.parse_block()?;
        Ok(EventHandler {
            kind,
            body,
            span: on_tok.span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, Box<Diagnostic>> {
        let open_tok = self.expect_kind(&TokenKind::LBrace, "{")?;
        let mut stmts = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        if !matches!(self.peek().kind, TokenKind::RBrace) {
            return Err(Box::new(Diagnostic::error(
                DiagnosticCode::E012,
                &open_tok.span,
                format!(
                    "Kurung kurawal '{{' pada baris {} tidak memiliki penutup '}}'.",
                    open_tok.span.line
                ),
                "Tutup blok dengan '}' yang sesuai.",
            )));
        }
        self.advance();
        Ok(Block {
            stmts,
            span: open_tok.span,
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;

        if self.is_ident_word("unsafe") {
            return Err(Box::new(Diagnostic::error(
                DiagnosticCode::E006,
                &tok.span,
                "'unsafe' tidak boleh dipakai di dalam fungsi atau blok event.",
                "Deklarasikan fungsi 'unsafe nama() { ... }' di level atas file lalu panggil namanya.",
            )));
        }

        if self.is_ident_word("const") {
            return Ok(Stmt::ConstDecl(self.parse_var_decl(true)?));
        }
        if self.is_ident_word("mut") {
            return Ok(Stmt::MutDecl(self.parse_var_decl(false)?));
        }
        if self.is_ident_word("if") {
            return self.parse_if_stmt();
        }
        if self.is_ident_word("for") {
            return self.parse_for_stmt();
        }
        if self.is_ident_word("while") {
            let start = self.advance().span;
            let condition = self.parse_expr(false)?;
            let body = self.parse_block()?;
            return Ok(Stmt::While {
                condition,
                body,
                span: start,
            });
        }
        if self.is_ident_word("return") {
            let ret_tok = self.advance();
            let same_line = !self.is_eof()
                && !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Semi)
                && self.peek().span.line == ret_tok.span.line;
            let value = if same_line {
                Some(self.parse_expr(true)?)
            } else {
                None
            };
            self.consume_optional_semi();
            return Ok(Stmt::Return {
                value,
                span: ret_tok.span,
            });
        }
        if self.is_ident_word("cancel") {
            let start = self.advance().span;
            self.expect_word("event")?;
            self.consume_optional_semi();
            return Ok(Stmt::CancelEvent { span: start });
        }
        if self.is_ident_word("damage") {
            let start = self.advance().span;
            let target = self.parse_expr(true)?;
            self.expect_word("by")?;
            let amount = self.parse_expr(true)?;
            self.consume_optional_semi();
            return Ok(Stmt::Damage {
                target,
                amount,
                span: start,
            });
        }
        if self.is_ident_word("ignite") {
            let start = self.advance().span;
            let target = self.parse_expr(true)?;
            self.expect_word("for")?;
            let duration = self.parse_duration_lit()?;
            self.consume_optional_semi();
            return Ok(Stmt::Ignite {
                target,
                duration,
                span: start,
            });
        }
        if self.is_ident_word("apply") {
            let start = self.advance().span;
            let effect_tok = self.peek().clone();
            let (effect, effect_span) = match &effect_tok.kind {
                TokenKind::Ident(s) => {
                    let s = s.clone();
                    self.advance();
                    (s, effect_tok.span)
                }
                TokenKind::StringLit(s) => {
                    let s = s.clone();
                    self.advance();
                    (s, effect_tok.span)
                }
                _ => {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &effect_tok.span,
                        "Diharapkan nama efek setelah 'apply'.",
                        "Contoh: apply slowness 2 to target for 3s",
                    )));
                }
            };
            let level = self.parse_expr(true)?;
            self.expect_word("to")?;
            let target = self.parse_expr(true)?;
            self.expect_word("for")?;
            let duration = self.parse_duration_lit()?;
            self.consume_optional_semi();
            return Ok(Stmt::ApplyEffect {
                effect,
                effect_span,
                level,
                target,
                duration,
                span: start,
            });
        }
        if self.is_ident_word("send") {
            let start = self.advance().span;
            let message = self.parse_expr(true)?;
            self.expect_word("to")?;
            let target = self.parse_expr(true)?;
            self.consume_optional_semi();
            return Ok(Stmt::SendMessage {
                message,
                target,
                span: start,
            });
        }
        if self.is_ident_word("after") {
            let start = self.advance().span;
            let duration = self.parse_duration_lit()?;
            let body = self.parse_block()?;
            return Ok(Stmt::After {
                duration,
                body,
                span: start,
            });
        }
        if self.is_ident_word("every") {
            let start = self.advance().span;
            let duration = self.parse_duration_lit()?;
            let body = self.parse_block()?;
            return Ok(Stmt::Every {
                duration,
                body,
                span: start,
            });
        }

        // Expression or assignment
        let expr = self.parse_expr(true)?;
        let op = match self.peek().kind {
            TokenKind::Eq => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::AddAssign),
            TokenKind::MinusEq => Some(AssignOp::SubAssign),
            TokenKind::StarEq => Some(AssignOp::MulAssign),
            TokenKind::SlashEq => Some(AssignOp::DivAssign),
            _ => None,
        };

        if let Some(assign_op) = op {
            let op_span = self.advance().span;
            let target = match expr {
                Expr::Ident(name, s) => AssignTarget::Ident(name, s),
                Expr::Index { object, index, span } => AssignTarget::Index {
                    object: *object,
                    index: *index,
                    span,
                },
                Expr::Member {
                    object,
                    field,
                    optional: false,
                    span,
                } => AssignTarget::Field {
                    object: *object,
                    field,
                    span,
                },
                _ => {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &op_span,
                        "Target assignment tidak valid.",
                        "Assignment hanya dapat dilakukan ke variabel, elemen index, atau properti objek.",
                    )));
                }
            };
            let value = self.parse_expr(true)?;
            self.consume_optional_semi();
            return Ok(Stmt::Assign {
                target,
                op: assign_op,
                value,
                span: op_span,
            });
        }

        let span = expr.span().clone();
        self.consume_optional_semi();
        Ok(Stmt::ExprStmt { expr, span })
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, Box<Diagnostic>> {
        let start = self.advance().span;
        let condition = self.parse_expr(false)?;
        let then_block = self.parse_block()?;
        let else_branch = if self.is_ident_word("else") {
            self.advance();
            if self.is_ident_word("if") {
                Some(ElseBranch::ElseIf(Box::new(self.parse_if_stmt()?)))
            } else {
                Some(ElseBranch::Else(self.parse_block()?))
            }
        } else {
            None
        };
        Ok(Stmt::If {
            condition,
            then_block,
            else_branch,
            span: start,
        })
    }

    fn parse_for_stmt(&mut self) -> Result<Stmt, Box<Diagnostic>> {
        let start = self.advance().span;
        let (var, var_span) = self.expect_user_ident("variabel loop 'for'")?;
        self.expect_word("in")?;
        let iter = self.parse_expr(false)?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            var,
            var_span,
            iter,
            body,
            span: start,
        })
    }

    fn parse_duration_lit(&mut self) -> Result<DurationLit, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;
        match tok.kind {
            TokenKind::Duration { raw, ms, ticks } => {
                self.advance();
                Ok(DurationLit {
                    raw,
                    ms,
                    ticks,
                    span: tok.span,
                })
            }
            _ => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                "Diharapkan literal durasi waktu (mis. 50ms, 3s, 2m).",
                "Tambahkan satuan waktu 'ms', 's', atau 'm' di belakang angka.",
            ))),
        }
    }

    fn parse_expr(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        self.parse_ternary(allow_brace_obj)
    }

    fn parse_ternary(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let cond = self.parse_or(allow_brace_obj)?;
        if matches!(self.peek().kind, TokenKind::Question) {
            let q_span = self.advance().span;
            let then_expr = self.parse_expr(true)?;
            self.expect_kind(&TokenKind::Colon, ":")?;
            let else_expr = self.parse_ternary(allow_brace_obj)?;
            return Ok(Expr::Ternary {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span: q_span,
            });
        }
        Ok(cond)
    }

    fn parse_or(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_and(allow_brace_obj)?;
        while self.is_ident_word("or") || matches!(self.peek().kind, TokenKind::PipePipe) {
            let sp = self.advance().span;
            let right = self.parse_and(allow_brace_obj)?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
                span: sp,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_not(allow_brace_obj)?;
        while self.is_ident_word("and") || matches!(self.peek().kind, TokenKind::AmpAmp) {
            let sp = self.advance().span;
            let right = self.parse_not(allow_brace_obj)?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
                span: sp,
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        if self.is_ident_word("not") {
            let sp = self.advance().span;
            let expr = self.parse_not(allow_brace_obj)?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
                span: sp,
            });
        }
        self.parse_comparison(allow_brace_obj)
    }

    fn parse_comparison(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_nullish(allow_brace_obj)?;
        loop {
            let tok = self.peek().clone();
            self.check_not_forbidden(&tok)?;

            if self.is_ident_word("is") {
                let sp = self.advance().span;
                let negated = if self.is_ident_word("not") {
                    self.advance();
                    true
                } else {
                    false
                };
                let predicate = self.parse_is_predicate(allow_brace_obj)?;
                left = Expr::IsCheck {
                    subject: Box::new(left),
                    negated,
                    predicate,
                    span: sp,
                };
                continue;
            }

            if self.is_ident_word("in") {
                let sp = self.advance().span;
                let right = self.parse_nullish(allow_brace_obj)?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op: BinaryOp::In,
                    right: Box::new(right),
                    span: sp,
                };
                continue;
            }

            if self.is_ident_word("within") {
                let sp = self.advance().span;
                let right = self.parse_nullish(allow_brace_obj)?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op: BinaryOp::Within,
                    right: Box::new(right),
                    span: sp,
                };
                continue;
            }

            let op = match tok.kind {
                TokenKind::EqEq => Some(BinaryOp::Eq),
                TokenKind::BangEq => Some(BinaryOp::Neq),
                TokenKind::Lt => Some(BinaryOp::Lt),
                TokenKind::Le => Some(BinaryOp::Le),
                TokenKind::Gt => Some(BinaryOp::Gt),
                TokenKind::Ge => Some(BinaryOp::Ge),
                _ => None,
            };
            if let Some(bop) = op {
                let sp = self.advance().span;
                let right = self.parse_nullish(allow_brace_obj)?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op: bop,
                    right: Box::new(right),
                    span: sp,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_is_predicate(&mut self, allow_brace_obj: bool) -> Result<IsPredicate, Box<Diagnostic>> {
        let (word, span) = self.expect_ident_any("predikat 'is'")?;
        match word.as_str() {
            "sneaking" => Ok(IsPredicate::Sneaking),
            "sprinting" => Ok(IsPredicate::Sprinting),
            "swimming" => Ok(IsPredicate::Swimming),
            "flying" => Ok(IsPredicate::Flying),
            "sleeping" => Ok(IsPredicate::Sleeping),
            "on_ground" => Ok(IsPredicate::OnGround),
            "in_water" => Ok(IsPredicate::InWater),
            "on" => {
                self.expect_word("ground")?;
                Ok(IsPredicate::OnGround)
            }
            "in" => {
                self.expect_word("water")?;
                Ok(IsPredicate::InWater)
            }
            "holding" => {
                let item = self.parse_nullish(allow_brace_obj)?;
                Ok(IsPredicate::Holding(Box::new(item)))
            }
            _ => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &span,
                format!("Predikat 'is {word}' tidak dikenal."),
                "Gunakan sneaking, sprinting, swimming, flying, sleeping, on_ground, in_water, atau holding <item>.",
            ))),
        }
    }

    fn parse_nullish(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_additive(allow_brace_obj)?;
        while matches!(self.peek().kind, TokenKind::QuestionQuestion) {
            let sp = self.advance().span;
            let right = self.parse_additive(allow_brace_obj)?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Nullish,
                right: Box::new(right),
                span: sp,
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_multiplicative(allow_brace_obj)?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                _ => None,
            };
            if let Some(bop) = op {
                let sp = self.advance().span;
                let right = self.parse_multiplicative(allow_brace_obj)?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op: bop,
                    right: Box::new(right),
                    span: sp,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut left = self.parse_unary(allow_brace_obj)?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => Some(BinaryOp::Mul),
                TokenKind::Slash => Some(BinaryOp::Div),
                TokenKind::Percent => Some(BinaryOp::Mod),
                _ => None,
            };
            if let Some(bop) = op {
                let sp = self.advance().span;
                let right = self.parse_unary(allow_brace_obj)?;
                left = Expr::Binary {
                    left: Box::new(left),
                    op: bop,
                    right: Box::new(right),
                    span: sp,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        match self.peek().kind {
            TokenKind::Minus => {
                let sp = self.advance().span;
                let expr = self.parse_unary(allow_brace_obj)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                    span: sp,
                })
            }
            TokenKind::Bang => {
                let sp = self.advance().span;
                let expr = self.parse_unary(allow_brace_obj)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                    span: sp,
                })
            }
            _ => self.parse_postfix(allow_brace_obj),
        }
    }

    fn parse_postfix(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let mut expr = self.parse_primary(allow_brace_obj)?;
        loop {
            let tok = self.peek().clone();
            match tok.kind {
                TokenKind::Dot | TokenKind::QuestionDot => {
                    let optional = matches!(tok.kind, TokenKind::QuestionDot);
                    let sp = self.advance().span;
                    let (field, _) = self.expect_ident_any("nama properti/method")?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        field,
                        optional,
                        span: sp,
                    };
                }
                TokenKind::LBracket if tok.span.line == self.prev_line() => {
                    let sp = self.advance().span;
                    let index = self.parse_expr(true)?;
                    self.expect_kind(&TokenKind::RBracket, "]")?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                        span: sp,
                    };
                }
                TokenKind::LParen if tok.span.line == self.prev_line() => {
                    let sp = self.advance().span;
                    let mut args = Vec::new();
                    while !matches!(self.peek().kind, TokenKind::RParen | TokenKind::Eof) {
                        args.push(self.parse_expr(true)?);
                        if matches!(self.peek().kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect_kind(&TokenKind::RParen, ")")?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        span: sp,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self, allow_brace_obj: bool) -> Result<Expr, Box<Diagnostic>> {
        let tok = self.peek().clone();
        self.check_not_forbidden(&tok)?;

        match tok.kind {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(n, tok.span))
            }
            TokenKind::Duration { raw, ms, ticks } => {
                self.advance();
                Ok(Expr::Duration(DurationLit {
                    raw,
                    ms,
                    ticks,
                    span: tok.span,
                }))
            }
            TokenKind::Distance(b) => {
                self.advance();
                Ok(Expr::Distance(b, tok.span))
            }
            TokenKind::StringLit(raw_inner) => {
                self.advance();
                let parts = parse_string_parts(&raw_inner, &tok.span)?;
                Ok(Expr::StringParts(parts, tok.span))
            }
            TokenKind::Ident(s) => {
                if s == "unsafe" {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E006,
                        &tok.span,
                        "'unsafe' tidak boleh dipakai sebagai nilai ekspresi.",
                        "Deklarasikan sebagai fungsi top-level 'unsafe nama() { ... }' lalu panggil namanya.",
                    )));
                }
                if s == "true" {
                    self.advance();
                    return Ok(Expr::Bool(true, tok.span));
                }
                if s == "false" {
                    self.advance();
                    return Ok(Expr::Bool(false, tok.span));
                }
                if s == "null" {
                    self.advance();
                    return Ok(Expr::Null(tok.span));
                }
                if is_reserved_keyword(&s) {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E016,
                        &tok.span,
                        format!("Kata kunci '{s}' tidak dapat dipakai sebagai ekspresi di sini."),
                        "Periksa struktur ekspresi atau nama variabel Anda.",
                    )));
                }
                self.advance();
                Ok(Expr::Ident(s, tok.span))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr(true)?;
                let close = self.expect_kind(&TokenKind::RParen, ")")?;
                let sp = Span::new(&self.file, tok.span.line, tok.span.column, close.span.end_column);
                Ok(Expr::Paren(Box::new(inner), sp))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBracket | TokenKind::Eof) {
                    items.push(self.parse_expr(true)?);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect_kind(&TokenKind::RBracket, "]")?;
                Ok(Expr::Array(items, tok.span))
            }
            TokenKind::LBrace if allow_brace_obj => {
                self.advance();
                let mut fields = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
                    let key_tok = self.peek().clone();
                    self.check_not_forbidden(&key_tok)?;
                    let (key, key_is_string) = match key_tok.kind {
                        TokenKind::Ident(k) => {
                            self.advance();
                            (k, false)
                        }
                        TokenKind::StringLit(k) => {
                            self.advance();
                            (k, true)
                        }
                        _ => {
                            return Err(Box::new(Diagnostic::error(
                                DiagnosticCode::E016,
                                &key_tok.span,
                                "Diharapkan nama properti objek.",
                                "Contoh: { key = value } atau { key: value }",
                            )));
                        }
                    };
                    if matches!(self.peek().kind, TokenKind::Eq | TokenKind::Colon) {
                        self.advance();
                    } else {
                        return Err(Box::new(Diagnostic::error(
                            DiagnosticCode::E016,
                            &self.peek().span,
                            "Diharapkan '=' atau ':' setelah key objek.",
                            "Contoh: { amplifier = 1 } atau { amplifier: 1 }",
                        )));
                    }
                    let value = self.parse_expr(true)?;
                    fields.push(ObjectField {
                        key,
                        key_is_string,
                        value,
                    });
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                if !matches!(self.peek().kind, TokenKind::RBrace) {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E012,
                        &tok.span,
                        format!(
                            "Kurung kurawal '{{' pada baris {} tidak memiliki penutup '}}'.",
                            tok.span.line
                        ),
                        "Tutup objek konfigurasi dengan '}'.",
                    )));
                }
                self.advance();
                Ok(Expr::Object(fields, tok.span))
            }
            _ => Err(Box::new(Diagnostic::error(
                DiagnosticCode::E016,
                &tok.span,
                "Diharapkan ekspresi.",
                "Periksa nilai atau operator sebelum posisi ini.",
            ))),
        }
    }
}

fn parse_string_parts(raw_inner: &str, str_span: &Span) -> Result<Vec<StringPart>, Box<Diagnostic>> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let chars: Vec<(usize, char)> = raw_inner.char_indices().collect();
    let mut idx = 0usize;

    while idx < chars.len() {
        let (_, c) = chars[idx];
        if c == '\\' {
            idx += 1;
            if idx < chars.len() {
                let (_, esc) = chars[idx];
                match esc {
                    'n' => lit.push('\n'),
                    'r' => lit.push('\r'),
                    't' => lit.push('\t'),
                    '\\' => lit.push('\\'),
                    '"' => lit.push('"'),
                    '{' => lit.push('{'),
                    '}' => lit.push('}'),
                    other => {
                        lit.push('\\');
                        lit.push(other);
                    }
                }
                idx += 1;
            }
            continue;
        }

        if c == '{' {
            if !lit.is_empty() {
                parts.push(StringPart::Literal(std::mem::take(&mut lit)));
            }
            idx += 1;
            let expr_start = if idx < chars.len() {
                chars[idx].0
            } else {
                raw_inner.len()
            };
            let mut depth = 1usize;
            let mut in_quote = false;
            let mut closed_byte = None;

            while idx < chars.len() {
                let (bpos, ch) = chars[idx];
                if ch == '\\' && in_quote {
                    idx += 2;
                    continue;
                }
                if ch == '"' {
                    in_quote = !in_quote;
                } else if !in_quote {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            closed_byte = Some(bpos);
                            idx += 1;
                            break;
                        }
                    }
                }
                idx += 1;
            }

            let expr_end = match closed_byte {
                Some(e) => e,
                None => {
                    return Err(Box::new(Diagnostic::error(
                        DiagnosticCode::E012,
                        str_span,
                        format!(
                            "Kurung kurawal '{{' pada interpolasi string baris {} tidak ditutup '}}'.",
                            str_span.line
                        ),
                        "Tutup ekspresi interpolasi dengan '}' atau escape dengan '\\{'.",
                    )));
                }
            };

            let inner_expr_src = &raw_inner[expr_start..expr_end];
            if inner_expr_src.trim().is_empty() {
                return Err(Box::new(Diagnostic::error(
                    DiagnosticCode::E016,
                    str_span,
                    "Ekspresi interpolasi string '{}' kosong.",
                    "Isi ekspresi di dalam '{...}' atau escape menjadi '\\{\\}'.",
                )));
            }
            let (toks, lex_diags) = tokenize_with_offset(
                inner_expr_src,
                &str_span.file,
                str_span.line,
                str_span.column + 1,
            );
            if let Some(d) = lex_diags.into_iter().next() {
                return Err(Box::new(d));
            }
            let mut sub_parser = Parser {
                source: inner_expr_src,
                file: str_span.file.clone(),
                tokens: toks,
                pos: 0,
            };
            let expr = sub_parser.parse_expr(true)?;
            if !sub_parser.is_eof() {
                return Err(Box::new(Diagnostic::error(
                    DiagnosticCode::E016,
                    &sub_parser.peek().span,
                    "Token sisa di dalam ekspresi interpolasi string.",
                    "Pastikan isi '{...}' hanya berupa satu ekspresi valid.",
                )));
            }
            parts.push(StringPart::Expr(expr));
            continue;
        }

        lit.push(c);
        idx += 1;
    }

    if !lit.is_empty() || parts.is_empty() {
        parts.push(StringPart::Literal(lit));
    }
    Ok(parts)
}

fn scan_js_brace_block(
    source: &str,
    body_start_byte: usize,
    open_span: &Span,
) -> Result<(String, usize), Box<Diagnostic>> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = body_start_byte;
    let mut depth = 1usize;

    while i < len {
        let b = bytes[i];
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
            continue;
        }
        if b == b'"' || b == b'\'' || b == b'`' {
            let q = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(len);
                    continue;
                }
                if q == b'`' && bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                    i += 2;
                    let mut tpl_depth = 1usize;
                    while i < len && tpl_depth > 0 {
                        if bytes[i] == b'{' {
                            tpl_depth += 1;
                        } else if bytes[i] == b'}' {
                            tpl_depth -= 1;
                        }
                        i += 1;
                    }
                    continue;
                }
                if bytes[i] == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                let raw = source[body_start_byte..i].to_string();
                return Ok((raw, i + 1));
            }
        }
        i += 1;
    }

    Err(Box::new(Diagnostic::error(
        DiagnosticCode::E012,
        open_span,
        format!(
            "Kurung kurawal '{{' pada baris {} tidak memiliki penutup '}}'.",
            open_span.line
        ),
        "Tutup blok 'unsafe' dengan '}' yang sesuai.",
    )))
}

fn scan_js_statement_end(source: &str, start_byte: usize) -> (String, usize) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = start_byte;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    while i < len {
        let b = bytes[i];
        if b == b'"' || b == b'\'' || b == b'`' {
            let q = b;
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(len);
                    continue;
                }
                if bytes[i] == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            let trimmed = source[start_byte..i].trim().to_string();
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            return (trimmed, i);
        }
        match b {
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b';' if paren == 0 && bracket == 0 && brace == 0 => {
                let trimmed = source[start_byte..i].trim().to_string();
                return (trimmed, i + 1);
            }
            b'\n' if paren == 0 && bracket == 0 && brace == 0 => {
                let trimmed = source[start_byte..i].trim().to_string();
                return (trimmed, i + 1);
            }
            _ => {}
        }
        i += 1;
    }
    (source[start_byte..len].trim().to_string(), len)
}
