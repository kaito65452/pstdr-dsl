use crate::ast::BindTargetStmt;
use crate::codegen::ComponentKind;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UUID_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn generate_uuid_v4() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x1234_5678_9abc_def0);
    let pid = std::process::id() as u64;
    let seq = UUID_SEQ.fetch_add(1, Ordering::Relaxed);

    let mut s1 = splitmix64(nanos ^ (pid << 17) ^ seq);
    let mut s2 = splitmix64(s1 ^ 0x9e37_79b9_7f4a_7c15);
    s1 = (s1 & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    s2 = (s2 & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;

    let bytes1 = s1.to_be_bytes();
    let bytes2 = s2.to_be_bytes();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes1[0], bytes1[1], bytes1[2], bytes1[3],
        bytes1[4], bytes1[5],
        bytes1[6], bytes1[7],
        bytes2[0], bytes2[1],
        bytes2[2], bytes2[3], bytes2[4], bytes2[5], bytes2[6], bytes2[7]
    )
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

pub fn component_id_for_script(script_stem: &str) -> String {
    let clean: String = script_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("pstdr:{clean}")
}

/// Resolves a `[1, x, x]` `min_engine_version` triplet from a target version string.
///
/// Official Bedrock rules:
/// - Legacy format `"1.x.y"` or `"1.x"` → `[1, x, y]` (e.g. `"1.21.50"` → `[1, 21, 50]`)
/// - Modern format `"26.x"` or `"26.x.y"` (major >= 26, not starting with 1) → prepend `1`:
///   `"26.60"` → `[1, 26, 60]`, `"26.60.0"` → `[1, 26, 60]`
///
/// The result **always** has `1` as its first element per the official Bedrock manifest spec.
pub fn resolve_min_engine_version(target: &str) -> Result<[u64; 3], String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("Version string cannot be empty".to_string());
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!(
            "Invalid version format '{trimmed}': must have 2 or 3 dot-separated components"
        ));
    }

    let parse_part = |s: &str| -> Result<u64, String> {
        s.parse::<u64>()
            .map_err(|_| format!("Version component '{s}' is not a non-negative integer"))
    };

    let first = parse_part(parts[0])?;

    if first == 1 {
        // Legacy format: "1.x" or "1.x.y"
        if parts.len() < 2 {
            return Err(format!(
                "Invalid version format '{trimmed}': at least 2 components required for '1.x' format"
            ));
        }
        let minor = parse_part(parts[1])?;
        let patch = if parts.len() >= 3 { parse_part(parts[2])? } else { 0 };
        Ok([1, minor, patch])
    } else if first >= 26 {
        // Modern format: "26.x" or "26.x.y" – prepend 1
        if parts.len() < 2 {
            return Err(format!(
                "Invalid version format '{trimmed}': at least 2 components required for '{first}.x' format"
            ));
        }
        let minor = parse_part(parts[1])?;
        // Third sub-patch segment in modern format is ignored (Bedrock uses two segments after prepending 1)
        Ok([1, first, minor])
    } else {
        Err(format!(
            "Unrecognized version format '{trimmed}': expected '1.x.y' (legacy) or '26.x' (modern, >= 26)"
        ))
    }
}

/// Backward-compatible alias used by legacy code.
/// Use `resolve_min_engine_version` for new logic.
#[deprecated(since = "0.1.0", note = "Use `resolve_min_engine_version` instead")]
pub fn parse_semver_triplet(ver: &str) -> [u64; 3] {
    resolve_min_engine_version(ver).unwrap_or([1, 26, 60])
}

/// Determines whether a BP file's `format_version` uses the Custom Components V2 schema (>= 1.21.90).
pub fn uses_custom_components_v2(format_version: &str) -> bool {
    let triplet = resolve_min_engine_version(format_version).unwrap_or([1, 0, 0]);
    triplet >= [1, 21, 90]
}

/// Validates the string `min_engine_version` format for manifest `format_version: 3` (SemVer `"1.x.x"`).
pub fn is_valid_v3_min_engine(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let Ok(p0) = parts[0].parse::<u64>() else { return false; };
    let Ok(_p1) = parts[1].parse::<u64>() else { return false; };
    let Ok(_p2) = parts[2].parse::<u64>() else { return false; };
    p0 == 1
}

/// Injects `minecraft:custom_components` (V1) or direct component keys (V2)
/// into a single BP item or block JSON file.
pub fn wire_target_json(
    target_path: &Path,
    comp_id: &str,
) -> Result<Option<ComponentKind>, String> {
    if !target_path.is_file() {
        return Ok(None);
    }

    let raw = fs::read_to_string(target_path)
        .map_err(|e| format!("Failed to read target '{}': {e}", target_path.display()))?;
    let Ok(mut val) = serde_json::from_str::<Value>(&raw) else {
        return Ok(None);
    };

    let fv = val
        .get("format_version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.21.50");
    let is_v2 = uses_custom_components_v2(fv);

    let kind_key = if val.get("minecraft:item").is_some() {
        Some(("minecraft:item", ComponentKind::Item))
    } else if val.get("minecraft:block").is_some() {
        Some(("minecraft:block", ComponentKind::Block))
    } else {
        None
    };

    let Some((root_key, kind)) = kind_key else {
        return Ok(None);
    };

    let mut modified = false;
    if let Some(root_obj) = val.get_mut(root_key).and_then(|v| v.as_object_mut()) {
        let comps = root_obj
            .entry("components".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(comps_obj) = comps.as_object_mut() {
            if is_v2 {
                // Custom Components V2 (format_version >= 1.21.90):
                // DO NOT write a "minecraft:custom_components" array.
                // Inject directly as an object key inside "components": "pstdr:<slug>": {}.
                if !comps_obj.contains_key(comp_id) {
                    comps_obj.insert(comp_id.to_string(), json!({}));
                    modified = true;
                }
            } else {
                // Custom Components V1 (format_version < 1.21.90):
                // Inject into the "minecraft:custom_components": ["pstdr:<slug>"] array.
                let cc = comps_obj
                    .entry("minecraft:custom_components".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(arr) = cc.as_array_mut() {
                    let exists = arr.iter().any(|v| v.as_str() == Some(comp_id));
                    if !exists {
                        arr.push(Value::String(comp_id.to_string()));
                        modified = true;
                    }
                }
            }
        }
    }

    if modified {
        let pretty = serde_json::to_string_pretty(&val)
            .map_err(|e| format!("Failed to serialize JSON '{}': {e}", target_path.display()))?;
        fs::write(target_path, format!("{pretty}\n"))
            .map_err(|e| format!("Failed to write '{}': {e}", target_path.display()))?;
    }

    Ok(Some(kind))
}

/// Injects `minecraft:custom_components` (V1) or direct component keys (V2)
/// into `BP/items/*.json` and `BP/blocks/*.json` files declared via `bind_target(...)`.
pub fn wire_bind_targets(
    workspace_root: &Path,
    bind_targets: &[BindTargetStmt],
    script_stem: &str,
) -> Result<Vec<(String, ComponentKind)>, String> {
    let comp_id = component_id_for_script(script_stem);
    let mut registered = Vec::new();

    for bt in bind_targets {
        let target_path = workspace_root.join(&bt.path);
        if let Some(kind) = wire_target_json(&target_path, &comp_id)? {
            if !registered.iter().any(|(id, k)| id == &comp_id && k == &kind) {
                registered.push((comp_id.clone(), kind));
            }
        }
    }

    Ok(registered)
}

/// Ensures `BP/manifest.json` has `data` and `script` modules (entry `scripts/main.js`),
/// the `@minecraft/server` dependency, and `min_engine_version` matching `format_version` (v2: array, v3: SemVer string).
pub fn update_manifest(
    workspace_root: &Path,
    target_version: &str,
    script_api_version: &str,
) -> Result<(), String> {
    update_manifest_with_ui(workspace_root, target_version, script_api_version, false)
}

/// Same as `update_manifest`, with optional `@minecraft/server-ui` (`"2.0.0"`) dependency injection
/// when at least one `ui` form block is used in the workspace.
pub fn update_manifest_with_ui(
    workspace_root: &Path,
    target_version: &str,
    script_api_version: &str,
    needs_server_ui: bool,
) -> Result<(), String> {
    let bp_dir = workspace_root.join("BP");
    fs::create_dir_all(&bp_dir)
        .map_err(|e| format!("Failed to create directory '{}': {e}", bp_dir.display()))?;
    let manifest_path = bp_dir.join("manifest.json");
    let min_engine = resolve_min_engine_version(target_version)?;

    let mut manifest: Value = if manifest_path.is_file() {
        let raw = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read BP/manifest.json: {e}"))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("BP/manifest.json is not valid JSON: {e}"))?
    } else {
        json!({
            "format_version": 2,
            "header": {
                "name": "PackStudio Add-On",
                "description": "Generated by pstdr-c",
                "uuid": generate_uuid_v4(),
                "version": [1, 0, 0],
                "min_engine_version": min_engine
            },
            "modules": [
                {
                    "type": "data",
                    "uuid": generate_uuid_v4(),
                    "version": [1, 0, 0]
                },
                {
                    "type": "script",
                    "language": "javascript",
                    "uuid": generate_uuid_v4(),
                    "entry": "scripts/main.js",
                    "version": [1, 0, 0]
                }
            ],
            "dependencies": []
        })
    };

    let fmt_ver = manifest
        .get("format_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(2);
    let mod_ver = if fmt_ver == 3 {
        json!("1.0.0")
    } else {
        json!([1, 0, 0])
    };

    if let Some(root_obj) = manifest.as_object_mut() {
        root_obj
            .entry("format_version".to_string())
            .or_insert_with(|| json!(2));

        let header = root_obj
            .entry("header".to_string())
            .or_insert_with(|| json!({}));
        if let Some(h) = header.as_object_mut() {
            h.entry("uuid".to_string())
                .or_insert_with(|| Value::String(generate_uuid_v4()));
            h.entry("version".to_string())
                .or_insert_with(|| mod_ver.clone());

            if fmt_ver == 3 {
                let is_valid = h
                    .get("min_engine_version")
                    .and_then(|v| v.as_str())
                    .map(is_valid_v3_min_engine)
                    .unwrap_or(false);
                if !is_valid {
                    let semver_str =
                        format!("{}.{}.{}", min_engine[0], min_engine[1], min_engine[2]);
                    h.insert("min_engine_version".to_string(), json!(semver_str));
                }
            } else {
                let needs_min = !matches!(
                    h.get("min_engine_version").and_then(|v| v.as_array()),
                    Some(arr) if arr.len() == 3 && arr[0].as_u64() == Some(1)
                );
                if needs_min {
                    h.insert("min_engine_version".to_string(), json!(min_engine));
                }
            }
        }

        let modules = root_obj
            .entry("modules".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = modules.as_array_mut() {
            let has_data = arr
                .iter()
                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("data"));
            if !has_data {
                arr.push(json!({
                    "type": "data",
                    "uuid": generate_uuid_v4(),
                    "version": mod_ver
                }));
            }

            let has_script = arr
                .iter()
                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("script"));
            if !has_script {
                arr.push(json!({
                    "type": "script",
                    "language": "javascript",
                    "uuid": generate_uuid_v4(),
                    "entry": "scripts/main.js",
                    "version": mod_ver
                }));
            }
        }

        let deps = root_obj
            .entry("dependencies".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = deps.as_array_mut() {
            let mut updated = false;
            for dep in arr.iter_mut() {
                if dep.get("module_name").and_then(|m| m.as_str()) == Some("@minecraft/server") {
                    if let Some(obj) = dep.as_object_mut() {
                        obj.insert(
                            "version".to_string(),
                            Value::String(script_api_version.to_string()),
                        );
                    }
                    updated = true;
                }
            }
            if !updated {
                arr.push(json!({
                    "module_name": "@minecraft/server",
                    "version": script_api_version
                }));
            }
            if needs_server_ui {
                let mut ui_updated = false;
                for dep in arr.iter_mut() {
                    if dep.get("module_name").and_then(|m| m.as_str())
                        == Some("@minecraft/server-ui")
                    {
                        if let Some(obj) = dep.as_object_mut() {
                            obj.insert("version".to_string(), Value::String("2.0.0".to_string()));
                        }
                        ui_updated = true;
                    }
                }
                if !ui_updated {
                    arr.push(json!({
                        "module_name": "@minecraft/server-ui",
                        "version": "2.0.0"
                    }));
                }
            }
        }
    }

    let pretty = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize BP/manifest.json: {e}"))?;
    fs::write(&manifest_path, format!("{pretty}\n"))
        .map_err(|e| format!("Failed to write BP/manifest.json: {e}"))
}

/// Updates the `BP/scripts/main.js` entry point to import all generated modules.
pub fn update_main_js(workspace_root: &Path, generated_modules: &[String]) -> Result<(), String> {
    let scripts_dir = workspace_root.join("BP/scripts");
    fs::create_dir_all(&scripts_dir)
        .map_err(|e| format!("Failed to create directory '{}': {e}", scripts_dir.display()))?;

    let mut sorted = generated_modules.to_vec();
    sorted.sort();
    sorted.dedup();
    // Ensure `_registry` is always imported first when present.
    if let Some(pos) = sorted.iter().position(|m| m == "_registry") {
        let reg = sorted.remove(pos);
        sorted.insert(0, reg);
    }

    let mut out = String::from("// Auto-generated entry point by pstdr-c\n");
    for m in sorted {
        out.push_str(&format!("import \"./generated/{m}.js\";\n"));
    }

    let main_path = scripts_dir.join("main.js");
    fs::write(&main_path, out)
        .map_err(|e| format!("Failed to write '{}': {e}", main_path.display()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub component_id: String,
    pub kind: ComponentKind,
    pub slug: String,
}

/// Generates the centralized `BP/scripts/generated/_registry.js` (or `.ts`) module
/// that registers all partitioned item and block custom components in a single startup callback.
pub fn emit_registry_module(
    entries: &[RegistryEntry],
    target_version: &str,
    mode: crate::codegen::EmitMode,
) -> String {
    emit_registry_module_with_commands(entries, &[], target_version, mode)
}

/// Same as `emit_registry_module`, additionally invoking custom command registrations from
/// `commands/<stem>.js` inside the same single `system.beforeEvents.startup` subscriber.
pub fn emit_registry_module_with_commands(
    entries: &[RegistryEntry],
    command_modules: &[String],
    target_version: &str,
    mode: crate::codegen::EmitMode,
) -> String {
    let use_v2_startup = uses_custom_components_v2(target_version);
    let init_ev_param = if mode == crate::codegen::EmitMode::TypeScript {
        "(initEv: any)"
    } else {
        "(initEv)"
    };
    let ext = if mode == crate::codegen::EmitMode::TypeScript {
        ""
    } else {
        ".js"
    };

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| {
        let ka = match a.kind {
            ComponentKind::Item => 0u8,
            ComponentKind::Block => 1u8,
        };
        let kb = match b.kind {
            ComponentKind::Item => 0u8,
            ComponentKind::Block => 1u8,
        };
        ka.cmp(&kb).then_with(|| a.slug.cmp(&b.slug))
    });
    sorted.dedup_by(|a, b| a.kind == b.kind && a.slug == b.slug);

    let mut sorted_cmds = command_modules.to_vec();
    sorted_cmds.sort();
    sorted_cmds.dedup();

    let mut out = String::from("// Auto-generated component registry by pstdr-c\n");
    let need_system = use_v2_startup || !sorted_cmds.is_empty();
    let need_world = !use_v2_startup && !sorted.is_empty();
    let mut mc_imports = Vec::new();
    if need_world {
        mc_imports.push("world");
    }
    if need_system {
        mc_imports.push("system");
    }
    if !mc_imports.is_empty() {
        out.push_str(&format!(
            "import {{ {} }} from \"@minecraft/server\";\n",
            mc_imports.join(", ")
        ));
    }

    for entry in &sorted {
        match entry.kind {
            ComponentKind::Item => {
                out.push_str(&format!(
                    "import {{ component as item_{} }} from \"./items/{}{ext}\";\n",
                    entry.slug, entry.slug
                ));
            }
            ComponentKind::Block => {
                out.push_str(&format!(
                    "import {{ component as block_{} }} from \"./blocks/{}{ext}\";\n",
                    entry.slug, entry.slug
                ));
            }
        }
    }
    for stem in &sorted_cmds {
        let safe = stem.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
        out.push_str(&format!(
            "import {{ registerCommands as cmd_{safe} }} from \"./commands/{stem}{ext}\";\n"
        ));
    }

    out.push('\n');
    if use_v2_startup {
        out.push_str(&format!(
            "system.beforeEvents.startup.subscribe({init_ev_param} => {{\n"
        ));
        for entry in &sorted {
            match entry.kind {
                ComponentKind::Item => {
                    out.push_str(&format!(
                        "    initEv.itemComponentRegistry.registerCustomComponent(\"{}\", item_{});\n",
                        entry.component_id, entry.slug
                    ));
                }
                ComponentKind::Block => {
                    out.push_str(&format!(
                        "    initEv.blockComponentRegistry.registerCustomComponent(\"{}\", block_{});\n",
                        entry.component_id, entry.slug
                    ));
                }
            }
        }
        for stem in &sorted_cmds {
            let safe = stem.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
            out.push_str(&format!("    cmd_{safe}(initEv);\n"));
        }
        out.push_str("});\n");
    } else {
        if !sorted.is_empty() {
            out.push_str(&format!(
                "world.beforeEvents.worldInitialize.subscribe({init_ev_param} => {{\n"
            ));
            for entry in &sorted {
                match entry.kind {
                    ComponentKind::Item => {
                        out.push_str(&format!(
                            "    initEv.itemComponentRegistry.registerCustomComponent(\"{}\", item_{});\n",
                            entry.component_id, entry.slug
                        ));
                    }
                    ComponentKind::Block => {
                        out.push_str(&format!(
                            "    initEv.blockComponentRegistry.registerCustomComponent(\"{}\", block_{});\n",
                            entry.component_id, entry.slug
                        ));
                    }
                }
            }
            out.push_str("});\n");
        }
        if !sorted_cmds.is_empty() {
            if !sorted.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "system.beforeEvents.startup.subscribe({init_ev_param} => {{\n"
            ));
            for stem in &sorted_cmds {
                let safe = stem.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
                out.push_str(&format!("    cmd_{safe}(initEv);\n"));
            }
            out.push_str("});\n");
        }
    }

    out
}

