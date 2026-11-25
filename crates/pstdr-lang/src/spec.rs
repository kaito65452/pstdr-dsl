use crate::ast::ApiProfile;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub const BUNDLED_SPEC_JSON: &str = include_str!("../../../data/spec/bedrock_api_latest.json");
pub const BUNDLED_LEGACY_SPEC_JSON: &str =
    include_str!("../../../data/spec/bedrock_api_v1_legacy.json");

#[derive(Debug, Clone, Deserialize)]
pub struct BedrockSpecRaw {
    #[serde(default)]
    pub spec_version: Option<String>,
    #[serde(default)]
    pub minecraft_version: Option<String>,
    #[serde(default)]
    pub server_api: Option<String>,
    #[serde(default)]
    pub server_ui_api: Option<String>,
    pub bedrock_version: String,
    pub script_api_version: String,
    /// Official `min_engine_version` for Bedrock manifests — always starts with 1,
    /// e.g. `[1, 26, 60]` for target `26.60`.
    #[serde(default)]
    pub min_engine_version: Option<[u64; 3]>,
    /// `format_version` for item JSON (e.g. `"1.21.90"`).
    #[serde(default)]
    pub item_format_version: Option<String>,
    /// `format_version` for block JSON (e.g. `"1.21.90"`).
    #[serde(default)]
    pub block_format_version: Option<String>,
    /// Stable `world.beforeEvents` names from Mojang metadata.
    #[serde(default)]
    pub stable_before_events: Vec<String>,
    /// Stable `world.afterEvents` names from Mojang metadata.
    #[serde(default)]
    pub stable_after_events: Vec<String>,
    /// `@minecraft/server` event names that require a `-beta` Script API version.
    #[serde(default)]
    pub beta_events: Option<Vec<String>>,
    /// Event names executed in read-only / restricted execution mode.
    #[serde(default)]
    pub read_only_events: Vec<String>,
    /// Class-qualified Beta-only APIs (`ClassName.memberName`).
    #[serde(default)]
    pub beta_apis: Vec<String>,
    /// Removed/deprecated APIs in this profile (e.g. `runCommandAsync` in 2.x).
    #[serde(default)]
    pub deprecated_apis: Vec<String>,
    /// Valid `@minecraft/server` `World` methods (canonical camelCase).
    #[serde(default)]
    pub world_methods: Option<Vec<String>>,
    /// Valid `@minecraft/server` `System` methods (canonical camelCase).
    #[serde(default)]
    pub system_methods: Option<Vec<String>>,
    /// Symbol catalog keyed by `@minecraft/server` class/enum name.
    #[serde(default)]
    pub server_symbols: HashMap<String, Vec<String>>,
    /// Supported `@minecraft/server-ui` form classes.
    #[serde(default)]
    pub server_ui_classes: Vec<String>,
    pub effects: Vec<String>,
    pub sounds: Vec<String>,
    #[serde(default)]
    pub item_textures: Vec<String>,
    #[serde(default)]
    pub terrain_textures: Vec<String>,
    #[serde(default)]
    pub vanilla_items: Vec<String>,
    #[serde(default)]
    pub vanilla_blocks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BedrockSpec {
    pub spec_version: String,
    pub minecraft_version: String,
    pub server_ui_api_version: String,
    pub bedrock_version: String,
    pub script_api_version: String,
    /// `min_engine_version` for `BP/manifest.json` — always `[1, x, x]`.
    pub min_engine_version: [u64; 3],
    /// `format_version` for item JSON.
    pub item_format_version: String,
    /// `format_version` for block JSON.
    pub block_format_version: String,
    /// Stable `world.beforeEvents` names (lowercased).
    pub stable_before_events: HashSet<String>,
    /// Stable `world.afterEvents` names (lowercased).
    pub stable_after_events: HashSet<String>,
    /// `@minecraft/server` event names that require a `-beta` API version (canonical casing preserved).
    pub beta_events: HashSet<String>,
    /// Lowercased event names executed in read-only mode.
    pub read_only_events: HashSet<String>,
    /// Class-qualified Beta-only APIs (`ClassName.memberName`, exact case preserved).
    pub beta_apis: HashSet<String>,
    /// Removed/deprecated APIs in this profile (exact case preserved).
    pub deprecated_apis: HashSet<String>,
    /// Valid `@minecraft/server` `World` methods (canonical camelCase).
    pub world_methods: HashSet<String>,
    /// Valid `@minecraft/server` `System` methods (canonical camelCase).
    pub system_methods: HashSet<String>,
    /// Symbol catalog keyed by `@minecraft/server` class/enum name.
    pub server_symbols: HashMap<String, Vec<String>>,
    /// Supported `@minecraft/server-ui` form classes.
    pub server_ui_classes: HashSet<String>,
    pub effects: HashSet<String>,
    pub sounds: HashSet<String>,
    pub item_textures: HashSet<String>,
    pub terrain_textures: HashSet<String>,
    pub vanilla_items: HashSet<String>,
    pub vanilla_blocks: HashSet<String>,
    pub source_label: String,
}

impl BedrockSpec {
    pub fn from_json_str(json: &str, source_label: impl Into<String>) -> Result<Self, String> {
        let raw: BedrockSpecRaw =
            serde_json::from_str(json).map_err(|e| format!("Failed to parse spec JSON: {e}"))?;
        // Compute min_engine_version: use the explicit JSON value if present,
        // otherwise derive from bedrock_version using official Bedrock rules.
        let min_engine_version = raw.min_engine_version.unwrap_or_else(|| {
            crate::wiring::resolve_min_engine_version(&raw.bedrock_version)
                .unwrap_or([1, 26, 60])
        });
        let beta_events: HashSet<String> = raw
            .beta_events
            .unwrap_or_else(|| vec!["chatSend".to_string()])
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let world_methods: HashSet<String> = raw
            .world_methods
            .unwrap_or_else(default_world_methods)
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let system_methods: HashSet<String> = raw
            .system_methods
            .unwrap_or_else(default_system_methods)
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let profile = ApiProfile::resolve_or_default(
            raw.server_api.as_deref().unwrap_or(&raw.script_api_version),
        );
        let spec_version = raw
            .spec_version
            .unwrap_or_else(|| raw.script_api_version.clone());
        let minecraft_version = raw
            .minecraft_version
            .unwrap_or_else(|| raw.bedrock_version.clone());
        let server_ui_api_version = raw
            .server_ui_api
            .unwrap_or_else(|| profile.server_ui_version().to_string());
        let deprecated_apis: HashSet<String> = raw
            .deprecated_apis
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let server_ui_classes: HashSet<String> = raw
            .server_ui_classes
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            spec_version,
            minecraft_version,
            server_ui_api_version,
            bedrock_version: raw.bedrock_version,
            script_api_version: raw.script_api_version,
            min_engine_version,
            item_format_version: raw
                .item_format_version
                .unwrap_or_else(|| profile.default_component_format_version().to_string()),
            block_format_version: raw
                .block_format_version
                .unwrap_or_else(|| profile.default_component_format_version().to_string()),
            stable_before_events: raw
                .stable_before_events
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .collect(),
            stable_after_events: raw
                .stable_after_events
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .collect(),
            beta_events,
            read_only_events: raw
                .read_only_events
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .collect(),
            beta_apis: raw
                .beta_apis
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            deprecated_apis,
            world_methods,
            system_methods,
            server_symbols: raw.server_symbols,
            server_ui_classes,
            effects: raw.effects.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            sounds: raw.sounds.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            item_textures: raw.item_textures.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            terrain_textures: raw.terrain_textures.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            vanilla_items: raw
                .vanilla_items
                .into_iter()
                .map(|s| {
                    s.trim()
                        .strip_prefix("minecraft:")
                        .unwrap_or(s.trim())
                        .to_ascii_lowercase()
                })
                .collect(),
            vanilla_blocks: raw
                .vanilla_blocks
                .into_iter()
                .map(|s| {
                    s.trim()
                        .strip_prefix("minecraft:")
                        .unwrap_or(s.trim())
                        .to_ascii_lowercase()
                })
                .collect(),
            source_label: source_label.into(),
        })
    }

    pub fn bundled() -> Self {
        Self::for_profile(ApiProfile::Modern2x)
    }

    pub fn for_profile(profile: ApiProfile) -> Self {
        match profile {
            ApiProfile::Modern2x => Self::from_json_str(
                BUNDLED_SPEC_JSON,
                "embedded:data/spec/bedrock_api_latest.json",
            )
            .expect("Bundled bedrock_api_latest.json must be valid JSON"),
            ApiProfile::Legacy1x => Self::from_json_str(
                BUNDLED_LEGACY_SPEC_JSON,
                "embedded:data/spec/bedrock_api_v1_legacy.json",
            )
            .expect("Bundled bedrock_api_v1_legacy.json must be valid JSON"),
        }
    }

    pub fn load(custom_spec: Option<&Path>, workspace_root: Option<&Path>) -> Result<Self, String> {
        if let Some(p) = custom_spec {
            let text = fs::read_to_string(p)
                .map_err(|e| format!("Failed to read spec '{}': {e}", p.display()))?;
            return Self::from_json_str(&text, p.display().to_string());
        }
        if let Some(root) = workspace_root {
            let local = root.join("data/spec/bedrock_api_latest.json");
            if local.is_file() {
                let text = fs::read_to_string(&local)
                    .map_err(|e| format!("Failed to read spec '{}': {e}", local.display()))?;
                return Self::from_json_str(&text, local.display().to_string());
            }
        }
        Ok(Self::bundled())
    }

    pub fn is_valid_effect(&self, effect_id: &str) -> bool {
        let norm = effect_id
            .trim()
            .strip_prefix("minecraft:")
            .unwrap_or(effect_id.trim())
            .to_ascii_lowercase();
        self.effects.contains(&norm)
    }

    pub fn is_valid_sound(&self, sound_id: &str) -> bool {
        let norm = sound_id
            .trim()
            .strip_prefix("minecraft:")
            .unwrap_or(sound_id.trim())
            .to_ascii_lowercase();
        self.sounds.contains(&norm)
    }

    pub fn is_valid_vanilla_item_texture(&self, alias: &str) -> bool {
        let norm = alias
            .trim()
            .strip_prefix("minecraft:")
            .unwrap_or(alias.trim())
            .to_ascii_lowercase();
        self.item_textures.contains(&norm) || self.vanilla_items.contains(&norm)
    }

    pub fn is_valid_vanilla_terrain_texture(&self, alias: &str) -> bool {
        let norm = alias
            .trim()
            .strip_prefix("minecraft:")
            .unwrap_or(alias.trim())
            .to_ascii_lowercase();
        self.terrain_textures.contains(&norm) || self.vanilla_blocks.contains(&norm)
    }

    pub fn is_beta_event(&self, event_name: &str) -> bool {
        let trimmed = event_name.trim();
        self.beta_events.contains(trimmed)
            || self
                .beta_events
                .iter()
                .any(|e| e.eq_ignore_ascii_case(trimmed))
    }

    pub fn is_beta_api(&self, symbol: &str) -> bool {
        let trimmed = symbol.trim();
        self.beta_apis.contains(trimmed)
            || self
                .beta_apis
                .iter()
                .any(|s| s.eq_ignore_ascii_case(trimmed))
    }

    pub fn is_deprecated_api(&self, symbol: &str) -> bool {
        let trimmed = symbol.trim();
        self.deprecated_apis.contains(trimmed)
            || self
                .deprecated_apis
                .iter()
                .any(|s| s.eq_ignore_ascii_case(trimmed))
    }

    pub fn has_server_symbol(&self, class_name: &str, member: &str) -> bool {
        self.server_symbols
            .get(class_name)
            .is_some_and(|members| members.iter().any(|m| m == member))
    }

    pub fn has_server_ui_class(&self, class_name: &str) -> bool {
        self.server_ui_classes.contains(class_name.trim())
    }

    pub fn is_valid_world_method(&self, method: &str) -> bool {
        let canonical = map_world_method(method).unwrap_or(method);
        self.world_methods.contains(canonical)
    }

    pub fn is_valid_system_method(&self, method: &str) -> bool {
        let canonical = map_system_method(method).unwrap_or(method);
        self.system_methods.contains(canonical)
    }

    pub fn suggest_world_method(&self, method: &str) -> String {
        let lower = method.to_ascii_lowercase();
        if lower.contains("time") || lower.contains("tick") || lower.contains("clock") {
            return "Did you mean 'system.current_tick', 'world.get_absolute_time()', or 'world.get_time_of_day()'?".to_string();
        }
        if lower.contains("player") {
            return "Did you mean 'world.players()' (getAllPlayers) or 'world.get_players(...)' (getPlayers)?".to_string();
        }
        if lower.contains("dim") {
            return "Did you mean 'world.dimension(\"overworld\")' (getDimension)?".to_string();
        }
        if lower.contains("msg") || lower.contains("message") || lower.contains("say") || lower.contains("broadcast") {
            return "Did you mean 'world.send_message(...)' (sendMessage)?".to_string();
        }
        if lower.contains("sound") || lower.contains("music") {
            return "Did you mean 'world.play_sound(...)', 'world.play_music(...)', 'world.queue_music(...)', or 'world.stop_music()'?".to_string();
        }
        "Use a valid @minecraft/server World method such as world.players(), world.dimension(\"overworld\"), world.send_message(...), world.get_absolute_time(), or world.get_time_of_day().".to_string()
    }

    pub fn suggest_system_method(&self, method: &str) -> String {
        let lower = method.to_ascii_lowercase();
        if lower.contains("time") || lower.contains("tick") {
            return "Did you mean 'system.current_tick' (property), 'system.wait_ticks(n)', 'world.get_absolute_time()', or 'world.get_time_of_day()'?".to_string();
        }
        "Use a valid @minecraft/server System method such as system.run(...), system.run_timeout(...), system.run_interval(...), system.clear_run(...), system.send_script_event(...), or system.wait_ticks(...).".to_string()
    }
}

fn default_world_methods() -> Vec<String> {
    [
        "broadcastClientMessage",
        "clearDynamicProperties",
        "getAbsoluteTime",
        "getAllPlayers",
        "getDay",
        "getDefaultSpawnLocation",
        "getDimension",
        "getDynamicProperty",
        "getDynamicPropertyIds",
        "getDynamicPropertyTotalByteCount",
        "getEntity",
        "getLootTableManager",
        "getMoonPhase",
        "getPlayers",
        "getTimeOfDay",
        "playMusic",
        "playSound",
        "queueMusic",
        "sendMessage",
        "setAbsoluteTime",
        "setDefaultSpawnLocation",
        "setDynamicProperty",
        "setTimeOfDay",
        "stopMusic",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_system_methods() -> Vec<String> {
    [
        "clearJob",
        "clearRun",
        "run",
        "runInterval",
        "runJob",
        "runTimeout",
        "sendScriptEvent",
        "waitTicks",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Returns true when a `@minecraft/server` API version string targets a Beta release (e.g. `"2.11.0-beta"`).
pub fn is_beta_api_version(api: &str) -> bool {
    api.trim().to_ascii_lowercase().ends_with("-beta")
}

fn is_js_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Returns true when `needle` occurs in `haystack` bounded by non-identifier characters.
pub fn contains_ident_token(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let n_len = needle.len();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let left_ok = abs == 0 || !is_js_ident_char(bytes[abs - 1]);
        let right_idx = abs + n_len;
        let right_ok = right_idx >= bytes.len() || !is_js_ident_char(bytes[right_idx]);
        if left_ok && right_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Returns true when `text` references `beforeEvents.<beta_ev>` or `afterEvents.<beta_ev>`.
pub fn text_uses_beta_event(text: &str, beta_ev: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let ev_lower = beta_ev.to_ascii_lowercase();
    contains_ident_token(&lower, &format!("beforeevents.{ev_lower}"))
        || contains_ident_token(&lower, &format!("afterevents.{ev_lower}"))
}

/// Returns true when `text` references a class-qualified Beta-only API (`ClassName.memberName`),
/// including singleton aliases (`World.x` -> `world.x`, `System.x` -> `system.x`,
/// `SystemBeforeEvents.x` -> `beforeEvents.x`, `SystemAfterEvents.x` -> `afterEvents.x`).
pub fn text_uses_beta_api(text: &str, beta_sym: &str) -> bool {
    if contains_ident_token(text, beta_sym) {
        return true;
    }
    let Some((cls, member)) = beta_sym.split_once('.') else {
        return false;
    };
    match cls {
        "World" => contains_ident_token(text, &format!("world.{member}")),
        "System" => contains_ident_token(text, &format!("system.{member}")),
        "SystemBeforeEvents" => contains_ident_token(text, &format!("beforeEvents.{member}")),
        "SystemAfterEvents" => contains_ident_token(text, &format!("afterEvents.{member}")),
        _ => false,
    }
}

/// Centralized mapping of snake_case .pstdr properties -> @minecraft/server API.
/// Properties outside this table are passed through as-is.
pub fn map_property(field: &str) -> Option<&'static str> {
    match field {
        "name_tag" => Some("nameTag"),
        "type_id" => Some("typeId"),
        "is_sneaking" => Some("isSneaking"),
        "is_sprinting" => Some("isSprinting"),
        "is_swimming" => Some("isSwimming"),
        "is_flying" => Some("isFlying"),
        "is_sleeping" => Some("isSleeping"),
        "is_on_ground" => Some("isOnGround"),
        "is_in_water" => Some("isInWater"),
        "is_climbing" => Some("isClimbing"),
        "is_falling" => Some("isFalling"),
        "is_valid" => Some("isValid"),
        "current_tick" => Some("currentTick"),
        "selected_slot" => Some("selectedSlotIndex"),
        "head_location" => Some("getHeadLocation()"),
        "view_direction" => Some("getViewDirection()"),
        "hp" => Some("getComponent(\"minecraft:health\")?.currentValue"),
        "max_hp" => Some("getComponent(\"minecraft:health\")?.effectiveMax"),
        "holding" | "mainhand" => {
            Some("getComponent(\"minecraft:equippable\")?.getEquipment(EquipmentSlot.Mainhand)")
        }
        _ => None,
    }
}

/// Property mapping on ItemStack (e.g. from `player.mainhand?.id`).
pub fn map_item_property(field: &str) -> Option<&'static str> {
    match field {
        "id" | "type_id" => Some("typeId"),
        "name_tag" => Some("nameTag"),
        _ => None,
    }
}

/// Ergonomic snake_case string and array helpers -> standard JS methods.
pub fn map_string_array_method(method: &str) -> Option<&'static str> {
    match method {
        "lower" => Some("toLowerCase"),
        "upper" => Some("toUpperCase"),
        "trim" => Some("trim"),
        "starts_with" => Some("startsWith"),
        "ends_with" => Some("endsWith"),
        "contains" => Some("includes"),
        "replace" => Some("replace"),
        "split" => Some("split"),
        "join" => Some("join"),
        "push" => Some("push"),
        _ => None,
    }
}

/// Mapping of snake_case methods on entities/players/strings/arrays -> @minecraft/server / JS.
pub fn map_method(method: &str) -> Option<&'static str> {
    match method {
        "actionbar" => Some("onScreenDisplay.setActionBar"),
        "title" => Some("onScreenDisplay.setTitle"),
        "subtitle" => Some("onScreenDisplay.updateSubtitle"),
        "send_message" => Some("sendMessage"),
        "play_sound" => Some("playSound"),
        "add_tag" => Some("addTag"),
        "remove_tag" => Some("removeTag"),
        "has_tag" => Some("hasTag"),
        "get_tags" => Some("getTags"),
        "apply_damage" => Some("applyDamage"),
        "apply_impulse" => Some("applyImpulse"),
        "apply_knockback" => Some("applyKnockback"),
        "run_command" => Some("runCommand"),
        "add_effect" => Some("addEffect"),
        "remove_effect" => Some("removeEffect"),
        "set_on_fire" => Some("setOnFire"),
        "extinguish_fire" => Some("extinguishFire"),
        "spawn_particle" => Some("spawnParticle"),
        "spawn_entity" => Some("spawnEntity"),
        "spawn_item" => Some("spawnItem"),
        "get_entities" => Some("getEntities"),
        "get_players" => Some("getPlayers"),
        "teleport" => Some("teleport"),
        "kill" => Some("kill"),
        other => map_string_array_method(other),
    }
}

/// Method mapping on the global `world` object.
pub fn map_world_method(method: &str) -> Option<&'static str> {
    match method {
        "players" | "get_all_players" => Some("getAllPlayers"),
        "get_players" => Some("getPlayers"),
        "send_message" => Some("sendMessage"),
        "dimension" | "get_dimension" => Some("getDimension"),
        "play_sound" => Some("playSound"),
        "play_music" => Some("playMusic"),
        "queue_music" => Some("queueMusic"),
        "stop_music" => Some("stopMusic"),
        "get_entity" => Some("getEntity"),
        "get_day" => Some("getDay"),
        "get_moon_phase" => Some("getMoonPhase"),
        "get_time_of_day" => Some("getTimeOfDay"),
        "set_time_of_day" => Some("setTimeOfDay"),
        "get_absolute_time" => Some("getAbsoluteTime"),
        "set_absolute_time" => Some("setAbsoluteTime"),
        "get_default_spawn_location" => Some("getDefaultSpawnLocation"),
        "set_default_spawn_location" => Some("setDefaultSpawnLocation"),
        "get_dynamic_property" => Some("getDynamicProperty"),
        "set_dynamic_property" => Some("setDynamicProperty"),
        "get_dynamic_property_ids" => Some("getDynamicPropertyIds"),
        "clear_dynamic_properties" => Some("clearDynamicProperties"),
        _ => None,
    }
}

/// Method mapping on the global `system` object.
pub fn map_system_method(method: &str) -> Option<&'static str> {
    match method {
        "run_timeout" => Some("runTimeout"),
        "run_interval" => Some("runInterval"),
        "clear_run" => Some("clearRun"),
        "run_job" => Some("runJob"),
        "clear_job" => Some("clearJob"),
        "send_script_event" => Some("sendScriptEvent"),
        "wait_ticks" => Some("waitTicks"),
        _ => None,
    }
}
