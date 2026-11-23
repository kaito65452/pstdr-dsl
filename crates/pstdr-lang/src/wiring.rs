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

/// Meresolusi `min_engine_version` berformat `[1, x, x]` dari string versi target.
///
/// Aturan resmi Bedrock:
/// - Format lama `"1.x.y"` atau `"1.x"` → `[1, x, y]` (mis. `"1.21.50"` → `[1, 21, 50]`)
/// - Format baru `"26.x"` atau `"26.x.y"` (major >= 26, bukan diawali 1) → prepend `1`:
///   `"26.60"` → `[1, 26, 60]`, `"26.60.0"` → `[1, 26, 60]`
///
/// Hasil **selalu** memiliki elemen pertama `1` sesuai spec manifest Bedrock resmi.
pub fn resolve_min_engine_version(target: &str) -> Result<[u64; 3], String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("String versi tidak boleh kosong".to_string());
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!(
            "Format versi tidak valid '{trimmed}': harus 2 atau 3 komponen dipisah '.'"
        ));
    }

    let parse_part = |s: &str| -> Result<u64, String> {
        s.parse::<u64>()
            .map_err(|_| format!("Komponen versi '{s}' bukan angka bulat non-negatif"))
    };

    let first = parse_part(parts[0])?;

    if first == 1 {
        // Format lama: "1.x" atau "1.x.y"
        if parts.len() < 2 {
            return Err(format!(
                "Format versi tidak valid '{trimmed}': minimal 2 komponen untuk format '1.x'"
            ));
        }
        let minor = parse_part(parts[1])?;
        let patch = if parts.len() >= 3 { parse_part(parts[2])? } else { 0 };
        Ok([1, minor, patch])
    } else if first >= 26 {
        // Format baru: "26.x" atau "26.x.y" – prepend 1
        if parts.len() < 2 {
            return Err(format!(
                "Format versi tidak valid '{trimmed}': minimal 2 komponen untuk format '{first}.x'"
            ));
        }
        let minor = parse_part(parts[1])?;
        // sub-patch ketiga pada format baru diabaikan (Bedrock hanya pakai dua segmen setelah prepend)
        Ok([1, first, minor])
    } else {
        Err(format!(
            "Format versi tidak dikenal '{trimmed}': diharapkan format '1.x.y' (lama) atau '26.x' (baru, >= 26)"
        ))
    }
}

/// Alias backward-compat yang dipakai oleh kode lama.
/// Gunakan `resolve_min_engine_version` untuk logika baru.
#[deprecated(since = "0.1.0", note = "Gunakan `resolve_min_engine_version` sebagai gantinya")]
pub fn parse_semver_triplet(ver: &str) -> [u64; 3] {
    resolve_min_engine_version(ver).unwrap_or([1, 26, 60])
}

/// Menyuntikkan `minecraft:custom_components` pada file BP/items/*.json dan BP/blocks/*.json
/// yang dideklarasikan melalui `bind_target(...)` secara idempoten.
pub fn wire_bind_targets(
    workspace_root: &Path,
    bind_targets: &[BindTargetStmt],
    script_stem: &str,
) -> Result<Vec<(String, ComponentKind)>, String> {
    let comp_id = component_id_for_script(script_stem);
    let mut registered = Vec::new();

    for bt in bind_targets {
        let target_path = workspace_root.join(&bt.path);
        if !target_path.is_file() {
            continue;
        }

        let raw = fs::read_to_string(&target_path)
            .map_err(|e| format!("Gagal membaca target '{}': {e}", target_path.display()))?;
        let Ok(mut val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };

        let kind_key = if val.get("minecraft:item").is_some() {
            Some(("minecraft:item", ComponentKind::Item))
        } else if val.get("minecraft:block").is_some() {
            Some(("minecraft:block", ComponentKind::Block))
        } else {
            None
        };

        if let Some((root_key, kind)) = kind_key {
            let mut modified = false;
            if let Some(root_obj) = val.get_mut(root_key).and_then(|v| v.as_object_mut()) {
                let comps = root_obj
                    .entry("components".to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(comps_obj) = comps.as_object_mut() {
                    // `minecraft:custom_components` tetap array string sesuai skema resmi
                    // pada format_version >= 1.21.50 (termasuk 1.21.90 untuk Bedrock 26.60).
                    let cc = comps_obj
                        .entry("minecraft:custom_components".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Some(arr) = cc.as_array_mut() {
                        let exists = arr.iter().any(|v| v.as_str() == Some(comp_id.as_str()));
                        if !exists {
                            arr.push(Value::String(comp_id.clone()));
                            modified = true;
                        }
                    }
                }
            }

            if modified {
                let pretty = serde_json::to_string_pretty(&val)
                    .map_err(|e| format!("Gagal serialize JSON '{}': {e}", bt.path))?;
                fs::write(&target_path, format!("{pretty}\n"))
                    .map_err(|e| format!("Gagal menulis '{}': {e}", target_path.display()))?;
            }

            if !registered.iter().any(|(id, k)| id == &comp_id && k == &kind) {
                registered.push((comp_id.clone(), kind));
            }
        }
    }

    Ok(registered)
}

/// Memastikan `BP/manifest.json` memiliki modul `script` (entry `scripts/main.js`),
/// dependency `@minecraft/server`, dan `min_engine_version` berformat `[1, x, x]`.
pub fn update_manifest(
    workspace_root: &Path,
    target_version: &str,
    script_api_version: &str,
) -> Result<(), String> {
    let bp_dir = workspace_root.join("BP");
    fs::create_dir_all(&bp_dir)
        .map_err(|e| format!("Gagal membuat folder '{}': {e}", bp_dir.display()))?;
    let manifest_path = bp_dir.join("manifest.json");
    let min_engine = resolve_min_engine_version(target_version)
        .unwrap_or([1, 26, 60]);

    let mut manifest: Value = if manifest_path.is_file() {
        let raw = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Gagal membaca BP/manifest.json: {e}"))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("BP/manifest.json bukan JSON valid: {e}"))?
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
            "modules": [],
            "dependencies": []
        })
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
                .or_insert_with(|| json!([1, 0, 0]));
            let needs_min = !matches!(
                h.get("min_engine_version").and_then(|v| v.as_array()),
                Some(arr) if arr.len() == 3 && arr[0].as_u64() == Some(1)
            );
            if needs_min {
                h.insert("min_engine_version".to_string(), json!(min_engine));
            }
        }

        let modules = root_obj
            .entry("modules".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = modules.as_array_mut() {
            let has_script = arr
                .iter()
                .any(|m| m.get("type").and_then(|t| t.as_str()) == Some("script"));
            if !has_script {
                arr.push(json!({
                    "type": "script",
                    "language": "javascript",
                    "uuid": generate_uuid_v4(),
                    "entry": "scripts/main.js",
                    "version": [1, 0, 0]
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
        }
    }

    let pretty = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Gagal serialize BP/manifest.json: {e}"))?;
    fs::write(&manifest_path, format!("{pretty}\n"))
        .map_err(|e| format!("Gagal menulis BP/manifest.json: {e}"))
}

/// Memperbarui entry point `BP/scripts/main.js` agar mengimpor seluruh modul hasil generate.
pub fn update_main_js(workspace_root: &Path, generated_modules: &[String]) -> Result<(), String> {
    let scripts_dir = workspace_root.join("BP/scripts");
    fs::create_dir_all(&scripts_dir)
        .map_err(|e| format!("Gagal membuat folder '{}': {e}", scripts_dir.display()))?;

    let mut sorted = generated_modules.to_vec();
    sorted.sort();
    sorted.dedup();

    let mut out = String::from("// Auto-generated entry point by pstdr-c\n");
    for m in sorted {
        out.push_str(&format!("import \"./generated/{m}.js\";\n"));
    }

    let main_path = scripts_dir.join("main.js");
    fs::write(&main_path, out)
        .map_err(|e| format!("Gagal menulis '{}': {e}", main_path.display()))
}

