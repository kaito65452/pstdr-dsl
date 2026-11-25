use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};
use crate::partition::TargetRegistry;
use crate::spec::{
    contains_ident_token, is_beta_api_version, text_uses_beta_api, text_uses_beta_event,
    BedrockSpec,
};
use crate::workspace::WorkspaceConfig;
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
    ScriptEvent,
    EntityTrigger,
    PlayerJoin,
    PlayerLeave,
    ChatSend,
    Hurt,
    Die,
    Consume,
    CustomCommand,
    PlaceBlock,
    DimensionChange,
    HealthChange,
    InteractEntity,
    ProjectileHitBlock,
    EntityHitBlock,
    ItemRelease,
    Explosion,
    EntityLoad,
    EntityRemove,
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
            Self::ScriptEvent => "on script_event",
            Self::EntityTrigger => "on entity_trigger",
            Self::PlayerJoin => "on join",
            Self::PlayerLeave => "on leave",
            Self::ChatSend => "on chat",
            Self::Hurt => "on hurt",
            Self::Die => "on die",
            Self::Consume => "on consume",
            Self::CustomCommand => "on cmd",
            Self::PlaceBlock => "on place",
            Self::DimensionChange => "on dimension_change",
            Self::HealthChange => "on health_change",
            Self::InteractEntity => "on interact_entity",
            Self::ProjectileHitBlock => "on projectile_hit_block",
            Self::EntityHitBlock => "on entity_hit_block",
            Self::ItemRelease => "on item_release",
            Self::Explosion => "on explosion",
            Self::EntityLoad => "on entity_load",
            Self::EntityRemove => "on entity_remove",
            Self::Timer => "after/every",
        }
    }

    fn is_cancellable(&self) -> bool {
        matches!(
            self,
            Self::RightClick
                | Self::BreakBlock
                | Self::PlaceBlock
                | Self::BlockInteract
                | Self::InteractEntity
                | Self::Explosion
                | Self::ChatSend
        )
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
    let workspace_mcfunctions = ctx
        .workspace_root
        .map(discover_workspace_mcfunctions)
        .unwrap_or_default();

    let active_api = program
        .header
        .as_ref()
        .map(|h| h.api.clone())
        .or_else(|| {
            ctx.workspace_root
                .map(|root| WorkspaceConfig::load_from_root(root).script_api)
        })
        .unwrap_or_else(|| ctx.spec.script_api_version.clone());

    let mut initial_diags = Vec::new();
    let profile = match ApiProfile::from_api_version(&active_api) {
        Ok(p) => p,
        Err(msg) => {
            let span = program
                .header
                .as_ref()
                .map(|h| h.span.clone())
                .unwrap_or_else(|| Span::file_only("<input>"));
            initial_diags.push(Diagnostic::error(
                DiagnosticCode::E016,
                &span,
                msg,
                "Set '@addon(target = \"26.60\", api = \"2.11.0\")' for Modern 2.x or '@addon(target = \"1.21.50\", api = \"1.16.0\")' for Legacy 1.x LTS.",
            ));
            ApiProfile::Modern2x
        }
    };

    let mut v = Validator {
        ctx,
        active_api,
        profile,
        scopes: vec![HashMap::new()],
        const_strings: HashMap::new(),
        state_names: HashSet::new(),
        event_ctx: EventCtx::TopLevel,
        diagnostics: initial_diags,
        bound_item_ids: Vec::new(),
        bound_block_ids: Vec::new(),
        has_block_bind_target: false,
        static_event_item_ids: Vec::new(),
        static_event_block_ids: Vec::new(),
        discovered_registry,
        inline_mcfunctions: workspace_mcfunctions,
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
    active_api: String,
    profile: ApiProfile,
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
    inline_mcfunctions: HashSet<String>,
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
                TopLevelItem::McFunctionDecl(mc) => {
                    let norm = normalize_mcfunction_path(&mc.path);
                    if !has_path_traversal(&norm) {
                        self.inline_mcfunctions.insert(norm);
                    }
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
                TopLevelItem::UnsafeFuncDecl(u) => {
                    // `unsafe` contents bypass .pstdr keyword/ID checks, but Beta-only APIs
                    // are still guarded when targeting a stable `@minecraft/server` version.
                    self.validate_unsafe_beta_usage(&u.name, &u.raw_body, &u.span);
                }
                TopLevelItem::UnsafeVarDecl(u) => {
                    self.validate_unsafe_beta_usage(&u.name, &u.raw_rest, &u.span);
                }
                TopLevelItem::McFunctionDecl(mc) => {
                    let norm = normalize_mcfunction_path(&mc.path);
                    if has_path_traversal(&norm) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E009,
                            &mc.path_span,
                            format!(
                                "Path traversal or invalid path is not allowed in mcfunction '{}'.",
                                mc.path
                            ),
                            "Use a non-empty relative path without '..' or leading slashes, e.g. \"folder/my_func\".",
                        ));
                    }
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

    fn validate_unsafe_beta_usage(&mut self, name: &str, raw_code: &str, span: &Span) {
        if self.profile == ApiProfile::Modern2x {
            let mut deprecated: Vec<&String> = self.ctx.spec.deprecated_apis.iter().collect();
            deprecated.sort();
            for dep_sym in deprecated {
                if contains_ident_token(raw_code, dep_sym) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E016,
                        span,
                        format!(
                            "Unsafe declaration '{name}' uses removed/deprecated API '{dep_sym}', which is unavailable in '@minecraft/server' '{}' ({}).",
                            self.active_api,
                            self.profile.label()
                        ),
                        "In @minecraft/server 2.x, replace 'runCommandAsync(...)' with 'runCommand(...)' (or use .pstdr 'command on <target> \"...\"' / 'run_fn'), or target Legacy 1.x LTS ('1.16.0').",
                    ));
                }
            }
        }
        if is_beta_api_version(&self.active_api) {
            return;
        }
        let mut beta_events: Vec<&String> = self.ctx.spec.beta_events.iter().collect();
        beta_events.sort();
        for beta_ev in beta_events {
            if !self.profile.is_chat_send_beta_only() && beta_ev.eq_ignore_ascii_case("chatSend") {
                continue;
            }
            if text_uses_beta_event(raw_code, beta_ev) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E018,
                    span,
                    format!(
                        "Unsafe declaration '{name}' uses Beta-only event '{beta_ev}', which is unavailable in stable '@minecraft/server' '{}'.",
                        self.active_api
                    ),
                    "Set '@addon(target = \"26.60\", api = \"2.11.0-beta\")' and enable the Beta APIs experiment in world settings, or remove the Beta-only event.",
                ));
            }
        }
        let mut beta_apis: Vec<&String> = self.ctx.spec.beta_apis.iter().collect();
        beta_apis.sort();
        for beta_sym in beta_apis {
            if text_uses_beta_api(raw_code, beta_sym) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::E018,
                    span,
                    format!(
                        "Unsafe declaration '{name}' uses Beta-only API '{beta_sym}', which is unavailable in stable '@minecraft/server' '{}'.",
                        self.active_api
                    ),
                    "Set '@addon(target = \"26.60\", api = \"2.11.0-beta\")' and enable the Beta APIs experiment in world settings, or remove the Beta-only API call.",
                ));
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
            EventKind::PlayerLeave { actor_var } => {
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::PlayerLeave;
            }
            EventKind::ChatSend { msg_var, actor_var } => {
                if self.profile.is_chat_send_beta_only()
                    && self.ctx.spec.is_beta_event("chatSend")
                    && !is_beta_api_version(&self.active_api)
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E018,
                        &ev.span,
                        format!(
                            "'on chat' compiles to 'world.beforeEvents.chatSend', which is Beta-only and unavailable in stable '@minecraft/server' '{}'.",
                            self.active_api
                        ),
                        "Use 'on cmd' for stable 2.x slash commands, set '@addon(target = \"26.60\", api = \"2.11.0-beta\")' (+ enable Beta APIs), or target Legacy 1.x LTS ('api = \"1.16.0\"') where 'chatSend' is stable.",
                    ));
                }
                self.insert_binding(msg_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::ChatSend;
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
            EventKind::ScriptEvent { event_id } => {
                self.validate_expr(event_id);
                match eval_static_string(event_id, &self.const_strings) {
                    Some(id_str) => {
                        let valid = id_str.split_once(':').is_some_and(|(ns, local)| {
                            !ns.trim().is_empty()
                                && !local.trim().is_empty()
                                && !ns.chars().any(char::is_whitespace)
                                && !local.chars().any(char::is_whitespace)
                        });
                        if !valid {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                event_id.span(),
                                format!(
                                    "Invalid script_event ID '{id_str}': must include a namespace colon in '<namespace>:<id>' format."
                                ),
                                "Use a namespaced identifier such as on script_event \"my_pack:my_event\" { ... }.",
                            ));
                        }
                    }
                    None => {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            event_id.span(),
                            "script_event ID must be a static string literal or const in '<namespace>:<id>' format.",
                            "Use a namespaced string literal or const, e.g. on script_event \"my_pack:my_event\" { ... }.",
                        ));
                    }
                }
                self.insert_binding("event", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("source", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("source_block", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::ScriptEvent;
            }
            EventKind::EntityTrigger {
                event_id,
                entity_type,
            } => {
                self.validate_expr(event_id);
                if let Some(ty) = entity_type {
                    self.validate_expr(ty);
                }
                self.insert_binding("entity", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("event_id", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::EntityTrigger;
            }
            EventKind::CustomCommand {
                command_id,
                description,
                params,
                actor_var,
                ..
            } => {
                if !self.profile.supports_custom_commands() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::E016,
                        &ev.span,
                        format!(
                            "'on cmd' (customCommandRegistry) is not supported on Legacy 1.x ('@minecraft/server' '{}').",
                            self.active_api
                        ),
                        "Use 'on chat' on Legacy 1.x LTS, or upgrade your target to '@addon(target = \"26.60\", api = \"2.11.0\")' to use 'on cmd'.",
                    ));
                }
                self.validate_expr(command_id);
                self.validate_expr(description);
                match eval_static_string(command_id, &self.const_strings) {
                    Some(id_str) => {
                        let valid = id_str.split_once(':').is_some_and(|(ns, local)| {
                            !ns.trim().is_empty()
                                && !local.trim().is_empty()
                                && !ns.chars().any(char::is_whitespace)
                                && !local.chars().any(char::is_whitespace)
                        });
                        if !valid {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                command_id.span(),
                                format!(
                                    "Invalid custom command ID '{id_str}': must include a namespace colon in '<namespace>:<name>' format."
                                ),
                                "Use a namespaced command name such as on cmd \"my_pack:guess\" description \"...\" { ... }.",
                            ));
                        }
                    }
                    None => {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            command_id.span(),
                            "Custom command ID must be a static string literal or const in '<namespace>:<name>' format.",
                            "Use a namespaced string literal or const, e.g. on cmd \"my_pack:guess\" description \"...\" { ... }.",
                        ));
                    }
                }
                let mut seen_params = HashSet::new();
                let mut seen_optional = false;
                for p in params {
                    if !seen_params.insert(p.name.as_str()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            &p.span,
                            format!("Duplicate command parameter '{}'.", p.name),
                            "Give each command parameter a unique name.",
                        ));
                    }
                    if p.optional {
                        seen_optional = true;
                    } else if seen_optional {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            &p.span,
                            format!(
                                "Required command parameter '{}' cannot appear after an optional ('?') parameter.",
                                p.name
                            ),
                            "Place all required parameters before optional ('?') parameters.",
                        ));
                    }
                    self.insert_binding(&p.name, BindingKind::ContextVar, p.span.line);
                }
                if let Some(a) = actor_var {
                    self.insert_binding(a, BindingKind::ContextVar, ev.span.line);
                }
                self.event_ctx = EventCtx::CustomCommand;
            }
            EventKind::PlaceBlock {
                block_var,
                actor_var,
            } => {
                self.insert_binding(block_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::PlaceBlock;
            }
            EventKind::DimensionChange { player_var } => {
                self.insert_binding(player_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("from_dimension", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("to_dimension", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("from_location", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("to_location", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::DimensionChange;
            }
            EventKind::HealthChange { entity_var } => {
                self.insert_binding(entity_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("old_value", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("new_value", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::HealthChange;
            }
            EventKind::InteractEntity {
                target_var,
                actor_var,
            } => {
                self.insert_binding(target_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::InteractEntity;
            }
            EventKind::ProjectileHitBlock {
                block_var,
                shooter_var,
            } => {
                self.insert_binding(block_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("projectile", BindingKind::ContextVar, ev.span.line);
                let shooter_name = shooter_var.as_deref().unwrap_or("shooter");
                self.insert_binding(shooter_name, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::ProjectileHitBlock;
            }
            EventKind::EntityHitBlock {
                block_var,
                attacker_var,
            } => {
                self.insert_binding(block_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding(attacker_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::EntityHitBlock;
            }
            EventKind::ItemRelease { item, actor_var } => {
                self.validate_expr(item);
                if let Some(s) = eval_static_string(item, &self.const_strings) {
                    self.static_event_item_ids
                        .push((s, item.span().clone()));
                }
                self.insert_binding(actor_var, BindingKind::ContextVar, ev.span.line);
                self.insert_binding("item", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("use_duration", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::ItemRelease;
            }
            EventKind::Explosion => {
                self.insert_binding("source", BindingKind::ContextVar, ev.span.line);
                self.insert_binding("impacted_blocks", BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::Explosion;
            }
            EventKind::EntityLoad { entity_var } => {
                self.insert_binding(entity_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::EntityLoad;
            }
            EventKind::EntityRemove { entity_var } => {
                self.insert_binding(entity_var, BindingKind::ContextVar, ev.span.line);
                self.event_ctx = EventCtx::EntityRemove;
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
                        if let Expr::Index {
                            object: inner_obj,
                            index: obj_id_expr,
                            ..
                        } = object
                        {
                            if matches!(inner_obj.as_ref(), Expr::Ident(n, _) if n == "score")
                                && self.lookup_binding("score").is_none()
                            {
                                self.validate_expr(obj_id_expr);
                                self.validate_expr(index);
                                return;
                            }
                        }
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
                        "Move 'cancel event' into an 'on right click', 'on break', 'on place', 'on interact', 'on interact_entity', 'on explosion', or 'on chat' block.",
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
            Stmt::Command {
                target, command, ..
            } => {
                if let Some(t) = target {
                    self.validate_expr(t);
                }
                self.validate_expr(command);
            }
            Stmt::RunFn { target, path, .. } => {
                if let Some(t) = target {
                    self.validate_expr(t);
                }
                self.validate_expr(path);
                if let Some(raw_path) = eval_static_string(path, &self.const_strings) {
                    let norm = normalize_mcfunction_path(&raw_path);
                    if has_path_traversal(&norm) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E009,
                            path.span(),
                            format!(
                                "Path traversal or invalid path is not allowed in run_fn '{}'.",
                                raw_path
                            ),
                            "Use a non-empty relative path without '..' or leading slashes, e.g. run_fn \"folder/my_func\".",
                        ));
                    } else {
                        let exists_inline = self.inline_mcfunctions.contains(&norm);
                        let exists_on_disk = self.ctx.workspace_root.is_some_and(|root| {
                            root.join("BP/functions")
                                .join(format!("{norm}.mcfunction"))
                                .is_file()
                        });
                        if !exists_inline && !exists_on_disk {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E017,
                                path.span(),
                                format!(
                                    "Referenced mcfunction '{norm}' not found: neither 'BP/functions/{norm}.mcfunction' nor an inline 'mcfunction \"{norm}\" {{ ... }}' declaration exists in the workspace."
                                ),
                                format!(
                                    "Create 'BP/functions/{norm}.mcfunction' or add an inline 'mcfunction \"{norm}\" {{ ... }}' block."
                                ),
                            ));
                        }
                    }
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
            Stmt::UiAction {
                title,
                body,
                target,
                buttons,
                ..
            } => {
                self.validate_expr(title);
                if let Some(b) = body {
                    self.validate_expr(b);
                }
                self.validate_expr(target);
                let prev = self.event_ctx;
                self.event_ctx = EventCtx::Func;
                for btn in buttons {
                    self.validate_expr(&btn.label);
                    if let Some(icon_expr) = &btn.icon {
                        self.validate_expr(icon_expr);
                        if let Some(icon_path) = eval_static_string(icon_expr, &self.const_strings) {
                            self.validate_ui_icon_path(&icon_path, icon_expr.span());
                        }
                    }
                    self.push_scope();
                    self.validate_block(&btn.body);
                    self.pop_scope();
                }
                self.event_ctx = prev;
            }
            Stmt::UiMessage {
                title,
                body,
                target,
                button1,
                button2,
                ..
            } => {
                self.validate_expr(title);
                if let Some(b) = body {
                    self.validate_expr(b);
                }
                self.validate_expr(target);
                self.validate_expr(&button1.label);
                self.validate_expr(&button2.label);
                let prev = self.event_ctx;
                self.event_ctx = EventCtx::Func;
                self.push_scope();
                self.validate_block(&button1.body);
                self.pop_scope();
                self.push_scope();
                self.validate_block(&button2.body);
                self.pop_scope();
                self.event_ctx = prev;
            }
            Stmt::UiModal {
                title,
                target,
                fields,
                on_submit,
                ..
            } => {
                self.validate_expr(title);
                self.validate_expr(target);
                let mut seen_bindings = HashSet::new();
                for f in fields {
                    match &f.kind {
                        UiModalFieldKind::TextField {
                            label,
                            placeholder,
                            default,
                        } => {
                            self.validate_expr(label);
                            if let Some(p) = placeholder {
                                self.validate_expr(p);
                            }
                            if let Some(d) = default {
                                self.validate_expr(d);
                            }
                        }
                        UiModalFieldKind::Toggle { label, default } => {
                            self.validate_expr(label);
                            if let Some(d) = default {
                                self.validate_expr(d);
                            }
                        }
                        UiModalFieldKind::Slider {
                            label,
                            min,
                            max,
                            step,
                            default,
                        } => {
                            self.validate_expr(label);
                            self.validate_expr(min);
                            self.validate_expr(max);
                            self.validate_expr(step);
                            if let Some(d) = default {
                                self.validate_expr(d);
                            }
                        }
                        UiModalFieldKind::Dropdown {
                            label,
                            options,
                            default,
                        } => {
                            self.validate_expr(label);
                            self.validate_expr(options);
                            if let Some(d) = default {
                                self.validate_expr(d);
                            }
                        }
                    }
                    if !seen_bindings.insert(f.binding.as_str()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            &f.binding_span,
                            format!("Duplicate modal field binding '{}'.", f.binding),
                            "Give each modal field a unique 'as <ident>' binding name.",
                        ));
                    }
                }
                let prev = self.event_ctx;
                self.event_ctx = EventCtx::Func;
                self.push_scope();
                for f in fields {
                    self.insert_binding(&f.binding, BindingKind::ContextVar, f.binding_span.line);
                }
                self.validate_block(on_submit);
                self.pop_scope();
                self.event_ctx = prev;
            }
            Stmt::ScoreboardShow {
                objective, title, ..
            } => {
                self.validate_expr(objective);
                if let Some(t) = title {
                    self.validate_expr(t);
                }
            }
            Stmt::ScoreboardHide { .. } => {}
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
                if let Expr::Index {
                    object: inner_obj,
                    index: obj_id_expr,
                    ..
                } = object.as_ref()
                {
                    if matches!(inner_obj.as_ref(), Expr::Ident(n, _) if n == "score")
                        && self.lookup_binding("score").is_none()
                    {
                        self.validate_expr(obj_id_expr);
                        self.validate_expr(index);
                        return;
                    }
                }
                self.validate_expr(object);
                self.validate_expr(index);
            }
            Expr::Member { object, .. } => {
                self.validate_expr(object);
            }
            Expr::Call { callee, args, span } => {
                if let Expr::Member { object, field, .. } = callee.as_ref() {
                    // Validate synthesized helper arity
                    match field.as_str() {
                        "title" if args.len() != 1 => {
                            let solution = if args.len() == 2 {
                                "In Bedrock Script API, 'title(msg)' accepts 1 argument. Call '.subtitle(...)' separately to display a subtitle."
                            } else {
                                "Pass a single title message, e.g. player.title(\"Welcome!\"). Use player.subtitle(\"...\") separately for a subtitle."
                            };
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Method 'title()' expects 1 argument (found {}).",
                                    args.len()
                                ),
                                solution,
                            ));
                        }
                        "subtitle" if args.len() != 1 => {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Method 'subtitle()' expects 1 argument (found {}).",
                                    args.len()
                                ),
                                "Pass a single subtitle message, e.g. player.subtitle(\"Wave 1\").",
                            ));
                        }
                        "actionbar" if args.len() != 1 => {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Method 'actionbar()' expects 1 argument (found {}).",
                                    args.len()
                                ),
                                "Pass a single actionbar message, e.g. player.actionbar(\"§aReady!\").",
                            ));
                        }
                        "give_item" if args.is_empty() || args.len() > 3 => {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Method 'give_item()' expects 1 to 3 arguments: (item_id, amount?, name_tag?) (found {}).",
                                    args.len()
                                ),
                                "Example: player.give_item(\"minecraft:diamond\", 1) or player.give_item(\"minecraft:diamond\", 1, \"§bGem\").",
                            ));
                        }
                        _ => {}
                    }

                    // Reject removed/deprecated runCommandAsync in Modern 2.x
                    if self.profile == ApiProfile::Modern2x
                        && (field == "runCommandAsync"
                            || field == "run_command_async"
                            || self.ctx.spec.is_deprecated_api(field))
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::E016,
                            span,
                            format!(
                                "Method '{field}()' was removed in '@minecraft/server' '{}' (Modern 2.x).",
                                self.active_api
                            ),
                            "Use 'command on <target> \"...\"', 'run_fn', or '.run_command(...)' (which emits 'runCommand' on 2.x), or target Legacy 1.x LTS ('api = \"1.16.0\"').",
                        ));
                    }

                    // Validate world.* and system.* method calls against BedrockSpec
                    if let Expr::Ident(obj_name, _) = object.as_ref() {
                        if obj_name == "world"
                            && self
                                .lookup_binding("world")
                                .is_some_and(|b| b.kind == BindingKind::Builtin)
                            && !self.ctx.spec.is_valid_world_method(field)
                        {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Unknown method 'world.{field}()' in @minecraft/server ({}).",
                                    self.ctx.spec.script_api_version
                                ),
                                self.ctx.spec.suggest_world_method(field),
                            ));
                        } else if obj_name == "system"
                            && self
                                .lookup_binding("system")
                                .is_some_and(|b| b.kind == BindingKind::Builtin)
                            && !self.ctx.spec.is_valid_system_method(field)
                        {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::E016,
                                span,
                                format!(
                                    "Unknown method 'system.{field}()' in @minecraft/server ({}).",
                                    self.ctx.spec.script_api_version
                                ),
                                self.ctx.spec.suggest_system_method(field),
                            ));
                        }
                    }

                    // Check vanilla sound ID in play_sound("...") / playSound("...")
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

    fn validate_ui_icon_path(&mut self, icon_path: &str, span: &Span) {
        let trimmed = icon_path.trim();
        if has_path_traversal(trimmed) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::E009,
                span,
                format!("Path traversal or invalid path is not allowed in UI icon '{icon_path}'."),
                "Use a relative texture path without '..' or leading slashes, e.g. \"textures/items/iron_sword\".",
            ));
            return;
        }
        let norm = trimmed.replace('\\', "/");
        let without_ext = norm
            .strip_suffix(".png")
            .or_else(|| norm.strip_suffix(".tga"))
            .or_else(|| norm.strip_suffix(".jpg"))
            .unwrap_or(&norm);

        let is_vanilla = if let Some(item_name) = without_ext.strip_prefix("textures/items/") {
            !item_name.contains('/') && self.ctx.spec.is_valid_vanilla_item_texture(item_name)
        } else if let Some(block_name) = without_ext.strip_prefix("textures/blocks/") {
            !block_name.contains('/') && self.ctx.spec.is_valid_vanilla_terrain_texture(block_name)
        } else if let Some(ui_name) = without_ext.strip_prefix("textures/ui/") {
            !ui_name.is_empty() && !ui_name.contains('/')
        } else {
            false
        };

        if is_vanilla {
            return;
        }

        let exists_in_rp = self
            .ctx
            .workspace_root
            .is_some_and(|root| crate::assets::rp_texture_file_exists(root, trimmed));
        if !exists_in_rp {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::E017,
                span,
                format!(
                    "Custom UI icon texture '{icon_path}' not found in RP/ and is not a recognized vanilla texture path."
                ),
                "Add the image file under RP/ (e.g. RP/textures/...) or use a valid vanilla path such as \"textures/items/iron_sword\".",
            ));
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

pub fn normalize_mcfunction_path(raw: &str) -> String {
    let normalized = raw.trim().replace('\\', "/");
    normalized
        .strip_suffix(".mcfunction")
        .unwrap_or(&normalized)
        .to_string()
}

fn discover_workspace_mcfunctions(root: &Path) -> HashSet<String> {
    let mut found = HashSet::new();
    let cfg = WorkspaceConfig::load_from_root(root);
    let src_dir = root.join(&cfg.src_dir);
    let scan_dir = if src_dir.is_dir() {
        src_dir
    } else {
        root.to_path_buf()
    };
    let Ok(files) = crate::workspace::collect_pstdr_files(&scan_dir) else {
        return found;
    };
    for file in files {
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        if !source.contains("mcfunction") {
            continue;
        }
        let label = file.display().to_string();
        let (prog_opt, _) = crate::parser::parse_source(&source, &label);
        if let Some(prog) = prog_opt {
            for item in prog.items {
                if let TopLevelItem::McFunctionDecl(mc) = item {
                    let norm = normalize_mcfunction_path(&mc.path);
                    if !has_path_traversal(&norm) {
                        found.insert(norm);
                    }
                }
            }
        }
    }
    found
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
