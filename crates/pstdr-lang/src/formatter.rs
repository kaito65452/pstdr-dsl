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
                    | TopLevelItem::McFunctionDecl(_)
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
                        | TopLevelItem::McFunctionDecl(_)
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
            TopLevelItem::Import(imp) => {
                let syms: Vec<&str> = imp.symbols.iter().map(|s| s.name.as_str()).collect();
                self.line(&format!(
                    "import {{ {} }} from \"{}\"",
                    syms.join(", "),
                    escape_str(&imp.path)
                ));
            }
            TopLevelItem::TableDecl(t) => {
                let prefix = if t.exported { "export " } else { "" };
                self.line(&format!("{prefix}table {} = {}", t.name, fmt_expr(&t.value)));
            }
            TopLevelItem::ConstDecl(d) => {
                let prefix = if d.exported { "export " } else { "" };
                self.line(&format!("{prefix}const {} = {}", d.name, fmt_expr(&d.value)));
            }
            TopLevelItem::MutDecl(d) => {
                self.line(&format!("mut {} = {}", d.name, fmt_expr(&d.value)));
            }
            TopLevelItem::StateDecl(s) => {
                let prefix = if s.exported { "export " } else { "" };
                self.line(&format!(
                    "{prefix}state {}[{}] = {}",
                    s.name,
                    s.owner_hint,
                    fmt_expr(&s.default_value)
                ));
            }
            TopLevelItem::FuncDecl(f) => {
                let prefix = if f.exported { "export " } else { "" };
                let params: Vec<String> = f
                    .params
                    .iter()
                    .enumerate()
                    .map(|(idx, (p, _))| {
                        if f.entity_params.get(idx).copied().unwrap_or(false) {
                            format!("{p}: Entity")
                        } else {
                            p.clone()
                        }
                    })
                    .collect();
                self.line(&format!("{prefix}func {}({}) {{", f.name, params.join(", ")));
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
            TopLevelItem::McFunctionDecl(mc) => {
                self.line(&format!(
                    "mcfunction \"{}\" {{{}}}",
                    escape_str(&mc.path),
                    mc.raw_body
                ));
            }
            TopLevelItem::EventHandler(ev) => {
                let mut header = match &ev.kind {
                    EventKind::RightClick { item, actor_var } => match item {
                        Some(it) => format!("on right click with {} by {actor_var} {{", fmt_expr(it)),
                        None => format!("on right click by {actor_var} {{"),
                    },
                    EventKind::HitTarget {
                        target_var,
                        item,
                        actor_var,
                    } => match item {
                        Some(it) => format!(
                            "on hit {target_var} with {} by {actor_var} {{",
                            fmt_expr(it)
                        ),
                        None => format!("on hit {target_var} by {actor_var} {{"),
                    },
                    EventKind::BreakBlock {
                        block_id,
                        actor_var,
                    } => {
                        format!("on break of {} by {actor_var} {{", fmt_expr(block_id))
                    }
                    EventKind::PlayerJoin { actor_var } => {
                        format!("on join of {actor_var} {{")
                    }
                    EventKind::PlayerLeave { actor_var } => {
                        format!("on leave of {actor_var} {{")
                    }
                    EventKind::ChatSend { msg_var, actor_var } => {
                        format!("on chat {msg_var} by {actor_var} {{")
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
                    EventKind::Consume { item, actor_var } => match item {
                        Some(it) => format!("on consume {} by {actor_var} {{", fmt_expr(it)),
                        None => format!("on consume by {actor_var} {{"),
                    },
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
                    EventKind::ScriptEvent { event_id } => {
                        format!("on script_event {} {{", fmt_expr(event_id))
                    }
                    EventKind::EntityTrigger {
                        event_id,
                        entity_type,
                    } => match entity_type {
                        Some(ty) => {
                            format!(
                                "on entity_trigger {} for {} {{",
                                fmt_expr(event_id),
                                fmt_expr(ty)
                            )
                        }
                        None => format!("on entity_trigger {} {{", fmt_expr(event_id)),
                    },
                    EventKind::CustomCommand {
                        command_id,
                        description,
                        permission,
                        params,
                        actor_var,
                    } => {
                        let mut h = format!(
                            "on cmd {} description {}",
                            fmt_expr(command_id),
                            fmt_expr(description)
                        );
                        if *permission != CmdPermission::Any {
                            h.push_str(&format!(" permission {}", permission.as_pstdr_str()));
                        } else {
                            h.push_str(" permission any");
                        }
                        if !params.is_empty() {
                            let p_strs: Vec<String> = params
                                .iter()
                                .map(|p| {
                                    let q = if p.optional { "?" } else { "" };
                                    format!("{}{q}: {}", p.name, p.param_type.as_pstdr_str())
                                })
                                .collect();
                            h.push_str(&format!(" ({})", p_strs.join(", ")));
                        }
                        if let Some(a) = actor_var {
                            h.push_str(&format!(" by {a}"));
                        }
                        h.push_str(" {");
                        h
                    }
                    EventKind::PlaceBlock {
                        block_var,
                        actor_var,
                    } => format!("on place of {block_var} by {actor_var} {{"),
                    EventKind::DimensionChange { player_var } => {
                        format!("on dimension_change of {player_var} {{")
                    }
                    EventKind::HealthChange { entity_var } => {
                        format!("on health_change of {entity_var} {{")
                    }
                    EventKind::InteractEntity {
                        target_var,
                        actor_var,
                    } => format!("on interact_entity of {target_var} by {actor_var} {{"),
                    EventKind::ProjectileHitBlock {
                        block_var,
                        shooter_var,
                    } => match shooter_var {
                        Some(s) => format!("on projectile_hit_block of {block_var} by {s} {{"),
                        None => format!("on projectile_hit_block of {block_var} {{"),
                    },
                    EventKind::EntityHitBlock {
                        block_var,
                        attacker_var,
                    } => format!("on entity_hit_block of {block_var} by {attacker_var} {{"),
                    EventKind::ItemRelease { item, actor_var } => match item {
                        Some(it) => format!("on item_release with {} by {actor_var} {{", fmt_expr(it)),
                        None => format!("on item_release by {actor_var} {{"),
                    },
                    EventKind::Explosion => "on explosion {".to_string(),
                    EventKind::EntityLoad { entity_var } => {
                        format!("on entity_load of {entity_var} {{")
                    }
                    EventKind::EntityRemove { entity_var } => {
                        format!("on entity_remove of {entity_var} {{")
                    }
                };
                if let Some(sneak) = ev.sneak_filter {
                    let suffix = if sneak { " when sneaking {" } else { " when not sneaking {" };
                    if let Some(prefix) = header.strip_suffix(" {") {
                        header = format!("{prefix}{suffix}");
                    }
                }
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
            Stmt::TableDecl(t) => {
                self.line(&format!("table {} = {}", t.name, fmt_expr(&t.value)));
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
            Stmt::Damage {
                target,
                amount,
                cause,
                source,
                ..
            } => {
                let mut line = format!("damage {} by {}", fmt_expr(target), fmt_expr(amount));
                if let Some(c) = cause {
                    line.push_str(&format!(" cause {}", fmt_expr(c)));
                }
                if let Some(s) = source {
                    line.push_str(&format!(" source {}", fmt_expr(s)));
                }
                self.line(&line);
            }
            Stmt::Heal { target, amount, .. } => {
                self.line(&format!("heal {} by {}", fmt_expr(target), fmt_expr(amount)));
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
                let dur_str = fmt_delay_spec(duration);
                match effect {
                    EffectSpec::Static { name, .. } => {
                        self.line(&format!(
                            "apply {name} {} to {} for {dur_str}",
                            fmt_expr(level),
                            fmt_expr(target),
                        ));
                    }
                    EffectSpec::Dynamic(eff_expr) => {
                        self.line(&format!(
                            "apply effect({}, {}) to {} for {dur_str}",
                            fmt_expr(eff_expr),
                            fmt_expr(level),
                            fmt_expr(target),
                        ));
                    }
                }
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
            Stmt::PlaySound {
                sound,
                target,
                volume,
                pitch,
                custom,
                ..
            } => {
                let kw = if *custom { "custom_sound" } else { "sound" };
                let mut line = format!("play {kw} {} at {}", fmt_expr(sound), fmt_expr(target));
                if let Some(v) = volume {
                    line.push_str(&format!(" volume {}", fmt_expr(v)));
                }
                if let Some(p) = pitch {
                    line.push_str(&format!(" pitch {}", fmt_expr(p)));
                }
                self.line(&line);
            }
            Stmt::Launch {
                projectile,
                from,
                speed,
                ..
            } => {
                self.line(&format!(
                    "launch {} from {} speed {}",
                    fmt_expr(projectile),
                    fmt_expr(from),
                    fmt_expr(speed)
                ));
            }
            Stmt::Dash {
                target,
                forward,
                up,
                ..
            } => {
                let mut line = format!("dash {} forward {}", fmt_expr(target), fmt_expr(forward));
                if let Some(u) = up {
                    line.push_str(&format!(" up {}", fmt_expr(u)));
                }
                self.line(&line);
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
            Stmt::Command {
                target, command, ..
            } => match target {
                Some(t) => self.line(&format!(
                    "command on {} {}",
                    fmt_expr(t),
                    fmt_expr(command)
                )),
                None => self.line(&format!("command {}", fmt_expr(command))),
            },
            Stmt::RunFn { target, path, .. } => match target {
                Some(t) => self.line(&format!("run_fn on {} {}", fmt_expr(t), fmt_expr(path))),
                None => self.line(&format!("run_fn {}", fmt_expr(path))),
            },
            Stmt::After { delay, body, .. } => {
                let delay_str = match delay {
                    DelaySpec::Literal(d) => d.raw.clone(),
                    DelaySpec::Expr(e) => {
                        let mut cur = e;
                        while let Expr::Paren(inner, _) = cur {
                            cur = inner.as_ref();
                        }
                        format!("({})", fmt_expr(cur))
                    }
                };
                self.line(&format!("after {delay_str} {{"));
                self.indent += 1;
                self.fmt_block_stmts(&body.stmts);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Timeline { entries, .. } => {
                self.line("timeline {");
                self.indent += 1;
                for (delay, body) in entries {
                    let delay_str = fmt_delay_spec(delay);
                    self.line(&format!("{delay_str}: {{"));
                    self.indent += 1;
                    self.fmt_block_stmts(&body.stmts);
                    self.indent -= 1;
                    self.line("}");
                }
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
            Stmt::UiAction {
                title,
                body,
                target,
                buttons,
                ..
            } => {
                let mut header = format!("ui action {}", fmt_expr(title));
                if let Some(b) = body {
                    header.push_str(&format!(" body {}", fmt_expr(b)));
                }
                header.push_str(&format!(" to {} {{", fmt_expr(target)));
                self.line(&header);
                self.indent += 1;
                for btn in buttons {
                    let mut bh = format!("button {}", fmt_expr(&btn.label));
                    if let Some(ic) = &btn.icon {
                        bh.push_str(&format!(" icon {}", fmt_expr(ic)));
                    }
                    bh.push_str(" {");
                    self.line(&bh);
                    self.indent += 1;
                    self.fmt_block_stmts(&btn.body.stmts);
                    self.indent -= 1;
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
            }
            Stmt::UiMessage {
                title,
                body,
                target,
                button1,
                button2,
                ..
            } => {
                let mut header = format!("ui message {}", fmt_expr(title));
                if let Some(b) = body {
                    header.push_str(&format!(" body {}", fmt_expr(b)));
                }
                header.push_str(&format!(" to {} {{", fmt_expr(target)));
                self.line(&header);
                self.indent += 1;
                self.line(&format!("button1 {} {{", fmt_expr(&button1.label)));
                self.indent += 1;
                self.fmt_block_stmts(&button1.body.stmts);
                self.indent -= 1;
                self.line("}");
                self.line(&format!("button2 {} {{", fmt_expr(&button2.label)));
                self.indent += 1;
                self.fmt_block_stmts(&button2.body.stmts);
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Stmt::UiModal {
                title,
                target,
                fields,
                on_submit,
                ..
            } => {
                self.line(&format!(
                    "ui modal {} to {} {{",
                    fmt_expr(title),
                    fmt_expr(target)
                ));
                self.indent += 1;
                for f in fields {
                    match &f.kind {
                        UiModalFieldKind::TextField {
                            label,
                            placeholder,
                            default,
                        } => {
                            let mut s = format!("text_field {}", fmt_expr(label));
                            if let Some(p) = placeholder {
                                s.push_str(&format!(" placeholder {}", fmt_expr(p)));
                            }
                            if let Some(d) = default {
                                s.push_str(&format!(" default {}", fmt_expr(d)));
                            }
                            s.push_str(&format!(" as {}", f.binding));
                            self.line(&s);
                        }
                        UiModalFieldKind::Toggle { label, default } => {
                            let mut s = format!("toggle {}", fmt_expr(label));
                            if let Some(d) = default {
                                s.push_str(&format!(" default {}", fmt_expr(d)));
                            }
                            s.push_str(&format!(" as {}", f.binding));
                            self.line(&s);
                        }
                        UiModalFieldKind::Slider {
                            label,
                            min,
                            max,
                            step,
                            default,
                        } => {
                            let mut s = format!(
                                "slider {} min {} max {} step {}",
                                fmt_expr(label),
                                fmt_expr(min),
                                fmt_expr(max),
                                fmt_expr(step)
                            );
                            if let Some(d) = default {
                                s.push_str(&format!(" default {}", fmt_expr(d)));
                            }
                            s.push_str(&format!(" as {}", f.binding));
                            self.line(&s);
                        }
                        UiModalFieldKind::Dropdown {
                            label,
                            options,
                            default,
                        } => {
                            let mut s = format!(
                                "dropdown {} options {}",
                                fmt_expr(label),
                                fmt_expr(options)
                            );
                            if let Some(d) = default {
                                s.push_str(&format!(" default {}", fmt_expr(d)));
                            }
                            s.push_str(&format!(" as {}", f.binding));
                            self.line(&s);
                        }
                    }
                }
                self.line("on submit {");
                self.indent += 1;
                self.fmt_block_stmts(&on_submit.stmts);
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Stmt::ScoreboardShow {
                objective,
                title,
                slot,
                ..
            } => match title {
                Some(t) => self.line(&format!(
                    "scoreboard show {} title {} slot {}",
                    fmt_expr(objective),
                    fmt_expr(t),
                    slot.as_pstdr_str()
                )),
                None => self.line(&format!(
                    "scoreboard show {} slot {}",
                    fmt_expr(objective),
                    slot.as_pstdr_str()
                )),
            },
            Stmt::ScoreboardHide { slot, .. } => {
                self.line(&format!("scoreboard hide slot {}", slot.as_pstdr_str()));
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

fn fmt_delay_spec(delay: &DelaySpec) -> String {
    match delay {
        DelaySpec::Literal(d) => d.raw.clone(),
        DelaySpec::Expr(e) => fmt_expr(e),
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
        Expr::WithinDistance {
            left,
            distance,
            right,
            ..
        } => {
            format!(
                "{} within {} of {}",
                fmt_expr(left),
                fmt_expr(distance),
                fmt_expr(right)
            )
        }
        Expr::RaycastEntities {
            origin,
            range,
            pierce,
            step,
            ..
        } => {
            let mut s = format!("raycast entities from {} range {}", fmt_expr(origin), fmt_expr(range));
            if let Some(p) = pierce {
                s.push_str(&format!(" pierce {}", fmt_expr(p)));
            }
            if let Some(st) = step {
                s.push_str(&format!(" step {}", fmt_expr(st)));
            }
            s
        }
        Expr::ConeEntities {
            origin,
            radius,
            angle,
            ..
        } => {
            format!(
                "entities in cone({}, radius: {}, angle: {})",
                fmt_expr(origin),
                fmt_expr(radius),
                fmt_expr(angle)
            )
        }
        Expr::EntitiesWithin { radius, origin, .. } => {
            format!("entities within {} of {}", fmt_expr(radius), fmt_expr(origin))
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
