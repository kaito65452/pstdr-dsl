use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};
use crate::partition::TargetRegistry;
use crate::spec::BedrockSpec;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKind {
    Const,
    Mut,
    State,
    Func,
    UnsafeFunc,
    UnsafeVar,
    ContextVar,
    Builtin,
}

#[derive(Debug, Clone)]
struct BindingInfo {
    kind: BindingKind,
    decl_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventCtx {
    TopLevel,
    Func,
    RightClick,
    HitTarget,
    BreakBlock,
    BlockInteract,
    BlockTick,
    ProjectileHit,
    EntitySpawn,
    PlayerJoin,
    Hurt,
    Die,
    Consume,
    Timer,
}

impl EventCtx {
    fn label(&self) -> &'static str {
        match self {
            Self::TopLevel => "top level",
            Self::Func => "func",
            Self::RightClick => "on right click",
            Self::HitTarget => "on hit",
            Self::BreakBlock => "on break",
            Self::BlockInteract => "on interact",
            Self::BlockTick => "on tick",
            Self::ProjectileHit => "on projectile hit",
            Self::EntitySpawn => "on spawn",
            Self::PlayerJoin => "on join",
            Self::Hurt => "on hurt",
            Self::Die => "on die",
            Self::Consume => "on consume",
            Self::Timer => "after/every",
        }
    }

    fn is_cancellable(&self) -> bool {
        matches!(self, Self::RightClick | Self::BreakBlock | Self::BlockInteract)
    }
}

pub struct ValidationContext<'a> {
    pub workspace_root: Option<&'a Path>,
    pub spec: &'a BedrockSpec,
    pub strict_vanilla_ids: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedProgramInfo {
    pub const_strings: HashMap<String, String>,
    pub state_names: HashSet<String>,
    pub bound_item_ids: Vec<(String, String, Span)>,
    pub bound_block_ids: Vec<(String, String, Span)>,
}

pub fn validate_program(
    program: &Program,
    ctx: &ValidationContext<'_>,
) -> (ValidatedProgramInfo, Vec<Diagnostic>) {
    let discovered_registry = ctx
        .workspace_root
        .and_then(|root| TargetRegistry::discover(root).ok());

    let mut v = Validator {
        ctx,
        scopes: vec![HashMap::new()],
        const_strings: HashMap::new(),
        state_names: HashSet::new(),
        event_ctx: EventCtx::TopLevel,
        diagnostics: Vec::new(),
        bound_item_ids: Vec::new(),
        bound_block_ids: Vec::new(),
        has_block_bind_target: false,
        static_event_item_ids: Vec::new(),
        static_event_block_ids: Vec::new(),
        discovered_registry,
    };

    v.seed_builtins();
    v.validate_bind_targets(program);
    v.register_top_level(program);
    v.validate_items(program);
    v.check_binding_id_matches();

    (
        ValidatedProgramInfo {
            const_strings: v.const_strings,
            state_names: v.state_names,
            bound_item_ids: v.bound_item_ids,
            bound_block_ids: v.bound_block_ids,
        },
        v.diagnostics,
    )
}

struct Validator<'a> {
    ctx: &'a ValidationContext<'a>,
    scopes: Vec<HashMap<String, BindingInfo>>,
    const_strings: HashMap<String, String>,
    state_names: HashSet<String>,
    event_ctx: EventCtx,
    diagnostics: Vec<Diagnostic>,
    bound_item_ids: Vec<(String, String, Span)>,
    bound_block_ids: Vec<(String, String, Span)>,
    has_block_bind_target: bool,
    static_event_item_ids: Vec<(String, Span)>,
    static_event_block_ids: Vec<(String, Span)>,
    discovered_registry: Option<TargetRegistry>,
}

impl<'a> Validator<'a> {
    fn seed_builtins(&mut self) {
        let builtins = [
            "world", "system", "console", "Math", "JSON", "String", "Number", "Boolean",
            "Array", "Object", "Date", "parseInt", "parseFloat", "isNaN",
        ];
        for name in builtins {
            self.scopes[0].insert(
                name.to_string(),
                BindingInfo {
                    kind: BindingKind::Builtin,
                    decl_line: 0,
                },
            );
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn insert_binding(&mut self, name: &str, kind: BindingKind, decl_line: usize) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), BindingInfo { kind, decl_line });
        }
    }

    fn lookup_binding(&self, name: &str) -> Option<&BindingInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn validate_bind_targets(&mut self, program: &Program) {
        for bt in &program.bind_targets {
            if has_path_traversal(&bt.path) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E009,
                    &bt.span,
                    format!("Path traversal is not allowed in bind_target('{}').", bt.path),
                    "Remove '..' or absolute paths; use only paths relative to the workspace root.",
                ));
                continue;
            }

            if bt.path.contains("blocks/") || bt.path.contains("blocks\\") {
                self.has_block_bind_target = true;
            }

            let Some(root) = self.ctx.workspace_root else {
                continue;
            };

            let target_path = root.join(&bt.path);
            if !target_path.is_file() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E007,
                    &bt.span,
                    format!("Target file '{}' in bind_target() not found.", bt.path),
                    "Check the spelling of the relative path from the project root and ensure the file exists.",
                ));
                continue;
            }

            if let (Ok(canon_root), Ok(canon_target)) =
                (fs::canonicalize(root), fs::canonicalize(&target_path))
            {
                if !canon_target.starts_with(&canon_root) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E009,
                        &bt.span,
                        format!("Path traversal is not allowed in bind_target('{}').", bt.path),
                        "Target must reside inside the project workspace root directory.",
                    ));
                    continue;
                }
            }

            if let Ok(raw_json) = fs::read_to_string(&target_path) {
                if let Ok(val) = serde_json::from_str::<Value>(&raw_json) {
                    if let Some(id) = val
                        .get("minecraft:item")
                        .and_then(|v| v.get("description"))
                        .and_then(|v| v.get("identifier"))
                        .and_then(|v| v.as_str())
                    {
                        self.bound_item_ids
                            .push((bt.path.clone(), id.to_string(), bt.span.clone()));
                    } else if let Some(id) = val
                        .get("minecraft:block")
                        .and_then(|v| v.get("description"))
                        .and_then(|v| v.get("identifier"))
                        .and_then(|v| v.as_str())
                    {
                        self.bound_block_ids
                            .push((bt.path.clone(), id.to_string(), bt.span.clone()));
                        self.has_block_bind_target = true;
                    }
                }
            }
        }
    }

    fn register_top_level(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                TopLevelItem::ConstDecl(d) => {
                    if let Some(s) = eval_static_string(&d.value, &self.const_strings) {
                        self.const_strings.insert(d.name.clone(), s);
                    }
                    self.insert_binding(&d.name, BindingKind::Const, d.span.line);
                }
                TopLevelItem::MutDecl(d) => {
                    self.insert_binding(&d.name, BindingKind::Mut, d.span.line);
                }
                TopLevelItem::StateDecl(s) => {
                    self.state_names.insert(s.name.clone());
                    self.insert_binding(&s.name, BindingKind::State, s.span.line);
                }
                TopLevelItem::FuncDecl(f) => {
                    self.insert_binding(&f.name, BindingKind::Func, f.span.line);
                }
                TopLevelItem::UnsafeFuncDecl(f) => {
                    self.insert_binding(&f.name, BindingKind::UnsafeFunc, f.span.line);
                }
                TopLevelItem::UnsafeVarDecl(u) => {
                    self.insert_binding(&u.name, BindingKind::UnsafeVar, u.span.line);
                }
                _ => {}
            }
        }
    }

    fn validate_items(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                TopLevelItem::ConstDecl(d) | TopLevelItem::MutDecl(d) => {
                    self.validate_expr(&d.value);
                }
                TopLevelItem::StateDecl(s) => {
                    self.validate_expr(&s.default_value);
                }
                TopLevelItem::FuncDecl(f) => {
                    let prev_ctx = self.event_ctx;
                    self.event_ctx = EventCtx::Func;
                    self.push_scope();
                    for (p, p_span) in &f.params {
                        self.insert_binding(p, BindingKind::Mut, p_span.line);
                    }
                    self.validate_block(&f.body);
                    self.pop_scope();
                    self.event_ctx = prev_ctx;
                }
                TopLevelItem::UnsafeFuncDecl(_) | TopLevelItem::UnsafeVarDecl(_) => {
                    // `unsafe` contents are isolated and exempt from .pstdr type/keyword checks.
                }
                TopLevelItem::EventHandler(ev) => {
                    self.validate_event_handler(ev);
                }
                TopLevelItem::AfterTimer { body, .. } => {
                    let prev_ctx = self.event_ctx;
                    self.event_ctx = EventCtx::Timer;
                    self.push_scope();
                    self.validate_block(body);
                    self.pop_scope();
                    self.event_ctx = prev_ctx;
                }
                TopLevelItem::EveryTimer { duration, body, .. } => {
                    self.check_every_interval(duration);
                    let prev_ctx = self.event_ctx;
                    self.event_ctx = EventCtx::Timer;
                    self.push_scope();
                    self.validate_block(body);
                    self.pop_scope();
                    self.event_ctx = prev_ctx;
                }
            }
        }
    }

    fn validate_event_handler(&mut self, ev: &EventHandler) {
        let prev_ctx = self.event_ctx;
        self.push_scope();

        match &ev.kind {
            EventKind::RightClick { item, actor_var } => {
                self.validate_expr(item);
                if let Some(s) = eval_static_string(item, &self.const_strings) {
                    self.static_event_item_ids
                        .push((s, item.span().clone()));
                }
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::RightClick;
            }
            EventKind::HitTarget {
                target_var,
                item,
                actor_var,
            } => {
                self.validate_expr(item);
                if let Some(s) = eval_static_string(item, &self.const_strings) {
                    self.static_event_item_ids
                        .push((s, item.span().clone()));
                }
                self.insert_binding(target_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::HitTarget;
            }
            EventKind::BreakBlock {
                block_id,
                actor_var,
            } => {
                self.validate_expr(block_id);
                if let Some(s) = eval_static_string(block_id, &self.const_strings) {
                    self.static_event_block_ids
                        .push((s, block_id.span().clone()));
                }
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("block", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::BreakBlock;
            }
            EventKind::PlayerJoin { actor_var } => {
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::PlayerJoin;
            }
            EventKind::Hurt {
                target_var,
                attacker_var,
            } => {
                self.insert_binding(target_var, BindingKind::ContextVar, ev.span.line);
                if let Some(a) = attacker_var {
                    self.insert_binding(a, BindingKind::ContextVar, ev.span.line);
                }
                self.event_ctx = EventCtx::Hurt;
            }
            EventKind::Die {
                target_var,
                killer_var,
            } => {
                self.insert_binding(target_var, BindingKind::ContextVar, ev.span.line);
                if let Some(k) = killer_var {
                    self.insert_binding(k, BindingKind::ContextVar, ev.span.line);
                }
                self.event_ctx = EventCtx::Die;
            }
            EventKind::Consume { item, actor_var } => {
                self.validate_expr(item);
                if let Some(s) = eval_static_string(item, &self.const_strings) {
                    self.static_event_item_ids
                        .push((s, item.span().clone()));
                }
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::Consume;
            }
            EventKind::BlockInteract {
                block_id,
                actor_var,
            } => {
                self.validate_expr(block_id);
                if let Some(s) = eval_static_string(block_id, &self.const_strings) {
                    self.static_event_block_ids
                        .push((s, block_id.span().clone()));
                }
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("block", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::BlockInteract;
            }
            EventKind::BlockTick { block_id } => {
                self.validate_expr(block_id);
                let resolved_id = eval_static_string(block_id, &self.const_strings);
                if let Some(ref s) = resolved_id {
                    self.static_event_block_ids
                        .push((s.clone(), block_id.span().clone()));
                }
                let discovered_match = resolved_id.as_deref().is_some_and(|id| {
                    self.discovered_registry
                        .as_ref()
                        .is_some_and(|reg| reg.has_block(id))
                });
                let has_bound_block = if self.ctx.workspace_root.is_some() {
                    !self.bound_block_ids.is_empty() || discovered_match
                } else {
                    self.has_block_bind_target
                };
                if !has_bound_block {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E007,
                        &ev.span,
                        "'on tick' requires a bound block target via bind_target(\"BP/blocks/...\") or a matching block JSON in BP/blocks/.",
                        "Add bind_target(\"BP/blocks/<block>.json\") at the top of the file or define a matching block JSON in BP/blocks/.",
                    ));
                }
                self.insert_binding("block", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("dimension", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::BlockTick;
            }
            EventKind::ProjectileHit {
                target_var,
                projectile_id,
                shooter_var,
            } => {
                if let Some(proj) = projectile_id {
                    self.validate_expr(proj);
                }
                self.insert_binding(target_var, BindingKind::ContextVar, ev.span.line);
                if let Some(shooter) = shooter_var {
                    self.insert_binding(shooter, BindingKind::ContextVar, ev.span.line);
                }
                self.insert_binding("projectile", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::ProjectileHit;
            }
            EventKind::EntitySpawn {
                entity_var,
                entity_type,
            } => {
                if let Some(ty) = entity_type {
                    self.validate_expr(ty);
                }
                self.insert_binding(entity_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::EntitySpawn;
            }
        }

        self.validate_block(&ev.body);
        self.pop_scope();
        self.event_ctx = prev_ctx;
    }

    fn check_binding_id_matches(&mut self) {
        for (path, id_json, span) in &self.bound_item_ids {
            if !self.static_event_item_ids.is_empty()
                && !self
                    .static_event_item_ids
                    .iter()
                    .any(|(id_code, _)| id_code == id_json)
            {
                let id_code = &self.static_event_item_ids[0].0;
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E008,
                    span,
                    format!(
                        "Identifier mismatch: '{path}' uses '{id_json}', script uses '{id_code}'."
                    ),
                    format!(
                        "Match \"identifier\" in '{path}' with item ID '{id_code}' in your .pstdr script."
                    ),
                ));
            }
        }

        for (path, id_json, span) in &self.bound_block_ids {
            if !self.static_event_block_ids.is_empty()
                && !self
                    .static_event_block_ids
                    .iter()
                    .any(|(id_code, _)| id_code == id_json)
            {
                let id_code = &self.static_event_block_ids[0].0;
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E008,
                    span,
                    format!(
                        "Identifier mismatch: '{path}' uses '{id_json}', script uses '{id_code}'."
                    ),
                    format!(
                        "Match \"identifier\" in '{path}' with block ID '{id_code}' in your .pstdr script."
                    ),
                ));
            }
        }
    }

    fn check_every_interval(&mut self, duration: &DurationLit) {
        if duration.ms < 50.0 || duration.ticks < 1.0 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::E010,
                &duration.span,
                "'every' interval must be at least 1 tick (50ms / 0.05s).",
                "Increase the interval to at least '50ms' or '1s' to prevent freezing the Bedrock tick loop.",
            ));
        }
    }

    fn validate_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.validate_stmt(stmt);
        }
    }

    fn validate_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ConstDecl(d) => {
                self.validate_expr(&d.value);
                self.insert_binding(&d.name, BindingKind::Const, d.span.line);
            }
            Stmt::MutDecl(d) => {
                self.validate_expr(&d.value);
                self.insert_binding(&d.name, BindingKind::Mut, d.span.line);
            }
            Stmt::Assign { target, value, .. } => {
                self.validate_expr(value);
                match target {
                    AssignTarget::Ident(name, span) => match self.lookup_binding(name).cloned() {
                        Some(info) if info.kind == BindingKind::Const => {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E002,
                                span,
                                format!("Cannot reassign constant variable '{name}'."),
                                format!(
                                    "Change the declaration on line {} to 'mut {name} = ...' if its value needs to change.",
                                    info.decl_line
                                ),
                            ));
                        }
                        None => {
                            self.emit_missing_scope_error(name, span);
                        }
                        _ => {}
                    },
                    AssignTarget::Index { object, index, .. } => {
                        if let Expr::Ident(name, span) = object {
                            if self.lookup_binding(name).is_none() {
                                self.emit_missing_scope_error(name, span);
                            }
                        } else {
                            self.validate_expr(object);
                        }
                        self.validate_expr(index);
                    }
                    AssignTarget::Field { object, .. } => {
                        self.validate_expr(object);
                    }
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_branch,
                ..
            } => {
                self.validate_expr(condition);
                self.push_scope();
                self.validate_block(then_block);
                self.pop_scope();
                if let Some(eb) = else_branch {
                    match eb {
                        ElseBranch::ElseIf(s) => self.validate_stmt(s),
                        ElseBranch::Else(b) => {
                            self.push_scope();
                            self.validate_block(b);
                            self.pop_scope();
                        }
                    }
                }
            }
            Stmt::For {
                var,
                var_span,
                iter,
                body,
                ..
            } => {
                self.validate_expr(iter);
                self.push_scope();
                self.insert_binding(var, BindingKind::Mut, var_span.line);
                self.validate_block(body);
                self.pop_scope();
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.validate_expr(condition);
                self.push_scope();
                self.validate_block(body);
                self.pop_scope();
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.validate_expr(v);
                }
            }
            Stmt::CancelEvent { span } => {
                if !self.event_ctx.is_cancellable() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E004,
                        span,
                        format!(
                            "'cancel event' is not valid inside '{}' block.",
                            self.event_ctx.label()
                        ),
                        "Move 'cancel event' into an 'on right click', 'on break', or 'on interact' block.",
                    ));
                }
            }
            Stmt::Damage { target, amount, .. } => {
                self.validate_expr(target);
                self.validate_expr(amount);
            }
            Stmt::Ignite { target, .. } => {
                self.validate_expr(target);
            }
            Stmt::ApplyEffect {
                effect,
                effect_span,
                level,
                target,
                ..
            } => {
                self.validate_expr(level);
                self.validate_expr(target);

                if let Some(lvl_val) = eval_static_number(level) {
                    if lvl_val <= 0.0 {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E011,
                            level.span(),
                            format!(
                                "Effect '{effect}' level must be >= 1 (found: {}).",
                                format_num(lvl_val)
                            ),
                            "Use human-readable 1-based levels starting at 1 (Level I).",
                        ));
                    }
                }

                if self.ctx.strict_vanilla_ids && !self.ctx.spec.is_valid_effect(effect) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E013,
                        effect_span,
                        format!(
                            "Vanilla effect/sound ID '{effect}' not found in the latest Bedrock specification."
                        ),
                        "Check the spelling of the effect name (refer to data/spec/bedrock_api_latest.json).",
                    ));
                }
            }
            Stmt::SendMessage {
                message, target, ..
            } => {
                self.validate_expr(message);
                self.validate_expr(target);
            }
            Stmt::SpawnParticle {
                particle,
                location,
                dimension,
                ..
            } => {
                self.validate_expr(particle);
                self.validate_expr(location);
                if let Some(dim) = dimension {
                    self.validate_expr(dim);
                }
            }
            Stmt::After { body, .. } => {
                let prev = self.event_ctx;
                self.event_ctx = EventCtx::Timer;
                self.push_scope();
                self.validate_block(body);
                self.pop_scope();
                self.event_ctx = prev;
            }
            Stmt::Every { duration, body, .. } => {
                self.check_every_interval(duration);
                let prev = self.event_ctx;
                self.event_ctx = EventCtx::Timer;
                self.push_scope();
                self.validate_block(body);
                self.pop_scope();
                self.event_ctx = prev;
            }
            Stmt::ExprStmt { expr, .. } => {
                self.validate_expr(expr);
            }
        }
    }

    fn validate_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(_, _)
            | Expr::Bool(_, _)
            | Expr::Null(_)
            | Expr::Duration(_)
            | Expr::Distance(_, _) => {}
            Expr::StringParts(parts, _) => {
                for p in parts {
                    if let StringPart::Expr(e) = p {
                        self.validate_expr(e);
                    }
                }
            }
            Expr::Ident(name, span) => {
                if self.lookup_binding(name).is_none() {
                    self.emit_missing_scope_error(name, span);
                }
            }
            Expr::Object(fields, _) => {
                for f in fields {
                    self.validate_expr(&f.value);
                }
            }
            Expr::Array(items, _) => {
                for it in items {
                    self.validate_expr(it);
                }
            }
            Expr::Index { object, index, .. } => {
                self.validate_expr(object);
                self.validate_expr(index);
            }
            Expr::Member { object, .. } => {
                self.validate_expr(object);
            }
            Expr::Call { callee, args, .. } => {
                // Check vanilla sound ID in play_sound("...") / playSound("...")
                if let Expr::Member { field, .. } = callee.as_ref() {
                    if (field == "play_sound" || field == "playSound") && self.ctx.strict_vanilla_ids
                    {
                        if let Some(first_arg) = args.first() {
                            if let Some(snd) = eval_static_string(first_arg, &self.const_strings) {
                                if !self.ctx.spec.is_valid_sound(&snd) {
                                    self.diagnostics.push(Diagnostic::error(
                                        DiagnosticCode::E013,
                                        first_arg.span(),
                                        format!(
                                            "Vanilla effect/sound ID '{snd}' not found in the latest Bedrock specification."
                                        ),
                                        "Check the spelling of the sound name in data/spec/bedrock_api_latest.json.",
                                    ));
                                }
                            }
                        }
                    }
                }
                self.validate_expr(callee);
                for a in args {
                    self.validate_expr(a);
                }
            }
            Expr::Unary { expr: inner, .. } | Expr::Paren(inner, _) => {
                self.validate_expr(inner);
            }
            Expr::Binary { left, right, .. } => {
                self.validate_expr(left);
                self.validate_expr(right);
            }
            Expr::IsCheck {
                subject, predicate, ..
            } => {
                self.validate_expr(subject);
                if let IsPredicate::Holding(item) = predicate {
                    self.validate_expr(item);
                }
            }
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.validate_expr(cond);
                self.validate_expr(then_expr);
                self.validate_expr(else_expr);
            }
        }
    }

    fn emit_missing_scope_error(&mut self, name: &str, span: &Span) {
        let hint = if self.event_ctx == EventCtx::Timer && name == "player" {
            "Inside 'every'/'after' blocks, iterate over players: for p in world.players() { ... }."
        } else {
            "Ensure the variable is declared with const/mut or is available in this event scope."
        };
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::E005,
            span,
            format!(
                "Context variable '{name}' is not available inside '{}'.",
                self.event_ctx.label()
            ),
            hint,
        ));
    }
}

pub fn has_path_traversal(path: &str) -> bool {
    let p = path.trim();
    if p.is_empty() || p.starts_with('/') || p.starts_with('\\') {
        return true;
    }
    if p.len() >= 2 && p.as_bytes()[1] == b':' {
        return true;
    }
    let pb = PathBuf::from(p);
    pb.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

pub fn eval_static_string(expr: &Expr, const_strings: &HashMap<String, String>) -> Option<String> {
    match expr {
        Expr::StringParts(parts, _) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    StringPart::Literal(s) => out.push_str(s),
                    StringPart::Expr(e) => {
                        let sub = eval_static_string(e, const_strings)?;
                        out.push_str(&sub);
                    }
                }
            }
            Some(out)
        }
        Expr::Ident(name, _) => const_strings.get(name).cloned(),
        Expr::Paren(inner, _) => eval_static_string(inner, const_strings),
        _ => None,
    }
}

pub fn eval_static_number(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n, _) => Some(*n),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
            ..
        } => eval_static_number(expr).map(|v| -v),
        Expr::Paren(inner, _) => eval_static_number(inner),
        _ => None,
    }
}

fn format_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
