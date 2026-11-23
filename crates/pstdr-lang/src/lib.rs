pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod formatter;
pub mod lexer;
pub mod parser;
pub mod spec;
pub mod validator;
pub mod wiring;
pub mod workspace;

pub use codegen::{emit_code, CodegenOptions, ComponentKind, EmitMode};
pub use diagnostics::{
    diagnostics_to_json, explain_code, format_diagnostics_compact, format_diagnostics_pretty,
    has_errors, Diagnostic, DiagnosticCode, Severity, Span,
};
pub use formatter::format_source;
pub use parser::parse_source;
pub use spec::BedrockSpec;
pub use validator::{validate_program, ValidatedProgramInfo, ValidationContext};
pub use workspace::{
    audit_workspace, check_path, compile_source, convert_single_file, convert_workspace,
    doctor_report, find_workspace_root, fmt_path, init_workspace, new_script, CompileResult,
    WorkspaceConfig,
};
pub use wiring::{is_valid_v3_min_engine, resolve_min_engine_version, uses_custom_components_v2};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pstdr_{prefix}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fase_a_forbidden_keywords_emit_e001() {
        let cases = [
            "class Foo {}",
            "let x = 1",
            "const x = nil",
            "on join of player { if player.hp ~= 0 { send \"a\" to player } }",
            "on join of player { if true then send \"a\" to player end }",
        ];
        for src in cases {
            let (_prog, diags) = parse_source(src, "test.pstdr");
            assert!(
                diags.iter().any(|d| d.code == "PSTDR-E001"),
                "Expected PSTDR-E001 for input `{src}`, got: {diags:?}"
            );
        }
    }

    #[test]
    fn fase_a_const_mutation_e002_and_mut_allowed() {
        let spec = BedrockSpec::bundled();
        let bad = r#"
on join of player {
    const a = 10
    a = 20
}
"#;
        let res = compile_source(bad, "test.pstdr", None, &spec, true, true, vec![]);
        assert!(res.diagnostics.iter().any(|d| d.code == "PSTDR-E002"));

        let good = r#"
on join of player {
    mut b = 10
    b += 5
}
"#;
        let res_ok = compile_source(good, "test.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res_ok.diagnostics));
        let js = res_ok.js.unwrap();
        assert!(js.contains("let b = 10;"));
        assert!(js.contains("b += 5;"));
    }

    #[test]
    fn fase_a_multi_error_e010_and_e005_and_e004() {
        let spec = BedrockSpec::bundled();
        let src = r#"
every 0s {
    send "tes" to player
    cancel event
}
"#;
        let res = compile_source(src, "multi.pstdr", None, &spec, true, true, vec![]);
        let codes: Vec<&str> = res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"PSTDR-E010"), "missing E010: {codes:?}");
        assert!(codes.contains(&"PSTDR-E005"), "missing E005: {codes:?}");
        assert!(codes.contains(&"PSTDR-E004"), "missing E004: {codes:?}");
    }

    #[test]
    fn fase_a_effect_level_zero_e011_and_invalid_vanilla_e013() {
        let spec = BedrockSpec::bundled();
        let src = r#"
on join of player {
    apply slownes 0 to player for 3s
}
"#;
        let res = compile_source(src, "eff.pstdr", None, &spec, true, true, vec![]);
        let codes: Vec<&str> = res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"PSTDR-E011"), "missing E011: {codes:?}");
        assert!(codes.contains(&"PSTDR-E013"), "missing E013: {codes:?}");
    }

    #[test]
    fn fase_a_unsafe_isolation_and_unclosed_brace_e012() {
        let spec = BedrockSpec::bundled();
        let src = r#"
unsafe spawn_raw(id_target) {
    const x = { a: [1, 2] };
    return id_target ?? x;
}
"#;
        let res = compile_source(src, "raw.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("// [unsafe:spawn_raw @ raw.pstdr:2]"));
        assert!(js.contains("const x = { a: [1, 2] };"));

        let unclosed = "func broken() {\n    mut a = 1\n";
        let (_p, diags) = parse_source(unclosed, "broken.pstdr");
        assert!(diags.iter().any(|d| d.code == "PSTDR-E012"));
    }

    #[test]
    fn fase_b_codegen_before_after_events_and_amplifier_and_snake_case() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "1.21.50", api = "1.16.0")

const BLADE_ID = "my_pack:frost_blade"
state mana[player] = 100

on break of "minecraft:stone" by player {
    send "Batu pecah" to player
}

on break of "minecraft:diamond_ore" by player {
    cancel event
    player.actionbar("§cTerproteksi!")
}

on hit target with BLADE_ID by player {
    const label = target.name_tag != "" ? target.name_tag : target.type_id
    apply strength 2 to player for 5s
    mana[player] -= 10
    send "Hit {label}" to player
}
"#;
        let res = compile_source(src, "frost.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("world.afterEvents.playerBreakBlock.subscribe"));
        assert!(js.contains("world.beforeEvents.playerBreakBlock.subscribe"));
        assert!(js.contains("ev.cancel = true;"));
        assert!(js.contains("system.run(() => {\n        player.onScreenDisplay.setActionBar"));
        assert!(js.contains("player.addEffect(\"strength\", 100, { amplifier: 1 });"));
        assert!(js.contains("target.nameTag"));
        assert!(js.contains("target.typeId"));
        assert!(js.contains("getDynamicProperty(\"pstdr:mana\")"));
        assert!(js.contains("setDynamicProperty(\"pstdr:mana\""));
    }

    #[test]
    fn fase_b_bind_target_const_propagation_and_wiring() {
        let dir = temp_dir("bind");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/frost_blade.json"),
            r#"{
  "format_version": "1.21.50",
  "minecraft:item": {
    "description": { "identifier": "my_pack:frost_blade" },
    "components": {}
  }
}"#,
        )
        .unwrap();

        fs::write(
            dir.join("src/frost_blade.pstdr"),
            r#"
@addon(target = "1.21.50", api = "1.16.0")
bind_target("BP/items/frost_blade.json")

const BLADE_ID = "my_pack:frost_blade"

on hit target with BLADE_ID by player {
    damage target by 5
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "diags: {diags:?}");

        let item_json = fs::read_to_string(dir.join("BP/items/frost_blade.json")).unwrap();
        assert!(item_json.contains("minecraft:custom_components"));
        assert!(item_json.contains("pstdr:frost_blade"));

        let main_js = fs::read_to_string(dir.join("BP/scripts/main.js")).unwrap();
        assert!(main_js.contains("import \"./generated/frost_blade.js\";"));

        let manifest = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        assert!(manifest.contains("@minecraft/server"));

        // Uji mismatch E008 lewat const yang berbeda
        let bad_script = r#"
bind_target("BP/items/frost_blade.json")
const WRONG_ID = "my_pack:other_blade"
on hit target with WRONG_ID by player {
    damage target by 2
}
"#;
        let bad_res = compile_source(bad_script, "bad.pstdr", Some(&dir), &spec, true, true, vec![]);
        assert!(bad_res.diagnostics.iter().any(|d| d.code == "PSTDR-E008"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_b_formatter_is_idempotent_and_preserves_unsafe() {
        let ugly = r#"
@addon(target="1.21.50",api="1.16.0")
const ID="my_pack:x";
unsafe raw_fn(a){
    const k = a ?? 1;
    return k;
}
on join of player{
mut n=1+2;
send "Hi {n}" to player;
}
"#;
        let first = format_source(ugly, "ugly.pstdr").unwrap();
        let second = format_source(&first, "ugly.pstdr").unwrap();
        assert_eq!(first, second, "Formatter must be idempotent");
        assert!(first.contains("const k = a ?? 1;"));
        assert!(!first.contains("mut n = 1 + 2;"));
        assert!(first.contains("mut n = 1 + 2"));
    }

    // ─── FASE 2: Tes baru untuk resolve_min_engine_version ───────────────────

    #[test]
    fn fase_2_resolve_min_engine_version_format_baru() {
        // Format baru: "26.x" → [1, 26, x]
        assert_eq!(resolve_min_engine_version("26.60"), Ok([1, 26, 60]));
        assert_eq!(resolve_min_engine_version("26.0"),  Ok([1, 26, 0]));
        // Format baru dengan 3 komponen (sub-patch ketiga diabaikan → major=26, minor=60)
        assert_eq!(resolve_min_engine_version("26.60.0"), Ok([1, 26, 60]));
        // Versi lebih besar
        assert_eq!(resolve_min_engine_version("30.10"), Ok([1, 30, 10]));
    }

    #[test]
    fn fase_2_resolve_min_engine_version_format_lama() {
        // Format lama: "1.x.y" → [1, x, y]
        assert_eq!(resolve_min_engine_version("1.21.50"),  Ok([1, 21, 50]));
        assert_eq!(resolve_min_engine_version("1.21.130"), Ok([1, 21, 130]));
        assert_eq!(resolve_min_engine_version("1.20.80"),  Ok([1, 20, 80]));
        // Format "1.x" tanpa patch → patch default 0
        assert_eq!(resolve_min_engine_version("1.20"), Ok([1, 20, 0]));
    }

    #[test]
    fn fase_2_resolve_min_engine_version_selalu_diawali_1() {
        // Pastikan elemen pertama selalu 1 untuk semua kasus yang valid
        for target in &["26.60", "26.0", "1.21.50", "1.20.80", "1.21.130"] {
            let result = resolve_min_engine_version(target).expect(target);
            assert_eq!(result[0], 1,
                "min_engine_version[0] harus 1 untuk target '{target}', dapat: {result:?}");
        }
    }

    #[test]
    fn fase_2_resolve_min_engine_version_error_cases() {
        // Input kosong
        assert!(resolve_min_engine_version("").is_err());
        // Format ambigu (major 2..25)
        assert!(resolve_min_engine_version("2.0").is_err());
        assert!(resolve_min_engine_version("25.60").is_err());
        // Bukan angka
        assert!(resolve_min_engine_version("abc.def").is_err());
        // Terlalu banyak komponen
        assert!(resolve_min_engine_version("1.2.3.4").is_err());
    }

    #[test]
    fn fase_2_update_manifest_min_engine_version_selalu_diawali_1() {
        let dir = temp_dir("manifest");
        // Target 26.60 → min_engine_version harus [1, 26, 60]
        crate::wiring::update_manifest(&dir, "26.60", "2.11.0").unwrap();
        let raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let min = val["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min[0].as_u64(), Some(1), "elemen pertama harus 1, dapat: {min:?}");
        assert_eq!(min[1].as_u64(), Some(26));
        assert_eq!(min[2].as_u64(), Some(60));

        // Target lama 1.21.50 → min_engine_version harus [1, 21, 50]
        let dir2 = temp_dir("manifest2");
        crate::wiring::update_manifest(&dir2, "1.21.50", "1.16.0").unwrap();
        let raw2 = fs::read_to_string(dir2.join("BP/manifest.json")).unwrap();
        let val2: serde_json::Value = serde_json::from_str(&raw2).unwrap();
        let min2 = val2["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min2[0].as_u64(), Some(1));
        assert_eq!(min2[1].as_u64(), Some(21));
        assert_eq!(min2[2].as_u64(), Some(50));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn fase_2_init_workspace_min_engine_version_26_60() {
        let dir = temp_dir("init");
        let spec = BedrockSpec::bundled(); // version 26.60
        init_workspace(&dir, "TestPack", &spec).unwrap();

        let raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let min = val["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min[0].as_u64(), Some(1),
            "init_workspace: min_engine_version[0] harus 1, dapat: {min:?}");

        // Verifikasi @minecraft/server versi terbaru ada di manifest
        let dep_ver = val["dependencies"][0]["version"].as_str().unwrap_or("");
        assert_eq!(dep_ver, "2.11.0",
            "dependency @minecraft/server harus 2.11.0, dapat: '{dep_ver}'");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_2_audit_workspace_tolak_format_salah_terima_yang_benar() {
        let dir = temp_dir("audit");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // Kasus GAGAL: [26, 60, 0] – angka pertama bukan 1
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[26,60,0]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags.iter().any(|d| d.code == "PSTDR-E015"),
            "Harus tolak [26, 60, 0]: {diags:?}"
        );

        // Kasus LULUS: [1, 26, 60]
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[1,26,60]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags2 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags2.iter().any(|d| d.code == "PSTDR-E015"),
            "[1, 26, 60] harus lulus E015: {diags2:?}"
        );

        // Kasus LULUS: [1, 21, 50]
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[1,21,50]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags3 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags3.iter().any(|d| d.code == "PSTDR-E015"),
            "[1, 21, 50] harus lulus E015: {diags3:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_2_codegen_snapshot_target_baru_26_60() {
        // Snapshot codegen untuk target 26.60: custom component harus memakai
        // system.beforeEvents.startup (API v2) dan import system (bukan world).
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/ice_shard.json")

const ICE_ID = "my_pack:ice_shard"

on right click with ICE_ID by player {
    player.actionbar("§b✦ Es Retak!")
}
"#;
        let dir = temp_dir("snap26");
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::write(
            dir.join("BP/items/ice_shard.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"my_pack:ice_shard"},"components":{}}}"#,
        ).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/ice_shard.pstdr"), src).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "diags: {diags:?}");

        let js = fs::read_to_string(dir.join("BP/scripts/generated/ice_shard.js")).unwrap();
        // Harus pakai system.beforeEvents.startup (API v2)
        assert!(
            js.contains("system.beforeEvents.startup.subscribe"),
            "Harus pakai system.beforeEvents.startup:\n{js}"
        );
        // Harus import system (bukan world saja)
        assert!(js.contains("system"), "Harus import system:\n{js}");
        // Header versi tidak memakai 1.21.50 secara kaku
        assert!(js.contains("26.60"), "Header harus menyebut 26.60:\n{js}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_2_codegen_snapshot_target_lama_1_21_50() {
        // Snapshot kompatibilitas mundur: target 1.21.50 masih bisa dikompilasi.
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "1.21.50", api = "1.16.0")

const BLADE_ID = "my_pack:frost_blade"

on hit target with BLADE_ID by player {
    damage target by 5
}
"#;
        let res = compile_source(src, "legacy.pstdr", None, &spec, false, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        // Event entityHitEntity tetap sama di semua API version
        assert!(js.contains("world.afterEvents.entityHitEntity.subscribe"));
        assert!(js.contains("1.21.50"), "Header harus menyebut 1.21.50:\n{js}");
    }

    // ─── FASE 2: Tes Tambahan Custom Components V1 vs V2 & Manifest v3 ───────

    #[test]
    fn fase_2_wiring_custom_components_v1_vs_v2() {
        let dir = temp_dir("wiring_v1_v2");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. Format lama: 1.21.50 -> array minecraft:custom_components
        fs::write(
            dir.join("BP/items/v1_item.json"),
            r#"{
  "format_version": "1.21.50",
  "minecraft:item": {
    "description": { "identifier": "my_pack:v1_item" },
    "components": {}
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/v1_item.pstdr"),
            r#"
@addon(target = "1.21.50", api = "1.16.0")
bind_target("BP/items/v1_item.json")
const V1_ID = "my_pack:v1_item"
on hit target with V1_ID by player { damage target by 1 }
"#,
        ).unwrap();

        // 2. Format baru: 1.21.90 -> direct key "pstdr:...": {}
        fs::write(
            dir.join("BP/items/v2_item_90.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:v2_item_90" },
    "components": {}
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/v2_item_90.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/v2_item_90.json")
const V2_90_ID = "my_pack:v2_item_90"
on hit target with V2_90_ID by player { damage target by 1 }
"#,
        ).unwrap();

        // 3. Format baru: 1.26.60 -> direct key "pstdr:...": {}
        fs::write(
            dir.join("BP/items/v2_item_26.json"),
            r#"{
  "format_version": "1.26.60",
  "minecraft:item": {
    "description": { "identifier": "my_pack:v2_item_26" },
    "components": {}
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/v2_item_26.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/v2_item_26.json")
const V2_26_ID = "my_pack:v2_item_26"
on hit target with V2_26_ID by player { damage target by 1 }
"#,
        ).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "diags: {diags:?}");

        // Verifikasi v1_item.json (format 1.21.50): memiliki array minecraft:custom_components
        let v1_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v1_item.json")).unwrap(),
        ).unwrap();
        let comps_v1 = &v1_json["minecraft:item"]["components"];
        let arr = comps_v1["minecraft:custom_components"]
            .as_array()
            .expect("harus ada array minecraft:custom_components");
        assert!(arr.iter().any(|v| v.as_str() == Some("pstdr:v1_item")));

        // Verifikasi v2_item_90.json (format 1.21.90): key langsung, tanpa array minecraft:custom_components
        let v2_90_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v2_item_90.json")).unwrap(),
        ).unwrap();
        let comps_v2_90 = &v2_90_json["minecraft:item"]["components"];
        assert!(
            comps_v2_90.get("minecraft:custom_components").is_none(),
            "1.21.90 tidak boleh ada array minecraft:custom_components"
        );
        assert!(
            comps_v2_90.get("pstdr:v2_item_90").is_some(),
            "1.21.90 harus punya key 'pstdr:v2_item_90'"
        );
        assert_eq!(comps_v2_90["pstdr:v2_item_90"], serde_json::json!({}));

        // Verifikasi v2_item_26.json (format 1.26.60): key langsung, tanpa array minecraft:custom_components
        let v2_26_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v2_item_26.json")).unwrap(),
        ).unwrap();
        let comps_v2_26 = &v2_26_json["minecraft:item"]["components"];
        assert!(
            comps_v2_26.get("minecraft:custom_components").is_none(),
            "1.26.60 tidak boleh ada array minecraft:custom_components"
        );
        assert!(
            comps_v2_26.get("pstdr:v2_item_26").is_some(),
            "1.26.60 harus punya key 'pstdr:v2_item_26'"
        );
        assert_eq!(comps_v2_26["pstdr:v2_item_26"], serde_json::json!({}));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_2_manifest_v3_update_and_audit() {
        let dir = temp_dir("manifest_v3");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. format_version: 3 dengan "min_engine_version": "1.26.60" -> LULUS
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":"1.26.60"},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_ok = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags_ok.iter().any(|d| d.code == "PSTDR-E015"),
            "Format v3 '1.26.60' harus lulus: {diags_ok:?}"
        );

        // 2. format_version: 3 dengan "min_engine_version": "26.60.0" -> GAGAL PSTDR-E015
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":"26.60.0"},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_err1 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags_err1.iter().any(|d| d.code == "PSTDR-E015"),
            "Format v3 '26.60.0' harus gagal: {diags_err1:?}"
        );

        // 3. format_version: 3 dengan array [1, 26, 60] -> GAGAL PSTDR-E015
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":[1,26,60]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_err2 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags_err2.iter().any(|d| d.code == "PSTDR-E015"),
            "Format v3 array [1,26,60] harus gagal: {diags_err2:?}"
        );

        // 4. update_manifest pada format_version: 3 menulis SemVer string "1.26.60", bukan array
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        crate::wiring::update_manifest(&dir, "26.60", "2.11.0").unwrap();
        let manifest_content = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
        assert_eq!(
            val["header"]["min_engine_version"].as_str(),
            Some("1.26.60"),
            "v3 manifest harus punya min_engine_version string '1.26.60': {manifest_content}"
        );
        assert!(
            val["header"]["min_engine_version"].as_array().is_none(),
            "v3 manifest tidak boleh berupa array"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fase_2_codegen_api_v1_uses_world_initialize() {
        let src = r#"
@addon(target = "1.21.50", api = "1.16.0")

const SWORD_ID = "my_pack:iron_sword"

on hit target with SWORD_ID by player {
    damage target by 4
}
"#;
        let (prog, diags) = parse_source(src, "v1.pstdr");
        assert!(diags.is_empty());
        let prog = prog.unwrap();
        let opts = CodegenOptions {
            source_file: "v1.pstdr".to_string(),
            source_map_comments: false,
            custom_components: vec![("pstdr:iron_sword".to_string(), ComponentKind::Item)],
        };
        let js = emit_code(&prog, &opts, EmitMode::JavaScript);
        assert!(
            js.contains("world.beforeEvents.worldInitialize.subscribe"),
            "API v1.16.0 harus pakai world.beforeEvents.worldInitialize:\n{js}"
        );
        assert!(
            !js.contains("system.beforeEvents.startup"),
            "API v1.16.0 tidak boleh pakai startup:\n{js}"
        );
        assert!(
            js.contains("world"),
            "API v1.16.0 harus mengimpor world:\n{js}"
        );
    }

    #[test]
    fn fase_2_codegen_api_v2_uses_system_startup() {
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

const WAND_ID = "my_pack:ice_wand"

on right click with WAND_ID by player {
    player.actionbar("§bFreeze!")
}
"#;
        let (prog, diags) = parse_source(src, "v2.pstdr");
        assert!(diags.is_empty());
        let prog = prog.unwrap();
        let opts = CodegenOptions {
            source_file: "v2.pstdr".to_string(),
            source_map_comments: false,
            custom_components: vec![("pstdr:ice_wand".to_string(), ComponentKind::Item)],
        };
        let js = emit_code(&prog, &opts, EmitMode::JavaScript);
        assert!(
            js.contains("system.beforeEvents.startup.subscribe"),
            "API v2.11.0 harus pakai system.beforeEvents.startup:\n{js}"
        );
        assert!(
            !js.contains("worldInitialize"),
            "API v2.11.0 tidak boleh pakai worldInitialize:\n{js}"
        );
        assert!(
            js.contains("import {") && js.contains("system"),
            "API v2.11.0 harus mengimpor system:\n{js}"
        );
    }
}
