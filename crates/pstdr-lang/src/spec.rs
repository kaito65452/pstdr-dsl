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
    pub effects: Vec<String>,
    pub sounds: Vec<String>,
    #[serde(default)]
    pub item_textures: Vec<String>,
    #[serde(default)]
    pub terrain_textures: Vec<String>,
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
    pub effects: HashSet<String>,
    pub sounds: HashSet<String>,
    pub item_textures: HashSet<String>,
    pub terrain_textures: HashSet<String>,
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
            effects: raw.effects.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            sounds: raw.sounds.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            item_textures: raw.item_textures.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            terrain_textures: raw.terrain_textures.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
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
        self.item_textures.contains(&norm)
    }

    pub fn is_valid_vanilla_terrain_texture(&self, alias: &str) -> bool {
        let norm = alias
            .trim()
            .strip_prefix("minecraft:")
            .unwrap_or(alias.trim())
            .to_ascii_lowercase();
        self.terrain_textures.contains(&norm)
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
