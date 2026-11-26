use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};
use crate::partition::TargetRegistry;
use crate::spec::BedrockSpec;
use crate::workspace::WorkspaceConfig;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Valid Bedrock `minecraft:enchantable.slot` enum values.
const VALID_ENCHANTABLE_SLOTS: &[&str] = &[
    "all",
    "armor_feet",
    "armor_torso",
    "armor_head",
    "armor_legs",
    "axe",
    "bow",
    "carrot_stick",
    "cosmetic_head",
    "crossbow",
    "elytra",
    "fishing_rod",
    "flintsteel",
    "hoe",
    "mace",
    "none",
    "pickaxe",
    "shears",
    "shield",
    "shovel",
    "spear",
    "sword",
];

/// Shared in-memory index of workspace assets, parsed once per CLI invocation
/// (`check`, `convert`, `build`, `audit`) to avoid quadratic `O(N_files × N_assets)` I/O.
#[derive(Debug, Clone)]
pub struct WorkspaceAssetIndex {
    pub workspace_root: PathBuf,
    pub canonical_root: Option<PathBuf>,
    pub config: WorkspaceConfig,
    pub bp_items: BTreeMap<String, Value>,
    pub bp_blocks: BTreeMap<String, Value>,
    pub bp_recipes: BTreeMap<String, Value>,
    pub rp_attachables: BTreeMap<String, Value>,
    pub rp_sound_definitions: Option<HashSet<String>>,
    pub rp_particles: HashSet<String>,
    pub rp_item_textures: HashMap<String, Vec<String>>,
    pub rp_terrain_textures: HashMap<String, Vec<String>>,
    pub has_rp: bool,
    pub bp_manifest_raw: Option<String>,
    pub rp_manifest_raw: Option<String>,
    pub disk_mcfunctions: HashSet<String>,
    pub target_registry: TargetRegistry,
}

impl WorkspaceAssetIndex {
    pub fn build(root: &Path) -> Self {
        let canonical_root = fs::canonicalize(root).ok();
        let config = WorkspaceConfig::load_from_root(root);

        let bp_items = load_json_dir_map(root, "BP/items");
        let bp_blocks = load_json_dir_map(root, "BP/blocks");
        let bp_recipes = load_json_dir_map(root, "BP/recipes");
        let rp_attachables = load_json_dir_map(root, "RP/attachables");

        let rp_item_textures = load_texture_dict(&root.join("RP/textures/item_texture.json"));
        let rp_terrain_textures = load_texture_dict(&root.join("RP/textures/terrain_texture.json"));
        let has_rp = root.join("RP/manifest.json").is_file() || root.join("RP/textures").is_dir();

        let rp_sound_definitions = load_sound_definitions(&root.join("RP/sounds/sound_definitions.json"));
        let rp_particles = load_rp_particles(root);

        let bp_manifest_raw = fs::read_to_string(root.join("BP/manifest.json")).ok();
        let rp_manifest_raw = fs::read_to_string(root.join("RP/manifest.json")).ok();

        let disk_mcfunctions = load_disk_mcfunctions(root);
        let target_registry = TargetRegistry::from_parsed_assets(root, &bp_items, &bp_blocks);

        Self {
            workspace_root: root.to_path_buf(),
            canonical_root,
            config,
            bp_items,
            bp_blocks,
            bp_recipes,
            rp_attachables,
            rp_sound_definitions,
            rp_particles,
            rp_item_textures,
            rp_terrain_textures,
            has_rp,
            bp_manifest_raw,
            rp_manifest_raw,
            disk_mcfunctions,
            target_registry,
        }
    }

    pub fn lookup_bp_target(&self, rel_path: &str) -> Option<&Value> {
        let norm = rel_path.trim().replace('\\', "/");
        self.bp_items.get(&norm).or_else(|| self.bp_blocks.get(&norm))
    }

    pub fn is_valid_custom_sound(&self, sound_id: &str) -> Option<bool> {
        let defs = self.rp_sound_definitions.as_ref()?;
        Some(defs.contains(&sound_id.trim().to_ascii_lowercase()))
    }
}

fn load_json_dir_map(root: &Path, sub_dir: &str) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    let dir = root.join(sub_dir);
    if !dir.is_dir() {
        return map;
    }
    let mut files = Vec::new();
    collect_json_files(&dir, &mut files);
    files.sort();
    for file_path in files {
        let Ok(raw) = fs::read_to_string(&file_path) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let rel = file_path
            .strip_prefix(root)
            .unwrap_or(&file_path)
            .display()
            .to_string()
            .replace('\\', "/");
        map.insert(rel, val);
    }
    map
}

fn load_sound_definitions(path: &Path) -> Option<HashSet<String>> {
    if !path.is_file() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    let val = serde_json::from_str::<Value>(&raw).ok()?;
    let mut set = HashSet::new();
    // Bedrock sound_definitions.json supports both { "sound_definitions": { ... } } and top-level keys
    let obj = val
        .get("sound_definitions")
        .and_then(|v| v.as_object())
        .or_else(|| val.as_object())?;
    for (k, _) in obj {
        if k != "format_version" {
            set.insert(k.to_ascii_lowercase());
        }
    }
    Some(set)
}

fn load_rp_particles(root: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let dir = root.join("RP/particles");
    if !dir.is_dir() {
        return set;
    }
    let mut files = Vec::new();
    collect_json_files(&dir, &mut files);
    for file_path in files {
        let Ok(raw) = fs::read_to_string(&file_path) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(id) = val
            .get("particle_effect")
            .and_then(|p| p.get("description"))
            .and_then(|d| d.get("identifier"))
            .and_then(|s| s.as_str())
        {
            set.insert(id.to_ascii_lowercase());
        }
    }
    set
}

fn load_disk_mcfunctions(root: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let fn_dir = root.join("BP/functions");
    if !fn_dir.is_dir() {
        return set;
    }
    collect_mcfunction_files(&fn_dir, &fn_dir, &mut set);
    set
}

fn collect_mcfunction_files(base: &Path, dir: &Path, out: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_mcfunction_files(base, &p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("mcfunction") {
            if let Ok(rel) = p.strip_prefix(base) {
                let norm = rel
                    .display()
                    .to_string()
                    .replace('\\', "/");
                let stem = norm.strip_suffix(".mcfunction").unwrap_or(&norm).to_string();
                out.insert(stem);
            }
        }
    }
}

/// Checks whether a texture alias violates format rules (paths, file extensions, or namespaces).
pub fn is_forbidden_texture_alias(alias: &str) -> bool {
    let trimmed = alias.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains(':') {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.ends_with(".png") || lower.ends_with(".tga") || lower.ends_with(".jpg")
}

/// Extracts texture path strings from a texture entry (string, array, or `{ variations: [{ path }] }`).
fn collect_texture_paths(val: &Value, out: &mut Vec<String>) {
    if let Some(s) = val.as_str() {
        out.push(s.to_string());
    } else if let Some(arr) = val.as_array() {
        for item in arr {
            collect_texture_paths(item, out);
        }
    } else if let Some(obj) = val.as_object() {
        if let Some(p) = obj.get("path").and_then(|v| v.as_str()) {
            out.push(p.to_string());
        }
        if let Some(vars) = obj.get("variations") {
            collect_texture_paths(vars, out);
        }
    }
}

/// Loads an alias -> list of image file paths dictionary from item_texture.json or terrain_texture.json.
fn load_texture_dict(json_path: &Path) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    if !json_path.is_file() {
        return map;
    }
    let Ok(content) = fs::read_to_string(json_path) else {
        return map;
    };
    let Ok(val) = serde_json::from_str::<Value>(&content) else {
        return map;
    };
    if let Some(texture_data) = val.get("texture_data").and_then(|td| td.as_object()) {
        for (alias, entry) in texture_data {
            let mut paths = Vec::new();
            if entry.is_string() {
                collect_texture_paths(entry, &mut paths);
            } else if let Some(obj) = entry.as_object() {
                if let Some(tex) = obj.get("textures") {
                    collect_texture_paths(tex, &mut paths);
                }
            }
            map.insert(alias.to_ascii_lowercase(), paths);
        }
    }
    map
}

/// Checks whether the physical texture image file exists inside the RP/ directory.
pub(crate) fn rp_texture_file_exists(root: &Path, tex_path: &str) -> bool {
    let rp = root.join("RP");
    let path = tex_path.trim().replace('\\', "/");
    let direct = rp.join(&path);
    if direct.is_file() {
        return true;
    }
    if rp.join(format!("{path}.png")).is_file() {
        return true;
    }
    if rp.join(format!("{path}.tga")).is_file() {
        return true;
    }
    if rp.join(format!("{path}.jpg")).is_file() {
        return true;
    }
    false
}

/// Recursively collects all .json files under a directory.
fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_json_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(p);
        }
    }
}

/// Extracts all texture aliases from the `minecraft:icon` component in item JSON.
fn extract_icon_aliases(val: &Value) -> Vec<String> {
    let mut aliases = Vec::new();
    let Some(item_obj) = val.get("minecraft:item").and_then(|v| v.as_object()) else {
        return aliases;
    };
    let Some(comps) = item_obj.get("components").and_then(|v| v.as_object()) else {
        return aliases;
    };
    let Some(icon) = comps.get("minecraft:icon") else {
        return aliases;
    };

    if let Some(s) = icon.as_str() {
        aliases.push(s.to_string());
    } else if let Some(obj) = icon.as_object() {
        if let Some(t) = obj.get("texture").and_then(|t| t.as_str()) {
            aliases.push(t.to_string());
        }
        if let Some(textures) = obj.get("textures") {
            if let Some(s) = textures.as_str() {
                aliases.push(s.to_string());
            } else if let Some(tex_obj) = textures.as_object() {
                for (_k, v) in tex_obj {
                    if let Some(s) = v.as_str() {
                        aliases.push(s.to_string());
                    }
                }
            } else if let Some(arr) = textures.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        aliases.push(s.to_string());
                    }
                }
            }
        }
    }
    aliases
}

/// Extracts all texture aliases from `minecraft:material_instances` in block JSON.
fn extract_material_instance_aliases(val: &Value) -> Vec<String> {
    let mut aliases = Vec::new();
    let Some(block_obj) = val.get("minecraft:block").and_then(|v| v.as_object()) else {
        return aliases;
    };
    let Some(comps) = block_obj.get("components").and_then(|v| v.as_object()) else {
        return aliases;
    };
    let Some(mat_inst) = comps
        .get("minecraft:material_instances")
        .and_then(|v| v.as_object())
    else {
        return aliases;
    };

    for (_key, entry) in mat_inst {
        if let Some(obj) = entry.as_object() {
            if let Some(tex) = obj.get("texture").and_then(|t| t.as_str()) {
                aliases.push(tex.to_string());
            }
        } else if let Some(s) = entry.as_str() {
            aliases.push(s.to_string());
        }
    }
    aliases
}

/// Validates all texture asset references in BP/items and BP/blocks against the Resource Pack
/// and vanilla catalog.
pub fn validate_workspace_assets(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let index = WorkspaceAssetIndex::build(root);
    Ok(validate_workspace_assets_with_index(&index, spec))
}

/// Validates texture asset references using a pre-built `WorkspaceAssetIndex`.
pub fn validate_workspace_assets_with_index(
    index: &WorkspaceAssetIndex,
    spec: &BedrockSpec,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let root = &index.workspace_root;
    let mut checked_paths: HashMap<String, bool> = HashMap::new();

    // 1. Check BP/items/*.json
    for (rel, val) in &index.bp_items {
        let span = Span::file_only(rel);
        let aliases = extract_icon_aliases(val);

        for alias in aliases {
            if let Some(paths) = index.rp_item_textures.get(&alias.to_ascii_lowercase()) {
                let exists = paths.iter().any(|p| {
                    *checked_paths
                        .entry(p.clone())
                        .or_insert_with(|| rp_texture_file_exists(root, p))
                });
                if !exists {
                    let path_info = if paths.is_empty() {
                        "no registered file paths".to_string()
                    } else {
                        paths.join(", ")
                    };
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Physical image file for item texture alias '{alias}' not found in RP/ (path: '{path_info}')."),
                        "Ensure the image file (.png or .tga) exists inside RP/ as defined in RP/textures/item_texture.json.",
                    ));
                }
            } else if is_forbidden_texture_alias(&alias) {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E017,
                    &span,
                    format!("Invalid item texture alias '{alias}': paths, file extensions (.png/.tga/.jpg), or namespaces are not allowed in 'minecraft:icon'."),
                    "Use a simple identifier alias (e.g. 'my_item' or 'snowball') without file extensions or folder paths.",
                ));
            } else if !spec.is_valid_vanilla_item_texture(&alias) {
                if !index.has_rp {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("BP references custom/unknown texture '{alias}', but no Resource Pack (RP/) was found."),
                        "Create RP/textures/item_texture.json + the corresponding .png file, or use a valid vanilla texture alias (e.g. 'snowball', 'diamond').",
                    ));
                } else {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Item texture alias '{alias}' not found in RP/textures/item_texture.json or the vanilla catalog."),
                        "Register the alias in RP/textures/item_texture.json along with its image file, or use a valid vanilla alias.",
                    ));
                }
            }
        }
    }

    // 2. Check BP/blocks/*.json
    for (rel, val) in &index.bp_blocks {
        let span = Span::file_only(rel);
        let aliases = extract_material_instance_aliases(val);

        for alias in aliases {
            if let Some(paths) = index.rp_terrain_textures.get(&alias.to_ascii_lowercase()) {
                let exists = paths.iter().any(|p| {
                    *checked_paths
                        .entry(p.clone())
                        .or_insert_with(|| rp_texture_file_exists(root, p))
                });
                if !exists {
                    let path_info = if paths.is_empty() {
                        "no registered file paths".to_string()
                    } else {
                        paths.join(", ")
                    };
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Physical image file for block texture alias '{alias}' not found in RP/ (path: '{path_info}')."),
                        "Ensure the image file (.png or .tga) exists inside RP/ as defined in RP/textures/terrain_texture.json.",
                    ));
                }
            } else if is_forbidden_texture_alias(&alias) {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E017,
                    &span,
                    format!("Invalid block texture alias '{alias}': paths, file extensions (.png/.tga/.jpg), or namespaces are not allowed in 'minecraft:material_instances'."),
                    "Use a simple identifier alias (e.g. 'my_block' or 'stone') without file extensions or folder paths.",
                ));
            } else if !spec.is_valid_vanilla_terrain_texture(&alias) {
                if !index.has_rp {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("BP references custom/unknown texture '{alias}', but no Resource Pack (RP/) was found."),
                        "Create RP/textures/terrain_texture.json + the corresponding .png file, or use a valid vanilla texture alias (e.g. 'stone', 'dirt').",
                    ));
                } else {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Block texture alias '{alias}' not found in RP/textures/terrain_texture.json or the vanilla catalog."),
                        "Register the alias in RP/textures/terrain_texture.json along with its image file, or use a valid vanilla alias.",
                    ));
                }
            }
        }
    }

    diags
}

/// Runs Bedrock 26.60 Asset Schema Gates during `pstdr-c audit`:
/// - Rejects numeric-only item ID suffixes (`bl:1`) in `BP/items/*.json`.
/// - Validates `minecraft:enchantable.slot` against Bedrock's exact enum (`armor_torso`, not `armor_chest`).
/// - Rejects missing `unlock` field in `BP/recipes/*.json` with `format_version >= 1.20.10`.
/// - Rejects non-existent armor render controllers (`controller.render.armor.*`) in `RP/attachables/*.json`.
pub fn validate_bedrock_schema_gates(index: &WorkspaceAssetIndex) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // 1 & 2: BP/items/*.json checks
    for (rel, val) in &index.bp_items {
        let span = Span::file_only(rel);
        let Some(item_obj) = val.get("minecraft:item").and_then(|v| v.as_object()) else {
            continue;
        };

        if let Some(id) = item_obj
            .get("description")
            .and_then(|d| d.get("identifier"))
            .and_then(|s| s.as_str())
        {
            let suffix = id.split_once(':').map(|(_, s)| s).unwrap_or(id);
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E017,
                    &span,
                    format!("Audit Gate failed: item identifier '{id}' in '{rel}' has a numeric-only suffix ('{suffix}'), which Bedrock rejects."),
                    "Start the item identifier suffix with an ASCII letter or descriptive slug (e.g. 'bl:item_1' instead of 'bl:1').",
                ));
            }
        }

        if let Some(ench_slot) = item_obj
            .get("components")
            .and_then(|c| c.get("minecraft:enchantable"))
            .and_then(|e| e.get("slot"))
            .and_then(|s| s.as_str())
        {
            if !VALID_ENCHANTABLE_SLOTS.contains(&ench_slot) {
                let hint = match ench_slot {
                    "armor_chest" | "chestplate" | "armor_chestplate" | "torso" => {
                        "Use 'armor_torso' instead (Bedrock uses 'armor_head', 'armor_torso', 'armor_legs', 'armor_feet')."
                    }
                    "armor_helmet" | "helmet" | "head" => {
                        "Use 'armor_head' instead (Bedrock uses 'armor_head', 'armor_torso', 'armor_legs', 'armor_feet')."
                    }
                    "armor_leggings" | "leggings" | "legs" => {
                        "Use 'armor_legs' instead (Bedrock uses 'armor_head', 'armor_torso', 'armor_legs', 'armor_feet')."
                    }
                    "armor_boots" | "boots" | "feet" => {
                        "Use 'armor_feet' instead (Bedrock uses 'armor_head', 'armor_torso', 'armor_legs', 'armor_feet')."
                    }
                    _ => "Use a valid Bedrock enchantable slot enum (e.g. 'sword', 'pickaxe', 'armor_head', 'armor_torso', 'armor_legs', 'armor_feet', 'all').",
                };
                diags.push(Diagnostic::error(
                    DiagnosticCode::E017,
                    &span,
                    format!("Audit Gate failed: invalid 'minecraft:enchantable.slot' value '{ench_slot}' in '{rel}'."),
                    hint,
                ));
            }
        }
    }

    // 3: BP/recipes/*.json missing `unlock` when format_version >= 1.20.10
    for (rel, val) in &index.bp_recipes {
        let span = Span::file_only(rel);
        let fv = val
            .get("format_version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.12.0");
        if parse_version_triplet(fv).is_some_and(|v| v >= [1, 20, 10]) {
            for recipe_key in [
                "minecraft:recipe_shaped",
                "minecraft:recipe_shapeless",
                "minecraft:recipe_furnace",
                "minecraft:recipe_smithing_transform",
                "minecraft:recipe_smithing_trim",
            ] {
                if let Some(body) = val.get(recipe_key).and_then(|v| v.as_object()) {
                    let has_unlock = match body.get("unlock") {
                        Some(Value::Array(arr)) => !arr.is_empty(),
                        Some(Value::Object(obj)) => !obj.is_empty(),
                        _ => false,
                    };
                    if !has_unlock {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E017,
                            &span,
                            format!("Audit Gate failed: '{rel}' ({recipe_key}, format_version '{fv}') is missing required 'unlock' field for format_version >= 1.20.10."),
                            "Add \"unlock\": [{ \"item\": \"minecraft:stick\" }] or \"unlock\": { \"context\": \"AlwaysUnlocked\" } inside the recipe object.",
                        ));
                    }
                }
            }
        }
    }

    // 4: RP/attachables/*.json invalid armor render controllers
    for (rel, val) in &index.rp_attachables {
        let span = Span::file_only(rel);
        let Some(rc_arr) = val
            .get("minecraft:attachable")
            .and_then(|a| a.get("description"))
            .and_then(|d| d.get("render_controllers"))
            .and_then(|r| r.as_array())
        else {
            continue;
        };
        for entry in rc_arr {
            let ctrl_name = if let Some(s) = entry.as_str() {
                Some(s)
            } else if let Some(obj) = entry.as_object() {
                obj.keys().next().map(|k| k.as_str())
            } else {
                None
            };
            if let Some(ctrl) = ctrl_name {
                if ctrl.starts_with("controller.render.armor.") {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Audit Gate failed: non-existent armor render controller '{ctrl}' in '{rel}'."),
                        "Replace it with 'controller.render.armor' (Bedrock has no slot-specific '.helmet'/'.chestplate'/'.leggings'/'.boots' render controllers).",
                    ));
                }
            }
        }
    }

    diags
}

fn parse_version_triplet(raw: &str) -> Option<[u64; 3]> {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let p0 = parts[0].parse::<u64>().ok()?;
    let p1 = parts[1].parse::<u64>().ok()?;
    let p2 = parts.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    if p0 >= 26 {
        Some([1, p0, p1])
    } else {
        Some([p0, p1, p2])
    }
}
