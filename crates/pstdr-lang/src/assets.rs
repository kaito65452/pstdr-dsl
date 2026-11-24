use crate::diagnostics::{Diagnostic, DiagnosticCode, Span};
use crate::spec::BedrockSpec;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
            if let Some(s) = entry.as_str() {
                paths.push(s.to_string());
            } else if let Some(obj) = entry.as_object() {
                if let Some(tex) = obj.get("textures") {
                    if let Some(s) = tex.as_str() {
                        paths.push(s.to_string());
                    } else if let Some(arr) = tex.as_array() {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                paths.push(s.to_string());
                            }
                        }
                    }
                }
            }
            map.insert(alias.to_ascii_lowercase(), paths);
        }
    }
    map
}

/// Checks whether the physical texture image file exists inside the RP/ directory.
fn rp_texture_file_exists(root: &Path, tex_path: &str) -> bool {
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
    let mut diags = Vec::new();

    let rp_item_textures = load_texture_dict(&root.join("RP/textures/item_texture.json"));
    let rp_terrain_textures = load_texture_dict(&root.join("RP/textures/terrain_texture.json"));
    let has_rp = root.join("RP/manifest.json").is_file() || root.join("RP/textures").is_dir();

    // 1. Scan BP/items/*.json
    let items_dir = root.join("BP/items");
    if items_dir.is_dir() {
        let mut item_files = Vec::new();
        collect_json_files(&items_dir, &mut item_files);
        item_files.sort();

        for file_path in item_files {
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
                .to_string();
            let span = Span::file_only(rel);
            let aliases = extract_icon_aliases(&val);

            for alias in aliases {
                if is_forbidden_texture_alias(&alias) {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Invalid item texture alias '{alias}': paths, file extensions (.png/.tga/.jpg), or namespaces are not allowed in 'minecraft:icon'."),
                        "Use a simple identifier alias (e.g. 'my_item' or 'snowball') without file extensions or folder paths.",
                    ));
                } else if let Some(paths) = rp_item_textures.get(&alias.to_ascii_lowercase()) {
                    let exists = paths.iter().any(|p| rp_texture_file_exists(root, p));
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
                } else if !spec.is_valid_vanilla_item_texture(&alias) {
                    if !has_rp {
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
    }

    // 2. Scan BP/blocks/*.json
    let blocks_dir = root.join("BP/blocks");
    if blocks_dir.is_dir() {
        let mut block_files = Vec::new();
        collect_json_files(&blocks_dir, &mut block_files);
        block_files.sort();

        for file_path in block_files {
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
                .to_string();
            let span = Span::file_only(rel);
            let aliases = extract_material_instance_aliases(&val);

            for alias in aliases {
                if is_forbidden_texture_alias(&alias) {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E017,
                        &span,
                        format!("Invalid block texture alias '{alias}': paths, file extensions (.png/.tga/.jpg), or namespaces are not allowed in 'minecraft:material_instances'."),
                        "Use a simple identifier alias (e.g. 'my_block' or 'stone') without file extensions or folder paths.",
                    ));
                } else if let Some(paths) = rp_terrain_textures.get(&alias.to_ascii_lowercase()) {
                    let exists = paths.iter().any(|p| rp_texture_file_exists(root, p));
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
                } else if !spec.is_valid_vanilla_terrain_texture(&alias) {
                    if !has_rp {
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
    }

    Ok(diags)
}
