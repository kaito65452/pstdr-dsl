use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const BUNDLED_SPEC_JSON: &str = include_str!("../../../data/spec/bedrock_api_latest.json");

#[derive(Debug, Clone, Deserialize)]
pub struct BedrockSpecRaw {
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
        Ok(Self {
            bedrock_version: raw.bedrock_version,
            script_api_version: raw.script_api_version,
            min_engine_version,
            item_format_version: raw
                .item_format_version
                .unwrap_or_else(|| "1.21.90".to_string()),
            block_format_version: raw
                .block_format_version
                .unwrap_or_else(|| "1.21.90".to_string()),
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
        Self::from_json_str(BUNDLED_SPEC_JSON, "embedded:data/spec/bedrock_api_latest.json")
            .expect("Bundled bedrock_api_latest.json must be valid JSON")
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

/// Mapping of snake_case methods on entities/players -> @minecraft/server.
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
        _ => None,
    }
}

/// Method mapping on the global `world` object.
pub fn map_world_method(method: &str) -> Option<&'static str> {
    match method {
        "players" => Some("getAllPlayers"),
        "send_message" => Some("sendMessage"),
        "dimension" => Some("getDimension"),
        "play_sound" => Some("playSound"),
        _ => None,
    }
}
