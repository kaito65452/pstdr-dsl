use crate::ast::*;
use crate::diagnostics::{has_errors, Diagnostic};
use crate::parser::parse_source;

/// Deterministically and idempotently formats `.pstdr` source code.
/// If the source contains syntax errors (including unclosed `}` braces),
/// the formatter refuses to modify the file and returns the list of diagnostics.
pub fn format_source(source: &str, file: &str) -> Result<String, Vec<Diagnostic>> {
    let (program, diags) = parse_source(source, file);
    if has_errors(&diags) {
        return Err(diags);
    }
    let Some(prog) = program else {
        return Err(diags);
    };

    let mut f = Formatter {
        out: String::new(),
        indent: 0,
    };
    f.fmt_program(&prog);
    Ok(f.out)
}

struct Formatter {
    out: String,
    indent: usize,
}

impl Formatter {
    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn line(&mut self, s: &str) {
        self.push_indent();
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn fmt_program(&mut self, prog: &Program) {
        let mut need_blank = false;

        if let Some(h) = &prog.header {
            self.line(&format!(
                "@addon(target = \"{}\", api = \"{}\")",
                escape_str(&h.target),
                escape_str(&h.api)
            ));
            need_blank = true;
        }

        if !prog.bind_targets.is_empty() {
            if need_blank {
                self.out.push('\n');
            }
            for bt in &prog.bind_targets {
                self.line(&format!("bind_target(\"{}\")", escape_str(&bt.path)));
            }
            need_blank = true;
        }

        for (idx, item) in prog.items.iter().enumerate() {
            let is_block_item = matches!(
                item,
                TopLevelItem::FuncDecl(_)
                    | TopLevelItem::UnsafeFuncDecl(_)
                    | TopLevelItem::EventHandler(_)
                    | TopLevelItem::AfterTimer { .. }
                    | TopLevelItem::EveryTimer { .. }
            );
            if idx == 0 {
                if need_blank {
                    self.out.push('\n');
                }
            } else if is_block_item
                || matches!(
                    prog.items[idx - 1],
                    TopLevelItem::FuncDecl(_)
                        | TopLevelItem::UnsafeFuncDecl(_)
                        | TopLevelItem::EventHandler(_)
                        | TopLevelItem::AfterTimer { .. }
                        | TopLevelItem::EveryTimer { .. }
                )
            {
                self.out.push('\n');
            }
            self.fmt_top_level(item);
        }
    }

    fn fmt_top_level(&mut self, item: &TopLevelItem) {
        match item {
            TopLevelItem::ConstDecl(d) => {
                self.line(&format!("const {} = {}", d.name, fmt_expr(&d.value)));
            }
            TopLevelItem::MutDecl(d) => {
                self.line(&format!("mut {} = {}", d.name, fmt_expr(&d.value)));
            }
            TopLevelItem::StateDecl(s) => {
                self.line(&format!(
                    "state {}[{}] = {}",
                    s.name,
                    s.owner_hint,
                    fmt_expr(&s.default_value)
                ));
            }
            TopLevelItem::FuncDecl(f) => {
                let params: Vec<&str> = f.params.iter().map(|(p, _)| p.as_str()).collect();
                self.line(&format!("func {}({}) {{", f.name, params.join(", ")));
                self.indent += 1;
                self.fmt_block_stmts(&f.body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            TopLevelItem::UnsafeFuncDecl(f) => {
                self.line(&format!(
                    "unsafe {}({}) {{{}}}",
                    f.name,
                    f.params.join(", "),
                    f.raw_body
                ));
            }
            TopLevelItem::UnsafeVarDecl(u) => {
                let kw = if u.keyword == "let" {
                    "mut"
                } else {
                    u.keyword.as_str()
                };
                let mut rest = u.raw_rest.trim_end().to_string();
                if !rest.ends_with(';') {
                    rest.push(';');
                }
                self.line(&format!("unsafe {kw} {rest}"));
            }
            TopLevelItem::EventHandler(ev) => {
                let header = match &ev.kind {
                    EventKind::RightClick { item, actor_var } => {
                        format!("on right click with {} by {actor_var} {{", fmt_expr(item))
                    }
                    EventKind::HitTarget {
                        target_var,
                        item,
                        actor_var,
                    } => {
                        format!(
                            "on hit {target_var} with {} by {actor_var} {{",
                            fmt_expr(item)
                        )
                    }
                    EventKind::BreakBlock {
                        block_id,
                        actor_var,
                    } => {
                        format!("on break of {} by {actor_var} {{", fmt_expr(block_id))
                    }
                    EventKind::PlayerJoin { actor_var } => {
                        format!("on join of {actor_var} {{")
                    }
                    EventKind::Hurt {
                        target_var,
                        attacker_var,
                    } => match attacker_var {
                        Some(a) => format!("on hurt {target_var} by {a} {{"),
                        None => format!("on hurt {target_var} {{"),
                    },
                    EventKind::Die {
                        target_var,
                        killer_var,
                    } => match killer_var {
                        Some(k) => format!("on die of {target_var} by {k} {{"),
                        None => format!("on die of {target_var} {{"),
                    },
                    EventKind::Consume { item, actor_var } => {
                        format!("on consume {} by {actor_var} {{", fmt_expr(item))
                    }
                    EventKind::BlockInteract {
                        block_id,
                        actor_var,
                    } => {
                        format!("on interact of {} by {actor_var} {{", fmt_expr(block_id))
                    }
                    EventKind::BlockTick { block_id } => {
                        format!("on tick of {} {{", fmt_expr(block_id))
                    }
                    EventKind::ProjectileHit {
                        target_var,
                        projectile_id,
                        shooter_var,
                    } => {
                        let mut header = format!("on projectile hit {target_var}");
                        if let Some(proj) = projectile_id {
                            header.push_str(&format!(" with {}", fmt_expr(proj)));
                        }
                        if let Some(shooter) = shooter_var {
                            header.push_str(&format!(" by {shooter}"));
                        }
                        header.push_str(" {");
                        header
                    }
                    EventKind::EntitySpawn {
                        entity_var,
                        entity_type,
                    } => match entity_type {
                        Some(ty) => format!("on spawn of {entity_var} type {} {{", fmt_expr(ty)),
                        None => format!("on spawn of {entity_var} {{"),
                    },
                };
                self.line(&header);
                self.indent += 1;
                self.fmt_block_stmts(&ev.body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            TopLevelItem::AfterTimer { duration, body, .. } => {
                self.line(&format!("after {} {{", duration.raw));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            TopLevelItem::EveryTimer { duration, body, .. } => {
                self.line(&format!("every {} {{", duration.raw));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    fn fmt_block_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.fmt_stmt(stmt);
        }
    }

    fn fmt_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ConstDecl(d) => {
                self.line(&format!("const {} = {}", d.name, fmt_expr(&d.value)));
            }
            Stmt::MutDecl(d) => {
                self.line(&format!("mut {} = {}", d.name, fmt_expr(&d.value)));
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                let lhs = match target {
                    AssignTarget::Ident(n, _) => n.clone(),
                    AssignTarget::Index { object, index, .. } => {
                        format!("{}[{}]", fmt_expr(object), fmt_expr(index))
                    }
                    AssignTarget::Field { object, field, .. } => {
                        format!("{}.{field}", fmt_expr(object))
                    }
                };
                self.line(&format!("{lhs} {} {}", op.as_str(), fmt_expr(value)));
            }
            Stmt::If {
                condition,
                then_block,
                else_branch,
                ..
            } => {
                self.line(&format!("if {} {{", fmt_expr(condition)));
                self.indent += 1;
                self.fmt_block_stmts(&then_block.stmts);
                self.indent -= 1;
                self.fmt_else_branch(else_branch);
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                self.line(&format!("for {var} in {} {{", fmt_expr(iter)));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.line(&format!("while {} {{", fmt_expr(condition)));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Return { value, .. } => match value {
                Some(v) => self.line(&format!("return {}", fmt_expr(v))),
                None => self.line("return"),
            },
            Stmt::CancelEvent { .. } => self.line("cancel event"),
            Stmt::Damage { target, amount, .. } => {
                self.line(&format!(
                    "damage {} by {}",
                    fmt_expr(target),
                    fmt_expr(amount)
                ));
            }
            Stmt::Ignite {
                target, duration, ..
            } => {
                self.line(&format!("ignite {} for {}", fmt_expr(target), duration.raw));
            }
            Stmt::ApplyEffect {
                effect,
                level,
                target,
                duration,
                ..
            } => {
                self.line(&format!(
                    "apply {effect} {} to {} for {}",
                    fmt_expr(level),
                    fmt_expr(target),
                    duration.raw
                ));
            }
            Stmt::SendMessage {
                message, target, ..
            } => {
                self.line(&format!(
                    "send {} to {}",
                    fmt_expr(message),
                    fmt_expr(target)
                ));
            }
            Stmt::SpawnParticle {
                particle,
                location,
                dimension,
                ..
            } => match dimension {
                Some(dim) => self.line(&format!(
                    "spawn particle {} at {} in {}",
                    fmt_expr(particle),
                    fmt_expr(location),
                    fmt_expr(dim)
                )),
                None => self.line(&format!(
                    "spawn particle {} at {}",
                    fmt_expr(particle),
                    fmt_expr(location)
                )),
            },
            Stmt::After { duration, body, .. } => {
                self.line(&format!("after {} {{", duration.raw));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Every { duration, body, .. } => {
                self.line(&format!("every {} {{", duration.raw));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::ExprStmt { expr, .. } => {
                self.line(&fmt_expr(expr));
            }
        }
    }

    fn fmt_else_branch(&mut self, else_branch: &Option<ElseBranch>) {
        match else_branch {
            None => self.line("}"),
            Some(ElseBranch::Else(b)) => {
                self.line("} else {");
                self.indent += 1;
                self.fmt_block_stmts(&b.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Some(ElseBranch::ElseIf(next_if)) => {
                if let Stmt::If {
                    condition,
                    then_block,
                    else_branch,
                    ..
                } = next_if.as_ref()
                {
                    self.line(&format!("}} else if {} {{", fmt_expr(condition)));
                    self.indent += 1;
                    self.fmt_block_stmts(&then_block.stmts);
                    self.indent -= 1;
                    self.fmt_else_branch(else_branch);
                }
            }
        }
    }
}

fn fmt_expr(expr: &Expr) -> String {
    match expr {
        Expr::Number(n, _) => fmt_num(*n),
        Expr::Bool(b, _) => b.to_string(),
        Expr::Null(_) => "null".to_string(),
        Expr::Duration(d) => d.raw.clone(),
        Expr::Distance(b, _) => format!("{}b", fmt_num(*b)),
        Expr::Ident(name, _) => name.clone(),
        Expr::StringParts(parts, _) => {
            let mut out = String::from("\"");
            for p in parts {
                match p {
                    StringPart::Literal(s) => out.push_str(&escape_str_interp(s)),
                    StringPart::Expr(e) => {
                        out.push('{');
                        out.push_str(&fmt_expr(e));
                        out.push('}');
                    }
                }
            }
            out.push('"');
            out
        }
        Expr::Object(fields, _) => {
            if fields.is_empty() {
                return "{}".to_string();
            }
            let inner: Vec<String> = fields
                .iter()
                .map(|f| {
                    let k = if f.key_is_string {
                        format!("\"{}\"", escape_str(&f.key))
                    } else {
                        f.key.clone()
                    };
                    format!("{k} = {}", fmt_expr(&f.value))
                })
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        Expr::Array(items, _) => {
            let inner: Vec<String> = items.iter().map(fmt_expr).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Index { object, index, .. } => format!("{}[{}]", fmt_expr(object), fmt_expr(index)),
        Expr::Member {
            object,
            field,
            optional,
            ..
        } => {
            let sep = if *optional { "?." } else { "." };
            format!("{}{sep}{field}", fmt_expr(object))
        }
        Expr::Call { callee, args, .. } => {
            let inner: Vec<String> = args.iter().map(fmt_expr).collect();
            format!("{}({})", fmt_expr(callee), inner.join(", "))
        }
        Expr::Unary { op, expr, .. } => match op {
            UnaryOp::Not => format!("not {}", fmt_expr(expr)),
            UnaryOp::Neg => format!("-{}", fmt_expr(expr)),
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Eq => "==",
                BinaryOp::Neq => "!=",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::And => "and",
                BinaryOp::Or => "or",
                BinaryOp::Nullish => "??",
                BinaryOp::In => "in",
                BinaryOp::Within => "within",
            };
            format!("{} {op_str} {}", fmt_expr(left), fmt_expr(right))
        }
        Expr::IsCheck {
            subject,
            negated,
            predicate,
            ..
        } => {
            let neg = if *negated { " not" } else { "" };
            let pred = match predicate {
                IsPredicate::Sneaking => "sneaking".to_string(),
                IsPredicate::Sprinting => "sprinting".to_string(),
                IsPredicate::Swimming => "swimming".to_string(),
                IsPredicate::Flying => "flying".to_string(),
                IsPredicate::Sleeping => "sleeping".to_string(),
                IsPredicate::OnGround => "on_ground".to_string(),
                IsPredicate::InWater => "in_water".to_string(),
                IsPredicate::Holding(item) => format!("holding {}", fmt_expr(item)),
            };
            format!("{} is{neg} {pred}", fmt_expr(subject))
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => format!(
            "{} ? {} : {}",
            fmt_expr(cond),
            fmt_expr(then_expr),
            fmt_expr(else_expr)
        ),
        Expr::Paren(inner, _) => format!("({})", fmt_expr(inner)),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn escape_str_interp(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
