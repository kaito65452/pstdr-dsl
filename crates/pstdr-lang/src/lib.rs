pub mod assets;
pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod formatter;
pub mod lexer;
pub mod parser;
pub mod partition;
pub mod spec;
pub mod validator;
pub mod wiring;
pub mod workspace;

pub use assets::validate_workspace_assets;
pub use codegen::{emit_code, CodegenOptions, ComponentKind, EmitMode};
pub use diagnostics::{
    diagnostics_to_json, explain_code, format_diagnostics_compact, format_diagnostics_pretty,
    has_errors, Diagnostic, DiagnosticCode, Severity, Span,
};
pub use formatter::format_source;
pub use parser::parse_source;
pub use partition::{partition_program, PartitionedProgram, PartitionedTarget, TargetEntry, TargetRegistry};
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
    fn phase_a_forbidden_keywords_emit_e001() {
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
    fn phase_a_const_mutation_e002_and_mut_allowed() {
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
    fn phase_a_multi_error_e010_and_e005_and_e004() {
        let spec = BedrockSpec::bundled();
        let src = r#"
every 0s {
    send "test" to player
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
    fn phase_a_effect_level_zero_e011_and_invalid_vanilla_e013() {
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
    fn phase_a_unsafe_isolation_and_unclosed_brace_e012() {
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
    fn phase_b_codegen_before_after_events_and_amplifier_and_snake_case() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "1.21.50", api = "1.16.0")

const BLADE_ID = "my_pack:frost_blade"
state mana[player] = 100

on break of "minecraft:stone" by player {
    send "Stone broken" to player
}

on break of "minecraft:diamond_ore" by player {
    cancel event
    player.actionbar("§cProtected!")
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
    fn phase_b_bind_target_const_propagation_and_wiring() {
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

        // Test E008 mismatch via a different const identifier
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
    fn phase_b_formatter_is_idempotent_and_preserves_unsafe() {
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

    // ─── PHASE 2: Tests for resolve_min_engine_version ───────────────────────

    #[test]
    fn phase_2_resolve_min_engine_version_modern_format() {
        // Modern format: "26.x" → [1, 26, x]
        assert_eq!(resolve_min_engine_version("26.60"), Ok([1, 26, 60]));
        assert_eq!(resolve_min_engine_version("26.0"),  Ok([1, 26, 0]));
        // Modern format with 3 components (third sub-patch ignored → major=26, minor=60)
        assert_eq!(resolve_min_engine_version("26.60.0"), Ok([1, 26, 60]));
        // Higher versions
        assert_eq!(resolve_min_engine_version("30.10"), Ok([1, 30, 10]));
    }

    #[test]
    fn phase_2_resolve_min_engine_version_legacy_format() {
        // Legacy format: "1.x.y" → [1, x, y]
        assert_eq!(resolve_min_engine_version("1.21.50"),  Ok([1, 21, 50]));
        assert_eq!(resolve_min_engine_version("1.21.130"), Ok([1, 21, 130]));
        assert_eq!(resolve_min_engine_version("1.20.80"),  Ok([1, 20, 80]));
        // "1.x" without patch → default patch 0
        assert_eq!(resolve_min_engine_version("1.20"), Ok([1, 20, 0]));
    }

    #[test]
    fn phase_2_resolve_min_engine_version_always_starts_with_1() {
        // Ensure first element is always 1 for all valid cases
        for target in &["26.60", "26.0", "1.21.50", "1.20.80", "1.21.130"] {
            let result = resolve_min_engine_version(target).expect(target);
            assert_eq!(result[0], 1,
                "min_engine_version[0] must be 1 for target '{target}', got: {result:?}");
        }
    }

    #[test]
    fn phase_2_resolve_min_engine_version_error_cases() {
        // Empty input
        assert!(resolve_min_engine_version("").is_err());
        // Ambiguous format (major 2..25)
        assert!(resolve_min_engine_version("2.0").is_err());
        assert!(resolve_min_engine_version("25.60").is_err());
        // Non-numeric
        assert!(resolve_min_engine_version("abc.def").is_err());
        // Too many components
        assert!(resolve_min_engine_version("1.2.3.4").is_err());
    }

    #[test]
    fn phase_2_update_manifest_min_engine_version_always_starts_with_1() {
        let dir = temp_dir("manifest");
        // Target 26.60 → min_engine_version must be [1, 26, 60]
        crate::wiring::update_manifest(&dir, "26.60", "2.11.0").unwrap();
        let raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let min = val["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min[0].as_u64(), Some(1), "first element must be 1, got: {min:?}");
        assert_eq!(min[1].as_u64(), Some(26));
        assert_eq!(min[2].as_u64(), Some(60));

        // Verify modules always contain BOTH types: "data" and "script" with distinct UUIDs
        let modules1 = val["modules"].as_array().expect("modules must be an array");
        let data_mod1 = modules1.iter().find(|m| m["type"] == "data").expect("must contain 'data' module");
        let script_mod1 = modules1.iter().find(|m| m["type"] == "script").expect("must contain 'script' module");
        let d_uuid1 = data_mod1["uuid"].as_str().unwrap();
        let s_uuid1 = script_mod1["uuid"].as_str().unwrap();
        assert!(!d_uuid1.is_empty());
        assert!(!s_uuid1.is_empty());
        assert_ne!(d_uuid1, s_uuid1, "data and script module UUIDs must be distinct");

        // Legacy target 1.21.50 → min_engine_version must be [1, 21, 50]
        let dir2 = temp_dir("manifest2");
        crate::wiring::update_manifest(&dir2, "1.21.50", "1.16.0").unwrap();
        let raw2 = fs::read_to_string(dir2.join("BP/manifest.json")).unwrap();
        let val2: serde_json::Value = serde_json::from_str(&raw2).unwrap();
        let min2 = val2["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min2[0].as_u64(), Some(1));
        assert_eq!(min2[1].as_u64(), Some(21));
        assert_eq!(min2[2].as_u64(), Some(50));

        let modules2 = val2["modules"].as_array().expect("modules must be an array");
        let data_mod2 = modules2.iter().find(|m| m["type"] == "data").expect("must contain 'data' module");
        let script_mod2 = modules2.iter().find(|m| m["type"] == "script").expect("must contain 'script' module");
        assert_ne!(data_mod2["uuid"], script_mod2["uuid"], "data and script module UUIDs must be distinct");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn phase_2_init_workspace_min_engine_version_26_60() {
        let dir = temp_dir("init");
        let spec = BedrockSpec::bundled(); // version 26.60
        init_workspace(&dir, "TestPack", &spec).unwrap();

        let raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let min = val["header"]["min_engine_version"].as_array().unwrap();
        assert_eq!(min[0].as_u64(), Some(1),
            "init_workspace: min_engine_version[0] must be 1, got: {min:?}");

        // Verify latest @minecraft/server version is in manifest
        let dep_ver = val["dependencies"][0]["version"].as_str().unwrap_or("");
        assert_eq!(dep_ver, "2.11.0",
            "@minecraft/server dependency must be 2.11.0, got: '{dep_ver}'");

        // Verify modules always contain BOTH types: "data" and "script" with distinct UUIDs
        let modules = val["modules"].as_array().expect("modules must be an array");
        let data_mod = modules.iter().find(|m| m["type"] == "data").expect("must contain 'data' module");
        let script_mod = modules.iter().find(|m| m["type"] == "script").expect("must contain 'script' module");
        let data_uuid = data_mod["uuid"].as_str().unwrap();
        let script_uuid = script_mod["uuid"].as_str().unwrap();
        assert!(!data_uuid.is_empty());
        assert!(!script_uuid.is_empty());
        assert_ne!(data_uuid, script_uuid, "data and script module UUIDs must be distinct");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_2_audit_workspace_rejects_invalid_and_accepts_valid_formats() {
        let dir = temp_dir("audit");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // FAIL case: [26, 60, 0] – first number is not 1
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[26,60,0]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags.iter().any(|d| d.code == "PSTDR-E015"),
            "Must reject [26, 60, 0]: {diags:?}"
        );

        // PASS case: [1, 26, 60]
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[1,26,60]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags2 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags2.iter().any(|d| d.code == "PSTDR-E015"),
            "[1, 26, 60] must pass E015: {diags2:?}"
        );

        // PASS case: [1, 21, 50]
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":2,"header":{"uuid":"a1b2c3d4-e5f6-7890-abcd-ef1234567890","version":[1,0,0],"min_engine_version":[1,21,50]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags3 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags3.iter().any(|d| d.code == "PSTDR-E015"),
            "[1, 21, 50] must pass E015: {diags3:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_2_codegen_snapshot_modern_target_26_60() {
        // Codegen snapshot for target 26.60: custom component must use
        // system.beforeEvents.startup (API v2) and import system (not world).
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/ice_shard.json")

const ICE_ID = "my_pack:ice_shard"

on right click with ICE_ID by player {
    player.actionbar("§b✦ Ice Shatter!")
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
        // Must use system.beforeEvents.startup (API v2)
        assert!(
            js.contains("system.beforeEvents.startup.subscribe"),
            "Must use system.beforeEvents.startup:\n{js}"
        );
        // Must import system (not world only)
        assert!(js.contains("system"), "Must import system:\n{js}");
        // Version header must not hardcode 1.21.50
        assert!(js.contains("26.60"), "Header must reference 26.60:\n{js}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_2_codegen_snapshot_legacy_target_1_21_50() {
        // Backward-compatibility snapshot: target 1.21.50 still compiles cleanly.
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
        // entityHitEntity event remains unchanged across all API versions
        assert!(js.contains("world.afterEvents.entityHitEntity.subscribe"));
        assert!(js.contains("1.21.50"), "Header must reference 1.21.50:\n{js}");
    }

    // ─── PHASE 2: Additional Tests for Custom Components V1 vs V2 & Manifest v3 ───

    #[test]
    fn phase_2_wiring_custom_components_v1_vs_v2() {
        let dir = temp_dir("wiring_v1_v2");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. Legacy format: 1.21.50 -> minecraft:custom_components array
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

        // 2. Modern format: 1.21.90 -> direct key "pstdr:...": {}
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

        // 3. Modern format: 1.26.60 -> direct key "pstdr:...": {}
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

        // Verify v1_item.json (format 1.21.50): has minecraft:custom_components array
        let v1_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v1_item.json")).unwrap(),
        ).unwrap();
        let comps_v1 = &v1_json["minecraft:item"]["components"];
        let arr = comps_v1["minecraft:custom_components"]
            .as_array()
            .expect("must have minecraft:custom_components array");
        assert!(arr.iter().any(|v| v.as_str() == Some("pstdr:v1_item")));

        // Verify v2_item_90.json (format 1.21.90): direct key, without minecraft:custom_components array
        let v2_90_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v2_item_90.json")).unwrap(),
        ).unwrap();
        let comps_v2_90 = &v2_90_json["minecraft:item"]["components"];
        assert!(
            comps_v2_90.get("minecraft:custom_components").is_none(),
            "1.21.90 must not contain minecraft:custom_components array"
        );
        assert!(
            comps_v2_90.get("pstdr:v2_item_90").is_some(),
            "1.21.90 must contain key 'pstdr:v2_item_90'"
        );
        assert_eq!(comps_v2_90["pstdr:v2_item_90"], serde_json::json!({}));

        // Verify v2_item_26.json (format 1.26.60): direct key, without minecraft:custom_components array
        let v2_26_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/items/v2_item_26.json")).unwrap(),
        ).unwrap();
        let comps_v2_26 = &v2_26_json["minecraft:item"]["components"];
        assert!(
            comps_v2_26.get("minecraft:custom_components").is_none(),
            "1.26.60 must not contain minecraft:custom_components array"
        );
        assert!(
            comps_v2_26.get("pstdr:v2_item_26").is_some(),
            "1.26.60 must contain key 'pstdr:v2_item_26'"
        );
        assert_eq!(comps_v2_26["pstdr:v2_item_26"], serde_json::json!({}));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_2_manifest_v3_update_and_audit() {
        let dir = temp_dir("manifest_v3");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. format_version: 3 with "min_engine_version": "1.26.60" -> PASS
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":"1.26.60"},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_ok = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !diags_ok.iter().any(|d| d.code == "PSTDR-E015"),
            "v3 format '1.26.60' must pass: {diags_ok:?}"
        );

        // 2. format_version: 3 with "min_engine_version": "26.60.0" -> FAIL PSTDR-E015
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":"26.60.0"},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_err1 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags_err1.iter().any(|d| d.code == "PSTDR-E015"),
            "v3 format '26.60.0' must fail: {diags_err1:?}"
        );

        // 3. format_version: 3 with array [1, 26, 60] -> FAIL PSTDR-E015
        fs::write(
            dir.join("BP/manifest.json"),
            r#"{"format_version":3,"header":{"uuid":"11111111-1111-4111-8111-111111111111","version":[1,0,0],"min_engine_version":[1,26,60]},"modules":[],"dependencies":[]}"#,
        ).unwrap();
        let diags_err2 = audit_workspace(&dir, &spec).unwrap();
        assert!(
            diags_err2.iter().any(|d| d.code == "PSTDR-E015"),
            "v3 format array [1,26,60] must fail: {diags_err2:?}"
        );

        // 4. update_manifest on format_version: 3 writes SemVer string "1.26.60", not array
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
            "v3 manifest must have string min_engine_version '1.26.60': {manifest_content}"
        );
        assert!(
            val["header"]["min_engine_version"].as_array().is_none(),
            "v3 manifest must not use an array"
        );

        // Verify v3 modules contain both types: "data" and "script" with version "1.0.0"
        let modules = val["modules"].as_array().expect("modules must be an array");
        let data_mod = modules.iter().find(|m| m["type"] == "data").expect("must contain 'data' module");
        let script_mod = modules.iter().find(|m| m["type"] == "script").expect("must contain 'script' module");
        assert_eq!(data_mod["version"].as_str(), Some("1.0.0"));
        assert_eq!(script_mod["version"].as_str(), Some("1.0.0"));
        let data_uuid = data_mod["uuid"].as_str().unwrap();
        let script_uuid = script_mod["uuid"].as_str().unwrap();
        assert!(!data_uuid.is_empty());
        assert!(!script_uuid.is_empty());
        assert_ne!(data_uuid, script_uuid, "data and script module UUIDs must be distinct");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_2_codegen_api_v1_uses_world_initialize() {
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
            bound_block_ids: vec![],
        };
        let js = emit_code(&prog, &opts, EmitMode::JavaScript);
        assert!(
            js.contains("world.beforeEvents.worldInitialize.subscribe"),
            "API v1.16.0 must use world.beforeEvents.worldInitialize:\n{js}"
        );
        assert!(
            !js.contains("system.beforeEvents.startup"),
            "API v1.16.0 must not use startup:\n{js}"
        );
        assert!(
            js.contains("world"),
            "API v1.16.0 must import world:\n{js}"
        );
    }

    #[test]
    fn phase_2_codegen_api_v2_uses_system_startup() {
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
            bound_block_ids: vec![],
        };
        let js = emit_code(&prog, &opts, EmitMode::JavaScript);
        assert!(
            js.contains("system.beforeEvents.startup.subscribe"),
            "API v2.11.0 must use system.beforeEvents.startup:\n{js}"
        );
        assert!(
            !js.contains("worldInitialize"),
            "API v2.11.0 must not use worldInitialize:\n{js}"
        );
        assert!(
            js.contains("import {") && js.contains("system"),
            "API v2.11.0 must import system:\n{js}"
        );
    }

    // ─── PHASE 3: Auto-Detection of Custom/Forbidden Textures Without RP (PSTDR-E017) ───

    #[test]
    fn phase_3_asset_guard_fails_compile_without_rp_on_custom_or_invalid_icon() {
        let dir = temp_dir("asset_guard_fail");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/frost_scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:frost_scythe" },
    "components": {
      "minecraft:icon": "frost_scythe"
    }
  }
}"#,
        ).unwrap();

        fs::write(
            dir.join("src/frost_scythe.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/frost_scythe.json")
const SCYTHE_ID = "my_pack:frost_scythe"
on hit target with SCYTHE_ID by player { damage target by 5 }
"#,
        ).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            diags.iter().any(|d| d.code == "PSTDR-E017"),
            "Must emit PSTDR-E017 on custom icon without RP: {diags:?}"
        );
        let js_out = dir.join("BP/scripts/generated/frost_scythe.js");
        assert!(
            !js_out.exists(),
            "Compilation must stop and NOT emit generated JS when asset errors exist"
        );

        // Also test diamond_sword (not a valid vanilla item_textures alias in 26.60)
        fs::write(
            dir.join("BP/items/frost_scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:frost_scythe" },
    "components": {
      "minecraft:icon": "diamond_sword"
    }
  }
}"#,
        ).unwrap();
        let diags2 = convert_workspace(&dir, &spec).unwrap();
        assert!(
            diags2.iter().any(|d| d.code == "PSTDR-E017"),
            "diamond_sword is not a valid vanilla alias in 26.60, must emit PSTDR-E017: {diags2:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_3_asset_guard_allows_valid_vanilla_icon_without_rp() {
        let dir = temp_dir("asset_guard_vanilla_ok");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. Direct string format: "snowball"
        fs::write(
            dir.join("BP/items/ice_ball.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:ice_ball" },
    "components": {
      "minecraft:icon": "snowball"
    }
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/ice_ball.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/ice_ball.json")
const BALL_ID = "my_pack:ice_ball"
on right click with BALL_ID by player { player.actionbar("Throw!") }
"#,
        ).unwrap();

        // 2. Modern object format: {"textures": {"default": "diamond"}}
        fs::write(
            dir.join("BP/items/gem.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:gem" },
    "components": {
      "minecraft:icon": {
        "textures": {
          "default": "diamond"
        }
      }
    }
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/gem.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/gem.json")
const GEM_ID = "my_pack:gem"
on right click with GEM_ID by player { player.actionbar("Shine!") }
"#,
        ).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags),
            "convert_workspace must pass with 0 errors for vanilla icons: {diags:?}"
        );
        let audit_diags = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&audit_diags),
            "audit_workspace must pass with 0 errors for vanilla icons: {audit_diags:?}"
        );
        assert!(dir.join("BP/scripts/generated/ice_ball.js").exists());
        assert!(dir.join("BP/scripts/generated/gem.js").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_3_asset_guard_rejects_raw_png_path_in_icon() {
        let dir = temp_dir("asset_guard_raw_png");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:scythe" },
    "components": {
      "minecraft:icon": "textures/items/scythe.png"
    }
  }
}"#,
        ).unwrap();
        fs::write(
            dir.join("src/scythe.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/scythe.json")
const SCYTHE_ID = "my_pack:scythe"
on hit target with SCYTHE_ID by player { damage target by 3 }
"#,
        ).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            diags.iter().any(|d| d.code == "PSTDR-E017"),
            "Must fail with PSTDR-E017 when using a raw .png path: {diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_3_asset_guard_passes_with_rp_and_fails_on_missing_png() {
        let dir = temp_dir("asset_guard_rp");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("RP/textures")).unwrap();

        fs::write(
            dir.join("RP/manifest.json"),
            r#"{
  "format_version": 2,
  "header": {
    "name": "Pack RP",
    "description": "RP",
    "uuid": "22222222-2222-4222-8222-222222222222",
    "version": [1, 0, 0],
    "min_engine_version": [1, 26, 60]
  },
  "modules": []
}"#,
        ).unwrap();

        fs::write(
            dir.join("RP/textures/item_texture.json"),
            r#"{
  "resource_pack_name": "pack",
  "texture_name": "atlas.items",
  "texture_data": {
    "frost_scythe": {
      "textures": "textures/items/frost_scythe"
    }
  }
}"#,
        ).unwrap();

        fs::write(
            dir.join("BP/items/frost_scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:frost_scythe" },
    "components": {
      "minecraft:icon": "frost_scythe"
    }
  }
}"#,
        ).unwrap();

        fs::write(
            dir.join("src/frost_scythe.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")
bind_target("BP/items/frost_scythe.json")
const SCYTHE_ID = "my_pack:frost_scythe"
on hit target with SCYTHE_ID by player { damage target by 5 }
"#,
        ).unwrap();

        // 1. Physical image file missing -> must fail with PSTDR-E017
        let diags_missing = convert_workspace(&dir, &spec).unwrap();
        assert!(
            diags_missing.iter().any(|d| d.code == "PSTDR-E017"),
            "Must fail with PSTDR-E017 because physical image file is missing: {diags_missing:?}"
        );

        // 2. Create .png file at RP/textures/items/frost_scythe.png
        fs::create_dir_all(dir.join("RP/textures/items")).unwrap();
        fs::write(dir.join("RP/textures/items/frost_scythe.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        // 3. After image is created -> convert_workspace & audit_workspace must pass with 0 errors
        let diags_ok = convert_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags_ok),
            "convert_workspace must succeed once png file exists: {diags_ok:?}"
        );
        let audit_diags = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&audit_diags),
            "audit_workspace must succeed once png file exists: {audit_diags:?}"
        );
        assert!(dir.join("BP/scripts/generated/frost_scythe.js").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_error_two_syntax_errors_top_level() {
        let src = "let a = 1\nlet b = 2\n";
        let (_prog, diags) = parse_source(src, "two_errors.pstdr");
        let e001_count = diags.iter().filter(|d| d.code == "PSTDR-E001").count();
        assert!(
            e001_count >= 2,
            "Expected at least 2 E001 errors, got {e001_count}: {diags:?}"
        );
        let lines: Vec<usize> = diags
            .iter()
            .filter(|d| d.code == "PSTDR-E001")
            .map(|d| d.line)
            .collect();
        assert!(lines.contains(&1), "Expected error on line 1: {lines:?}");
        assert!(lines.contains(&2), "Expected error on line 2: {lines:?}");
    }

    #[test]
    fn multi_error_two_errors_inside_same_block() {
        let spec = BedrockSpec::bundled();
        let src = r#"
on join of player {
    let x = 1
    const y = 2
    y = 3
}
"#;
        let res = compile_source(src, "block_multi.pstdr", None, &spec, true, true, vec![]);
        let codes: Vec<&str> = res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"PSTDR-E001"), "missing E001 in diags: {codes:?}");
        assert!(codes.contains(&"PSTDR-E002"), "missing E002 in diags: {codes:?}");
    }

    #[test]
    fn multi_error_syntax_plus_semantic_in_one_file() {
        let spec = BedrockSpec::bundled();
        let src = r#"
let a = 1
on join of player {
    apply slownes 0 to player for 3s
}
"#;
        let res = compile_source(src, "syn_sem.pstdr", None, &spec, true, true, vec![]);
        assert!(res.js.is_none(), "convert must fail when errors exist");
        assert!(res.ts.is_none(), "convert must fail when errors exist");
        let codes: Vec<&str> = res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"PSTDR-E001"), "missing E001 in diags: {codes:?}");
        assert!(codes.contains(&"PSTDR-E011"), "missing E011 in diags: {codes:?}");
        assert!(codes.contains(&"PSTDR-E013"), "missing E013 in diags: {codes:?}");

        let dir = temp_dir("multi_error_check_path");
        let file = dir.join("syn_sem.pstdr");
        fs::write(&file, src).unwrap();
        let check_diags = check_path(&file, None, &spec).unwrap();
        let check_codes: Vec<&str> = check_diags.iter().map(|d| d.code.as_str()).collect();
        assert!(check_codes.contains(&"PSTDR-E001"), "check_path missing E001: {check_codes:?}");
        assert!(check_codes.contains(&"PSTDR-E011"), "check_path missing E011: {check_codes:?}");
        assert!(check_codes.contains(&"PSTDR-E013"), "check_path missing E013: {check_codes:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_error_no_infinite_loop_on_garbage_input() {
        let spec = BedrockSpec::bundled();
        let garbage = "@@@ {{{ ))) let ===";
        let res1 = compile_source(garbage, "garbage1.pstdr", None, &spec, true, true, vec![]);
        assert!(has_errors(&res1.diagnostics), "Expected errors for garbage input");

        let unclosed = "on join of player { let x =";
        let res2 = compile_source(unclosed, "garbage2.pstdr", None, &spec, true, true, vec![]);
        assert!(has_errors(&res2.diagnostics), "Expected errors for unclosed block");
    }

    #[test]
    fn phase_4_rlcraft_core_events_and_spawn_particle_codegen_and_fmt() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

const FOOD_ID = "example:potion"

on hurt target by attacker {
    if attacker != null {
        spawn particle "minecraft:totem_particle" at target
    }
}

on die of victim by killer {
    if killer != null {
        killer.play_sound("random.orb")
    }
}

on consume FOOD_ID by player {
    apply strength 2 to player for 5s
    spawn particle "minecraft:heart_particle" at player.location in player.dimension
}
"#;
        let res = compile_source(src, "rlcraft_events.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("world.afterEvents.entityHurt.subscribe"));
        assert!(js.contains("const target = ev.hurtEntity;"));
        assert!(js.contains("const attacker = ev.damageSource?.damagingEntity;"));
        assert!(js.contains("target.dimension.spawnParticle(\"minecraft:totem_particle\", target.location);"));

        assert!(js.contains("world.afterEvents.entityDie.subscribe"));
        assert!(js.contains("const victim = ev.deadEntity;"));
        assert!(js.contains("const killer = ev.damageSource?.damagingEntity;"));

        assert!(js.contains("world.afterEvents.itemCompleteUse.subscribe"));
        assert!(js.contains("if (!ev.itemStack || ev.itemStack.typeId !== (FOOD_ID)) return;"));
        assert!(js.contains("player.addEffect(\"strength\", 100, { amplifier: 1 });"));
        assert!(js.contains("player.dimension.spawnParticle(\"minecraft:heart_particle\", player.location);"));

        let fmt1 = format_source(src, "rlcraft_events.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "rlcraft_events.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for new events and spawn particle");
    }

    #[test]
    fn phase_4_negative_validation_for_new_events() {
        let spec = BedrockSpec::bundled();
        let bad = r#"
on hurt target {
    cancel event
}
on die of victim {
    cancel event
}
on consume "example:apple" by player {
    cancel event
}
"#;
        let res = compile_source(bad, "bad_cancel.pstdr", None, &spec, true, true, vec![]);
        let e004_count = res.diagnostics.iter().filter(|d| d.code == "PSTDR-E004").count();
        assert_eq!(e004_count, 3, "Expected 3 E004 errors, got: {:?}", res.diagnostics);
    }

    #[test]
    fn phase_4_asset_guard_allows_namespaced_rp_alias_and_variations() {
        let dir = temp_dir("asset_namespaced_rp");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("BP/blocks")).unwrap();
        fs::create_dir_all(dir.join("RP/textures/items")).unwrap();
        fs::create_dir_all(dir.join("RP/textures/blocks")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("RP/manifest.json"),
            r#"{"format_version":2,"header":{"name":"RP","description":"RP","uuid":"22222222-2222-4222-8222-222222222222","version":[1,0,0],"min_engine_version":[1,26,60]},"modules":[]}"#,
        ).unwrap();
        fs::write(
            dir.join("RP/textures/item_texture.json"),
            r#"{"texture_data":{"example:potion":{"textures":"textures/items/potion"}}}"#,
        ).unwrap();
        fs::write(
            dir.join("RP/textures/terrain_texture.json"),
            r#"{"texture_data":{"example:stone_var":{"textures":{"variations":[{"path":"textures/blocks/stone1"}]}}}}"#,
        ).unwrap();
        fs::write(dir.join("RP/textures/items/potion.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        fs::write(dir.join("RP/textures/blocks/stone1.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        fs::write(
            dir.join("BP/items/potion.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"example:potion"},"components":{"minecraft:icon":"example:potion"}}}"#,
        ).unwrap();
        fs::write(
            dir.join("BP/blocks/stone_var.json"),
            r#"{"format_version":"1.21.90","minecraft:block":{"description":{"identifier":"example:stone_var"},"components":{"minecraft:material_instances":{"*":{"texture":"example:stone_var"}}}}}"#,
        ).unwrap();

        let diags = validate_workspace_assets(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "Registered namespaced alias & variations must pass: {diags:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    // ─── WAVE 2A: Custom Block Events, Projectile Hit, Entity Spawn & Spawn Particle ───

    #[test]
    fn wave_2a_block_interact_and_tick_custom_component_wiring_v2() {
        let dir = temp_dir("wave2a_block_v2");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/blocks")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/blocks/berry_bush.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:block": {
    "description": { "identifier": "my_pack:berry_bush" },
    "components": {}
  }
}"#,
        )
        .unwrap();

        let script = r#"@addon(target = "26.60", api = "2.11.0")

bind_target("BP/blocks/berry_bush.json")

const BUSH_ID = "my_pack:berry_bush"

on interact of BUSH_ID by player {
    send "Harvested berries!" to player
}

on tick of BUSH_ID {
    spawn particle "minecraft:villager_happy" at block.location in dimension
}
"#;
        fs::write(dir.join("src/berry_bush.pstdr"), script).unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags),
            "convert_workspace must succeed for custom block interact + tick: {diags:?}"
        );

        let js = fs::read_to_string(dir.join("BP/scripts/generated/berry_bush.js")).unwrap();
        assert!(
            js.contains("system.beforeEvents.startup.subscribe((initEv) => {"),
            "Bedrock 26.60 must use system.beforeEvents.startup:\n{js}"
        );
        assert!(
            js.contains("initEv.blockComponentRegistry.registerCustomComponent(\"pstdr:berry_bush\", {"),
            "Must register custom block component:\n{js}"
        );
        assert!(
            js.contains("onPlayerInteract(ev) {"),
            "Must emit onPlayerInteract:\n{js}"
        );
        assert!(
            js.contains("const player = ev.player;") && js.contains("const block = ev.block;"),
            "onPlayerInteract must bind actor and block:\n{js}"
        );
        assert!(
            js.contains("onTick(ev) {"),
            "Must emit onTick:\n{js}"
        );
        assert!(
            js.contains("const dimension = ev.dimension;"),
            "onTick must bind dimension:\n{js}"
        );
        assert!(
            js.contains("dimension.spawnParticle(\"minecraft:villager_happy\", block.location);"),
            "onTick must emit spawnParticle:\n{js}"
        );

        let ts = fs::read_to_string(dir.join("build/ts/berry_bush.ts")).unwrap();
        assert!(ts.contains("onPlayerInteract(ev: any) {"));
        assert!(ts.contains("onTick(ev: any) {"));

        // Verify BP/blocks/berry_bush.json got V2 component key wired
        let wired_json = fs::read_to_string(dir.join("BP/blocks/berry_bush.json")).unwrap();
        assert!(
            wired_json.contains("\"pstdr:berry_bush\": {}"),
            "V2 block JSON must receive direct component key:\n{wired_json}"
        );

        // Verify formatter round-trip
        let fmt1 = format_source(script, "berry_bush.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "berry_bush.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for block interact & tick");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wave_2a_unbound_block_interact_before_and_after_events() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

on interact of "minecraft:chest" by player {
    send "Opened chest!" to player
}

on interact of "minecraft:tnt" by player {
    cancel event
    send "TNT is disabled!" to player
}
"#;
        let res = compile_source(src, "vanilla_interact.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(
            js.contains("world.afterEvents.playerInteractWithBlock.subscribe((ev) => {"),
            "Unbound non-cancellable interact must use afterEvents:\n{js}"
        );
        assert!(
            js.contains("if (ev.block.typeId !== (\"minecraft:chest\")) return;"),
            "Must filter by ev.block.typeId:\n{js}"
        );
        assert!(
            js.contains("world.beforeEvents.playerInteractWithBlock.subscribe((ev) => {"),
            "Unbound cancellable interact must use beforeEvents:\n{js}"
        );
        assert!(
            js.contains("ev.cancel = true;") && js.contains("system.run(() => {"),
            "Before-event interact must set ev.cancel and wrap mutations in system.run:\n{js}"
        );
    }

    #[test]
    fn wave_2a_block_tick_requires_bound_block_target() {
        let spec = BedrockSpec::bundled();
        let bad = r#"
@addon(target = "26.60", api = "2.11.0")

on tick of "my_pack:unbound_block" {
    spawn particle "minecraft:villager_happy" at block.location in dimension
}
"#;
        let res = compile_source(bad, "bad_tick.pstdr", None, &spec, true, true, vec![]);
        assert!(
            res.diagnostics.iter().any(|d| d.code == "PSTDR-E007"),
            "on tick without bind_target(\"BP/blocks/...\") must emit validator error: {:?}",
            res.diagnostics
        );
    }

    #[test]
    fn wave_2a_projectile_hit_and_entity_spawn_codegen_ts_and_fmt() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

const SPEAR_ID = "hfrlc:diamond_spear"

on projectile hit victim with SPEAR_ID by shooter {
    damage victim by 6
    if shooter != null {
        send "Direct hit with {projectile.typeId}!" to shooter
    }
}

on projectile hit victim {
    ignite victim for 2s
}

on spawn of mob type "minecraft:zombie" {
    apply strength 1 to mob for 10s
}

on spawn of entity {
    spawn particle "minecraft:totem_particle" at entity
}
"#;
        let res = compile_source(src, "proj_spawn.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);

        let js = res.js.unwrap();
        assert!(js.contains("world.afterEvents.projectileHitEntity.subscribe((ev) => {"));
        assert!(js.contains("const victim = ev.getEntityHit()?.entity;"));
        assert!(js.contains("if (!victim) return;"));
        assert!(js.contains("if (!ev.projectile || ev.projectile.typeId !== (SPEAR_ID)) return;"));
        assert!(js.contains("const shooter = ev.source;"));
        assert!(js.contains("const projectile = ev.projectile;"));

        assert!(js.contains("world.afterEvents.entitySpawn.subscribe((ev) => {"));
        assert!(js.contains("const mob = ev.entity;"));
        assert!(js.contains("if (mob.typeId !== (\"minecraft:zombie\")) return;"));
        assert!(js.contains("const entity = ev.entity;"));

        let ts = res.ts.unwrap();
        assert!(ts.contains("world.afterEvents.projectileHitEntity.subscribe((ev: any) => {"));
        assert!(ts.contains("world.afterEvents.entitySpawn.subscribe((ev: any) => {"));

        let fmt1 = format_source(src, "proj_spawn.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "proj_spawn.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for projectile hit and entity spawn");
    }

    #[test]
    fn wave_2a_spawn_particle_coordinate_literal_vs_entity() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

on die of dead {
    const dim = dead.dimension
    spawn particle "minecraft:basic_flame_particle" at { x = 10, y = 64, z = -5 }
    spawn particle "minecraft:basic_flame_particle" at { x = 1, y = 2, z = 3 } in dim
    spawn particle "minecraft:totem_particle" at dead
    spawn particle "minecraft:heart_particle" at dead in dim
}
"#;
        let res = compile_source(src, "particles.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(
            js.contains("world.getDimension(\"overworld\").spawnParticle(\"minecraft:basic_flame_particle\", { x: 10, y: 64, z: -5 });"),
            "Object literal without dim must NOT append .location:\n{js}"
        );
        assert!(
            js.contains("dim.spawnParticle(\"minecraft:basic_flame_particle\", { x: 1, y: 2, z: 3 });"),
            "Object literal with dim must NOT append .location:\n{js}"
        );
        assert!(
            js.contains("dead.dimension.spawnParticle(\"minecraft:totem_particle\", dead.location);"),
            "Entity identifier without dim must emit entity.dimension and entity.location:\n{js}"
        );
        assert!(
            js.contains("dim.spawnParticle(\"minecraft:heart_particle\", dead.location);"),
            "Entity identifier with explicit dim must still append .location:\n{js}"
        );
    }

    // ─── WAVE 2B: Identifier-Driven Partitioning (partition.rs) ───

    #[test]
    fn partition_single_file_multiple_items_and_blocks() {
        let dir = temp_dir("wave2b_partition_multi");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("BP/blocks")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/frost_scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "pack:frost_scythe" },
    "components": { "minecraft:icon": "snowball" }
  }
}"#,
        )
        .unwrap();

        fs::write(
            dir.join("BP/items/fire_blade.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "pack:fire_blade" },
    "components": { "minecraft:icon": "diamond" }
  }
}"#,
        )
        .unwrap();

        fs::write(
            dir.join("BP/blocks/magic_altar.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:block": {
    "description": { "identifier": "pack:magic_altar" },
    "components": {}
  }
}"#,
        )
        .unwrap();

        fs::write(
            dir.join("src/all_weapons.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

const SCYTHE_ID = "pack:frost_scythe"
const BLADE_ID = "pack:fire_blade"
const ALTAR_ID = "pack:magic_altar"
const BONUS_DMG = 6
mut total_hits = 0

func announce_hit(p) {
    total_hits += 1
    send "Total hits: {total_hits}" to p
}

on hit target with SCYTHE_ID by player {
    damage target by BONUS_DMG
    announce_hit(player)
}

on right click with SCYTHE_ID by player {
    player.actionbar("Frost Nova!")
}

on hit target with BLADE_ID by player {
    ignite target for 3s
}

on interact of ALTAR_ID by player {
    send "Altar activated!" to player
}

on tick of ALTAR_ID {
    spawn particle "minecraft:villager_happy" at block.location in dimension
}

on join of player {
    send "Welcome to the arena!" to player
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags),
            "convert_workspace must succeed for multi-target file: {diags:?}"
        );

        // Verify distinct generated modules exist
        let scythe_js =
            fs::read_to_string(dir.join("BP/scripts/generated/items/frost_scythe.js")).unwrap();
        let blade_js =
            fs::read_to_string(dir.join("BP/scripts/generated/items/fire_blade.js")).unwrap();
        let altar_js =
            fs::read_to_string(dir.join("BP/scripts/generated/blocks/magic_altar.js")).unwrap();
        let shared_js =
            fs::read_to_string(dir.join("BP/scripts/generated/_shared/all_weapons.shared.js"))
                .unwrap();
        let world_js =
            fs::read_to_string(dir.join("BP/scripts/generated/world/all_weapons.js")).unwrap();
        let registry_js =
            fs::read_to_string(dir.join("BP/scripts/generated/_registry.js")).unwrap();
        let main_js = fs::read_to_string(dir.join("BP/scripts/main.js")).unwrap();

        // Item A merges on hit + on right click into a single custom component export
        assert!(scythe_js.contains("export const component = {"));
        assert!(scythe_js.contains("onUse(ev) {"));
        assert!(scythe_js.contains("onHitEntity(ev) {"));
        assert!(scythe_js.contains("from \"../_shared/all_weapons.shared.js\""));
        assert!(scythe_js.contains("// @pstdr-source: src/all_weapons.pstdr:"));

        // Item B has onHitEntity
        assert!(blade_js.contains("export const component = {"));
        assert!(blade_js.contains("onHitEntity(ev) {"));
        assert!(blade_js.contains("target.setOnFire(3, true);"));

        // Block merges on interact + on tick into a single block component export
        assert!(altar_js.contains("export const component = {"));
        assert!(altar_js.contains("onPlayerInteract(ev) {"));
        assert!(altar_js.contains("onTick(ev) {"));
        assert!(altar_js.contains("dimension.spawnParticle(\"minecraft:villager_happy\", block.location);"));

        // Shared module exports const, __pstdr_mut, and func
        assert!(shared_js.contains("export const BONUS_DMG = 6;"));
        assert!(shared_js.contains("export const __pstdr_mut = {"));
        assert!(shared_js.contains("total_hits: 0"));
        assert!(shared_js.contains("export function announce_hit(p) {"));
        assert!(shared_js.contains("__pstdr_mut.total_hits += 1;"));

        // World module contains on join
        assert!(world_js.contains("world.afterEvents.playerSpawn.subscribe"));
        assert!(world_js.contains("Welcome to the arena!"));

        // Centralized _registry.js registers all 3 components in one startup callback
        assert!(registry_js.contains("system.beforeEvents.startup.subscribe"));
        assert!(registry_js.contains("registerCustomComponent(\"pstdr:frost_scythe\", item_frost_scythe)"));
        assert!(registry_js.contains("registerCustomComponent(\"pstdr:fire_blade\", item_fire_blade)"));
        assert!(registry_js.contains("registerCustomComponent(\"pstdr:magic_altar\", block_magic_altar)"));

        // main.js imports _registry.js and world/all_weapons.js
        assert!(main_js.contains("import \"./generated/_registry.js\";"));
        assert!(main_js.contains("import \"./generated/world/all_weapons.js\";"));

        // All 3 BP JSON files are wired with their distinct component IDs
        let scythe_json = fs::read_to_string(dir.join("BP/items/frost_scythe.json")).unwrap();
        let blade_json = fs::read_to_string(dir.join("BP/items/fire_blade.json")).unwrap();
        let altar_json = fs::read_to_string(dir.join("BP/blocks/magic_altar.json")).unwrap();
        assert!(scythe_json.contains("\"pstdr:frost_scythe\": {}"));
        assert!(blade_json.contains("\"pstdr:fire_blade\": {}"));
        assert!(altar_json.contains("\"pstdr:magic_altar\": {}"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partition_orphan_cleanup_removes_deleted_item_module() {
        let dir = temp_dir("wave2b_orphan_cleanup");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/item_a.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"pack:item_a"},"components":{"minecraft:icon":"snowball"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("BP/items/item_b.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"pack:item_b"},"components":{"minecraft:icon":"diamond"}}}"#,
        )
        .unwrap();

        // First build with both item_a and item_b
        fs::write(
            dir.join("src/arsenal.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

on hit target with "pack:item_a" by player {
    damage target by 4
}

on hit target with "pack:item_b" by player {
    ignite target for 2s
}
"#,
        )
        .unwrap();

        let diags1 = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags1), "first convert failed: {diags1:?}");
        assert!(dir.join("BP/scripts/generated/items/item_a.js").is_file());
        assert!(dir.join("BP/scripts/generated/items/item_b.js").is_file());

        // Remove item_b from the .pstdr file and re-run convert_workspace
        fs::write(
            dir.join("src/arsenal.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

on hit target with "pack:item_a" by player {
    damage target by 4
}
"#,
        )
        .unwrap();

        let diags2 = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags2), "second convert failed: {diags2:?}");
        assert!(
            dir.join("BP/scripts/generated/items/item_a.js").is_file(),
            "item_a.js must still exist"
        );
        assert!(
            !dir.join("BP/scripts/generated/items/item_b.js").exists(),
            "stale item_b.js must be deleted by orphan cleanup"
        );

        let registry_js =
            fs::read_to_string(dir.join("BP/scripts/generated/_registry.js")).unwrap();
        assert!(registry_js.contains("pstdr:item_a"));
        assert!(
            !registry_js.contains("item_b"),
            "_registry.js must no longer reference deleted item_b:\n{registry_js}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partition_shared_mut_compiles_without_es_module_reassignment_error() {
        let dir = temp_dir("wave2b_shared_mut");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/items/dagger.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"pack:dagger"},"components":{"minecraft:icon":"snowball"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("BP/items/hammer.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"pack:hammer"},"components":{"minecraft:icon":"diamond"}}}"#,
        )
        .unwrap();

        fs::write(
            dir.join("src/combo.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

mut combo_count = 0

on hit target with "pack:dagger" by player {
    combo_count += 1
    player.actionbar("Combo: {combo_count}")
}

on hit target with "pack:hammer" by player {
    combo_count = 0
    damage target by 10
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "convert_workspace failed: {diags:?}");

        let shared_js =
            fs::read_to_string(dir.join("BP/scripts/generated/_shared/combo.shared.js")).unwrap();
        let dagger_js =
            fs::read_to_string(dir.join("BP/scripts/generated/items/dagger.js")).unwrap();
        let hammer_js =
            fs::read_to_string(dir.join("BP/scripts/generated/items/hammer.js")).unwrap();

        assert!(shared_js.contains("export const __pstdr_mut = {"));
        assert!(shared_js.contains("combo_count: 0"));
        assert!(!shared_js.contains("export let combo_count"));

        assert!(dagger_js.contains("import { __pstdr_mut } from \"../_shared/combo.shared.js\";"));
        assert!(dagger_js.contains("__pstdr_mut.combo_count += 1;"));
        assert!(dagger_js.contains("${__pstdr_mut.combo_count}"));

        assert!(hammer_js.contains("import { __pstdr_mut } from \"../_shared/combo.shared.js\";"));
        assert!(hammer_js.contains("__pstdr_mut.combo_count = 0;"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partition_cancellable_block_interact_routes_to_world_before_events() {
        let dir = temp_dir("wave2b_cancel_interact");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/blocks")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        fs::write(
            dir.join("BP/blocks/locked_chest.json"),
            r#"{"format_version":"1.21.90","minecraft:block":{"description":{"identifier":"pack:locked_chest"},"components":{}}}"#,
        )
        .unwrap();

        fs::write(
            dir.join("src/vault.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

const CHEST_ID = "pack:locked_chest"

on interact of CHEST_ID by player {
    cancel event
    send "Locked!" to player
}

on tick of CHEST_ID {
    spawn particle "minecraft:villager_happy" at block.location in dimension
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "convert_workspace failed: {diags:?}");

        let block_js =
            fs::read_to_string(dir.join("BP/scripts/generated/blocks/locked_chest.js")).unwrap();
        let world_js =
            fs::read_to_string(dir.join("BP/scripts/generated/world/vault.js")).unwrap();

        assert!(block_js.contains("onTick(ev) {"));
        assert!(!block_js.contains("onPlayerInteract"));
        assert!(world_js.contains("world.beforeEvents.playerInteractWithBlock.subscribe"));
        assert!(world_js.contains("ev.cancel = true;"));

        let _ = fs::remove_dir_all(&dir);
    }

    // ─── BLIND-TEST FEEDBACK: Leave/Chat Events, Safe ItemStack & Scoreboard Helpers ───

    #[test]
    fn on_leave_codegen_system_run_and_format_roundtrip() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

state active[player] = true

on leave of player {
    const leaving_name = player.name
    active[player] = false
    send "{leaving_name} disconnected" to world
}
"#;
        let res = compile_source(src, "leave_test.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { world, system } from \"@minecraft/server\";"), "js:\n{js}");
        assert!(js.contains("world.beforeEvents.playerLeave.subscribe((ev) => {"), "js:\n{js}");
        assert!(js.contains("const player = ev.player;"), "js:\n{js}");
        assert!(js.contains("const leaving_name = player.name;"), "js:\n{js}");
        assert!(
            js.contains("system.run(() => {\n        __set_active(player, false);\n    });"),
            "state write in on leave must be wrapped in system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n        world.sendMessage(`${leaving_name} disconnected`);\n    });"),
            "send in on leave must be wrapped in system.run:\n{js}"
        );

        let fmt1 = format_source(src, "leave_test.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "leave_test.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for on leave");
        assert!(fmt1.contains("on leave of player {"));

        // `cancel event` is NOT valid in `on leave`
        let bad = "on leave of player {\n    cancel event\n}\n";
        let bad_res = compile_source(bad, "bad_leave.pstdr", None, &spec, true, true, vec![]);
        assert!(
            bad_res.diagnostics.iter().any(|d| d.code == "PSTDR-E004"),
            "cancel event in on leave must emit PSTDR-E004: {:?}",
            bad_res.diagnostics
        );
    }

    #[test]
    fn on_chat_cancel_sync_and_mutations_wrapped_in_system_run() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0-beta")

on chat msg by player {
    if msg == "!kit" {
        cancel event
        send "Granting kit..." to player
        player.give_item("minecraft:blaze_rod", 1, "Starter")
    }
}
"#;
        let res = compile_source(src, "chat_test.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(
            js.contains("import { world, system, ItemStack } from \"@minecraft/server\";"),
            "js:\n{js}"
        );
        assert!(js.contains("world.beforeEvents.chatSend.subscribe((ev) => {"), "js:\n{js}");
        assert!(js.contains("const player = ev.sender;"), "js:\n{js}");
        assert!(js.contains("const msg = ev.message;"), "js:\n{js}");
        // ev.cancel = true must remain synchronous (NOT wrapped in system.run)
        assert!(js.contains("        ev.cancel = true;\n"), "ev.cancel must be synchronous:\n{js}");
        assert!(
            js.contains("system.run(() => {\n            player.sendMessage(\"Granting kit...\");\n        });"),
            "send must be wrapped in system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n            __pstdr_give_item(player, \"minecraft:blaze_rod\", 1, \"Starter\");\n        });"),
            "give_item must be wrapped in system.run:\n{js}"
        );

        let fmt1 = format_source(src, "chat_test.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "chat_test.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for on chat");
        assert!(fmt1.contains("on chat msg by player {"));
    }

    #[test]
    fn safe_item_stack_give_item_and_spawn_item_single_and_partitioned() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on join of player {
    player.give_item("minecraft:blaze_rod", 1, "Starter")
    player.dimension.spawn_item("minecraft:diamond", player, 3)
}
"#;
        let res = compile_source(src, "items_test.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { world, ItemStack } from \"@minecraft/server\";"), "js:\n{js}");
        assert!(js.contains("function __pstdr_give_item(entity, itemId, amount = 1, nameTag = null)"), "js:\n{js}");
        assert!(js.contains("const stack = new ItemStack(itemId, amount);"), "js:\n{js}");
        assert!(js.contains("if (nameTag != null) stack.nameTag = nameTag;"), "js:\n{js}");
        assert!(js.contains("function __pstdr_spawn_item(dim, itemOrId, locOrEntity, amount = 1)"), "js:\n{js}");
        assert!(js.contains("__pstdr_give_item(player, \"minecraft:blaze_rod\", 1, \"Starter\");"), "js:\n{js}");
        assert!(js.contains("__pstdr_spawn_item(player.dimension, \"minecraft:diamond\", player, 3);"), "js:\n{js}");

        // Partitioned workspace test: give_item in item component + on leave / on chat in world module
        let dir = temp_dir("safe_item_partitioned");
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("BP/items/kit_token.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"pack:kit_token"},"components":{"minecraft:icon":"diamond"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src/arena.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0-beta")

on right click with "pack:kit_token" by player {
    player.give_item("minecraft:blaze_rod", 1, "Starter")
}

on leave of player {
    send "{player.name} left" to world
}

on chat msg by player {
    if msg == "!ping" {
        cancel event
        send "pong" to player
    }
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "partitioned convert failed: {diags:?}");

        let item_js = fs::read_to_string(dir.join("BP/scripts/generated/items/kit_token.js")).unwrap();
        assert!(item_js.contains("import { ItemStack } from \"@minecraft/server\";"), "item_js:\n{item_js}");
        assert!(item_js.contains("function __pstdr_give_item("), "item_js:\n{item_js}");
        assert!(item_js.contains("__pstdr_give_item(player, \"minecraft:blaze_rod\", 1, \"Starter\");"), "item_js:\n{item_js}");

        let world_js = fs::read_to_string(dir.join("BP/scripts/generated/world/arena.js")).unwrap();
        assert!(world_js.contains("world.beforeEvents.playerLeave.subscribe"), "world_js:\n{world_js}");
        assert!(world_js.contains("world.beforeEvents.chatSend.subscribe"), "world_js:\n{world_js}");
        assert!(world_js.contains("ev.cancel = true;"), "world_js:\n{world_js}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scoreboard_helpers_codegen_and_before_event_safety() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0-beta")

on chat msg by player {
    const kills = player.get_score("kills")
    if msg == "!reset" and kills > 0 {
        cancel event
        player.set_score("kills", 0)
        player.add_score("resets", 1)
    }
}
"#;
        let res = compile_source(src, "scoreboard_test.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { world, system } from \"@minecraft/server\";"), "js:\n{js}");
        assert!(js.contains("function __pstdr_get_score(target, objectiveId)"), "js:\n{js}");
        assert!(js.contains("function __pstdr_set_score(target, objectiveId, value)"), "js:\n{js}");
        assert!(js.contains("function __pstdr_add_score(target, objectiveId, delta)"), "js:\n{js}");
        assert!(
            js.contains("world.scoreboard.getObjective(objectiveId) ?? world.scoreboard.addObjective(objectiveId, objectiveId)"),
            "set/add_score must auto-create missing objectives:\n{js}"
        );
        // get_score in const binding must be synchronous
        assert!(js.contains("    const kills = __pstdr_get_score(player, \"kills\");\n"), "js:\n{js}");
        // set_score and add_score must be wrapped in system.run inside beforeEvents
        assert!(
            js.contains("system.run(() => {\n            __pstdr_set_score(player, \"kills\", 0);\n        });"),
            "set_score must be wrapped in system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n            __pstdr_add_score(player, \"resets\", 1);\n        });"),
            "add_score must be wrapped in system.run:\n{js}"
        );
    }

    #[test]
    fn on_chat_rejected_on_stable_api_with_e018() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on chat msg by player {
    if msg == "!ping" {
        cancel event
        send "pong" to player
    }
}
"#;
        let res = compile_source(src, "stable_chat.pstdr", None, &spec, true, true, vec![]);
        assert!(res.js.is_none(), "stable API + on chat must emit no JS output");
        assert!(res.ts.is_none(), "stable API + on chat must emit no TS output");
        assert!(
            res.diagnostics.iter().any(|d| d.code == "PSTDR-E018"),
            "Expected PSTDR-E018 on stable API, got: {:?}",
            res.diagnostics
        );

        let dir = temp_dir("stable_chat_check");
        let file = dir.join("stable_chat.pstdr");
        fs::write(&file, src).unwrap();
        let check_diags = check_path(&file, None, &spec).unwrap();
        assert!(
            check_diags.iter().any(|d| d.code == "PSTDR-E018"),
            "check_path must emit PSTDR-E018: {check_diags:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_chat_allowed_when_beta_api_declared() {
        let dir = temp_dir("beta_chat_workspace");
        let spec = BedrockSpec::bundled();
        init_workspace(&dir, "BetaChatPack", &spec).unwrap();

        fs::write(
            dir.join("src/chat_cmd.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0-beta")

on chat msg by player {
    if msg == "!ping" {
        cancel event
        send "pong" to player
    }
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags), "beta convert_workspace failed: {diags:?}");

        let manifest_raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        assert!(
            manifest_raw.contains("\"2.11.0-beta\""),
            "BP/manifest.json must wire 2.11.0-beta:\n{manifest_raw}"
        );

        let audit_ok = audit_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&audit_ok), "audit_workspace must pass with -beta manifest: {audit_ok:?}");

        // Downgrade BP/manifest.json back to stable 2.11.0 and confirm Audit Gate catches E018
        let downgraded = manifest_raw.replace("\"2.11.0-beta\"", "\"2.11.0\"");
        fs::write(dir.join("BP/manifest.json"), downgraded).unwrap();
        let audit_bad = audit_workspace(&dir, &spec).unwrap();
        assert!(
            audit_bad.iter().any(|d| d.code == "PSTDR-E018"),
            "audit_workspace must emit PSTDR-E018 when manifest uses stable API with chatSend: {audit_bad:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
