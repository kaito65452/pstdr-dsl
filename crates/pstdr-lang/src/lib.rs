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
pub use ast::ApiProfile;
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

        // Also test an unknown alias not in RP or Mojang vanilla catalog
        fs::write(
            dir.join("BP/items/frost_scythe.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "my_pack:frost_scythe" },
    "components": {
      "minecraft:icon": "unknown_mythic_blade"
    }
  }
}"#,
        ).unwrap();
        let diags2 = convert_workspace(&dir, &spec).unwrap();
        assert!(
            diags2.iter().any(|d| d.code == "PSTDR-E017"),
            "unknown_mythic_blade is not a valid vanilla alias, must emit PSTDR-E017: {diags2:?}"
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
        assert!(scythe_js.contains("// @pstdr src/all_weapons.pstdr:"));

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
        // exactly once (no duplicate .js + .ts hits) with preserved camelCase "chatSend"
        let downgraded = manifest_raw.replace("\"2.11.0-beta\"", "\"2.11.0\"");
        fs::write(dir.join("BP/manifest.json"), downgraded).unwrap();
        let audit_bad = audit_workspace(&dir, &spec).unwrap();
        let e018s: Vec<_> = audit_bad.iter().filter(|d| d.code == "PSTDR-E018").collect();
        assert_eq!(
            e018s.len(),
            1,
            "audit_workspace must report PSTDR-E018 exactly once (no .ts duplicate): {audit_bad:?}"
        );
        assert!(
            e018s[0].message.contains("'chatSend'"),
            "audit_workspace must preserve exact 'chatSend' casing, got: {}",
            e018s[0].message
        );
        assert!(
            !e018s[0].file.ends_with(".ts"),
            "audit_workspace must not report duplicate .ts mirror file: {:?}",
            e018s[0]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsafe_beta_event_rejected_immediately_in_check_with_exact_casing() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

unsafe hook_chat() {
    world.beforeEvents.chatSend.subscribe((ev) => {});
}

on join of player {
    hook_chat()
}
"#;
        let res = compile_source(src, "unsafe_chat.pstdr", None, &spec, true, true, vec![]);
        assert!(res.js.is_none(), "stable API + unsafe chatSend must emit no JS");
        let e018 = res
            .diagnostics
            .iter()
            .find(|d| d.code == "PSTDR-E018")
            .expect("Expected PSTDR-E018 on unsafe chatSend in stable API");
        assert!(
            e018.message.contains("'chatSend'"),
            "Expected exact 'chatSend' casing in diagnostic, got: {}",
            e018.message
        );

        let dir = temp_dir("unsafe_chat_check");
        let file = dir.join("unsafe_chat.pstdr");
        fs::write(&file, src).unwrap();
        let check_diags = check_path(&file, None, &spec).unwrap();
        assert!(
            check_diags
                .iter()
                .any(|d| d.code == "PSTDR-E018" && d.message.contains("'chatSend'")),
            "check_path must emit PSTDR-E018 with exact 'chatSend' casing: {check_diags:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bulk_beta_apis_detected_in_check_and_audit_workspace_on_stable_manifest() {
        let dir = temp_dir("bulk_beta_apis_audit");
        let spec = BedrockSpec::from_json_str(
            r#"{
  "bedrock_version": "26.60",
  "script_api_version": "2.11.0",
  "min_engine_version": [1, 26, 60],
  "beta_events": ["chatSend"],
  "beta_apis": ["World.betaOnlyFeature", "System.experimentalTick"],
  "effects": ["speed"],
  "sounds": ["random.orb"],
  "vanilla_items": ["acacia_boat"],
  "vanilla_blocks": ["amethyst_block"]
}"#,
            "test:bulk_beta",
        )
        .unwrap();

        assert!(spec.is_beta_api("World.betaOnlyFeature"));
        assert!(!spec.is_beta_api("World.getAllPlayers"));

        init_workspace(&dir, "BulkBetaPack", &spec).unwrap();

        // 1. On stable 2.11.0, check / convert immediately flags `unsafe` Beta API with exact casing
        let stable_src = r#"@addon(target = "26.60", api = "2.11.0")

unsafe call_beta_api() {
    return world.betaOnlyFeature();
}

on join of player {
    call_beta_api()
}
"#;
        fs::write(dir.join("src/beta_check.pstdr"), stable_src).unwrap();
        let stable_conv = convert_workspace(&dir, &spec).unwrap();
        assert!(
            stable_conv
                .iter()
                .any(|d| d.code == "PSTDR-E018" && d.message.contains("'World.betaOnlyFeature'")),
            "convert_workspace must immediately catch unsafe Beta API on stable 2.11.0: {stable_conv:?}"
        );

        // 2. Compile with 2.11.0-beta, then downgrade BP/manifest.json to 2.11.0 -> audit_workspace catches E018 once
        fs::write(
            dir.join("src/beta_check.pstdr"),
            stable_src.replace("\"2.11.0\"", "\"2.11.0-beta\""),
        )
        .unwrap();
        let beta_conv = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&beta_conv), "beta convert_workspace failed: {beta_conv:?}");

        let manifest_raw = fs::read_to_string(dir.join("BP/manifest.json")).unwrap();
        fs::write(
            dir.join("BP/manifest.json"),
            manifest_raw.replace("\"2.11.0-beta\"", "\"2.11.0\""),
        )
        .unwrap();

        let audit_diags = audit_workspace(&dir, &spec).unwrap();
        let e018s: Vec<_> = audit_diags.iter().filter(|d| d.code == "PSTDR-E018").collect();
        assert_eq!(
            e018s.len(),
            1,
            "audit_workspace must report PSTDR-E018 exactly once: {audit_diags:?}"
        );
        assert!(
            e018s[0].message.contains("'World.betaOnlyFeature'"),
            "Expected exact 'World.betaOnlyFeature' casing, got: {}",
            e018s[0].message
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn spec_driven_vanilla_items_and_blocks_accepted_by_asset_guard() {
        let dir = temp_dir("spec_vanilla_assets");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("BP/blocks")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // Use official Mojang vanilla item & block IDs populated from bedrock-samples
        fs::write(
            dir.join("BP/items/acacia_relic.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:item": {
    "description": { "identifier": "pack:acacia_relic" },
    "components": {
      "minecraft:icon": "acacia_boat"
    }
  }
}"#,
        )
        .unwrap();
        fs::write(
            dir.join("BP/blocks/crystal_shrine.json"),
            r#"{
  "format_version": "1.21.90",
  "minecraft:block": {
    "description": { "identifier": "pack:crystal_shrine" },
    "components": {
      "minecraft:material_instances": {
        "*": { "texture": "amethyst_block" }
      }
    }
  }
}"#,
        )
        .unwrap();

        let diags = validate_workspace_assets(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags),
            "Official vanilla_items/vanilla_blocks must pass Asset Guard without RP: {diags:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ─── WAVE 3A: .mcfunction Pipeline, command, run_fn, script_event, entity_trigger ───

    #[test]
    fn wave_3a_command_and_run_fn_codegen_and_before_event_deferral() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

mcfunction "arena/reset_wave" {
    // Reset arena mobs
    kill @e[type=zombie]
    say Arena reset!
}

on join of player {
    command "say Welcome!"
    command on player "particle minecraft:totem_particle ~ ~1 ~"
    run_fn "arena/reset_wave"
    run_fn on player "arena/reset_wave"
}

on break of "minecraft:bedrock" by player {
    cancel event
    command "say Protected!"
    command on player "playsound note.bass @s"
    run_fn "arena/reset_wave"
    run_fn on player "arena/reset_wave"
}
"#;
        let res = compile_source(src, "wave3a.pstdr", None, &spec, true, false, vec![]);
        assert!(
            !has_errors(&res.diagnostics),
            "Expected no errors, got: {:?}",
            res.diagnostics
        );
        let js = res.js.unwrap();
        assert!(
            !js.contains("runCommandAsync"),
            "Must NEVER emit deprecated runCommandAsync:\n{js}"
        );
        assert!(
            js.contains("world.getDimension(\"overworld\").runCommand(\"say Welcome!\");"),
            "Missing overworld command:\n{js}"
        );
        assert!(
            js.contains("player.runCommand(\"particle minecraft:totem_particle ~ ~1 ~\");"),
            "Missing targeted command:\n{js}"
        );
        assert!(
            js.contains("world.getDimension(\"overworld\").runCommand(\"function arena/reset_wave\");"),
            "Missing overworld run_fn:\n{js}"
        );
        assert!(
            js.contains("player.runCommand(\"function arena/reset_wave\");"),
            "Missing targeted run_fn:\n{js}"
        );
        // Inside beforeEvents.playerBreakBlock, all command / run_fn calls must be deferred via system.run
        assert!(
            js.contains("system.run(() => {\n        world.getDimension(\"overworld\").runCommand(\"say Protected!\");\n    });"),
            "command inside beforeEvents must be deferred via system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n        player.runCommand(\"playsound note.bass @s\");\n    });"),
            "command on target inside beforeEvents must be deferred via system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n        world.getDimension(\"overworld\").runCommand(\"function arena/reset_wave\");\n    });"),
            "run_fn inside beforeEvents must be deferred via system.run:\n{js}"
        );
        assert!(
            js.contains("system.run(() => {\n        player.runCommand(\"function arena/reset_wave\");\n    });"),
            "run_fn on target inside beforeEvents must be deferred via system.run:\n{js}"
        );
        assert!(
            !js.contains("mcfunction"),
            "Inline mcfunction declarations must not emit raw mcfunction into JS:\n{js}"
        );

        // Formatter round-trip idempotency
        let fmt1 = format_source(src, "wave3a.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "wave3a.pstdr").unwrap();
        assert_eq!(fmt1, fmt2, "Formatter must be idempotent for Wave 3A syntax");
    }

    #[test]
    fn wave_3a_script_event_and_entity_trigger_codegen_and_validation() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

on script_event "my_pack:boss_spawn" {
    const id = event.id
    const msg = event.message
    if source != null {
        command on source "say Triggered {id}: {msg}"
    }
    if source_block != null {
        command "say From block {source_block.type_id}"
    }
}

on entity_trigger "my_pack:enrage" {
    command on entity "say Enraged via {event_id}"
}

on entity_trigger "my_pack:phase_two" for "my_pack:frost_boss" {
    apply strength 2 to entity for 10s
}
"#;
        let res = compile_source(src, "events3a.pstdr", None, &spec, true, false, vec![]);
        assert!(
            !has_errors(&res.diagnostics),
            "Expected no errors, got: {:?}",
            res.diagnostics
        );
        let js = res.js.unwrap();
        assert!(
            js.contains("import { world, system } from \"@minecraft/server\";"),
            "Must import world and system:\n{js}"
        );
        assert!(
            js.contains("system.afterEvents.scriptEventReceive.subscribe((event) => {"),
            "Must subscribe to system.afterEvents.scriptEventReceive:\n{js}"
        );
        assert!(
            js.contains("if (event.id !== \"my_pack:boss_spawn\") return;"),
            "Must filter event.id:\n{js}"
        );
        assert!(
            js.contains("const source = event.sourceEntity;"),
            "Must bind source:\n{js}"
        );
        assert!(
            js.contains("const source_block = event.sourceBlock;"),
            "Must bind source_block:\n{js}"
        );
        assert!(
            js.contains("}, { namespaces: [\"my_pack\"] });"),
            "Must pass namespaces filter option:\n{js}"
        );
        assert!(
            js.contains("world.afterEvents.dataDrivenEntityTrigger.subscribe((event) => {"),
            "Must subscribe to world.afterEvents.dataDrivenEntityTrigger:\n{js}"
        );
        assert!(
            js.contains("const entity = event.entity;"),
            "Must bind entity:\n{js}"
        );
        assert!(
            js.contains("const event_id = event.eventId;"),
            "Must bind event_id:\n{js}"
        );
        assert!(
            js.contains("}, { eventTypes: [\"my_pack:enrage\"] });"),
            "Must pass eventTypes without entityTypes:\n{js}"
        );
        assert!(
            js.contains("}, { eventTypes: [\"my_pack:phase_two\"], entityTypes: [\"my_pack:frost_boss\"] });"),
            "Must pass eventTypes and entityTypes:\n{js}"
        );

        // Invalid script_event without namespace colon must emit a clear error diagnostic
        let bad_se = r#"
on script_event "missing_namespace" {
    command "say bad"
}
"#;
        let bad_res = compile_source(bad_se, "bad_se.pstdr", None, &spec, true, false, vec![]);
        assert!(
            bad_res
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains("namespace")),
            "Expected clear namespace diagnostic for script_event without colon: {:?}",
            bad_res.diagnostics
        );

        // Reserved keywords check (E003)
        for kw in ["command", "run_fn", "mcfunction"] {
            let bad_kw = format!("const {kw} = 1\n");
            let (_p, diags) = parse_source(&bad_kw, "kw.pstdr");
            assert!(
                diags.iter().any(|d| d.code == "PSTDR-E003"),
                "Expected PSTDR-E003 for reserved keyword '{kw}', got: {diags:?}"
            );
        }

        // Formatter idempotency for script_event and entity_trigger
        let fmt1 = format_source(src, "events3a.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "events3a.pstdr").unwrap();
        assert_eq!(fmt1, fmt2);
    }

    #[test]
    fn wave_3a_run_fn_asset_guard_e017_and_workspace_mcfunction_pipeline() {
        let dir = temp_dir("wave3a_ws");
        let spec = BedrockSpec::bundled();
        fs::create_dir_all(dir.join("BP/functions/existing")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        // 1. Existing .mcfunction on disk
        fs::write(
            dir.join("BP/functions/existing/from_disk.mcfunction"),
            "say From disk!\n",
        )
        .unwrap();

        // 2. Missing .mcfunction must emit PSTDR-E017
        let missing_src = r#"
@addon(target = "26.60", api = "2.11.0")
on join of player {
    run_fn "does/not_exist"
}
"#;
        let missing_res = compile_source(
            missing_src,
            "src/missing.pstdr",
            Some(&dir),
            &spec,
            true,
            false,
            vec![],
        );
        assert!(
            missing_res
                .diagnostics
                .iter()
                .any(|d| d.code == "PSTDR-E017"),
            "Missing run_fn target must emit PSTDR-E017: {:?}",
            missing_res.diagnostics
        );

        // 3. Path traversal in mcfunction and run_fn must emit PSTDR-E009
        let trav_src = r#"
mcfunction "../escape" {
    say nope
}
on join of player {
    run_fn "../escape"
}
"#;
        let trav_res = compile_source(
            trav_src,
            "src/trav.pstdr",
            Some(&dir),
            &spec,
            true,
            false,
            vec![],
        );
        assert!(
            trav_res.diagnostics.iter().filter(|d| d.code == "PSTDR-E009").count() >= 2,
            "Path traversal in mcfunction and run_fn must emit PSTDR-E009: {:?}",
            trav_res.diagnostics
        );

        // 4. Cross-file inline mcfunction + disk mcfunction in convert_workspace
        fs::write(
            dir.join("src/funcs.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")

mcfunction "waves/boss/start" {
    // Spawn boss and announce
    
    tellraw @a {"rawtext":[{"text":"Don't blink!"}]} // inline comment
    summon minecraft:zombie ~ ~1 ~
}
"#,
        )
        .unwrap();

        fs::write(
            dir.join("src/main.pstdr"),
            r#"
@addon(target = "26.60", api = "2.11.0")

on script_event "arena:start" {
    run_fn "waves/boss/start"
    run_fn "existing/from_disk"
}
"#,
        )
        .unwrap();

        let diags = convert_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&diags),
            "Valid disk + cross-file inline mcfunction must compile cleanly: {diags:?}"
        );

        let emitted_fn = dir.join("BP/functions/waves/boss/start.mcfunction");
        assert!(
            emitted_fn.is_file(),
            "convert_workspace must emit BP/functions/waves/boss/start.mcfunction"
        );
        let fn_content = fs::read_to_string(&emitted_fn).unwrap();
        assert_eq!(
            fn_content,
            "tellraw @a {\"rawtext\":[{\"text\":\"Don't blink!\"}]}\nsummon minecraft:zombie ~ ~1 ~\n",
            "Emitted .mcfunction must strip // comments and blank lines cleanly"
        );

        let main_js = fs::read_to_string(dir.join("BP/scripts/generated/main.js")).unwrap();
        assert!(main_js.contains("system.afterEvents.scriptEventReceive.subscribe"));
        assert!(main_js.contains("world.getDimension(\"overworld\").runCommand(\"function waves/boss/start\");"));
        assert!(main_js.contains("world.getDimension(\"overworld\").runCommand(\"function existing/from_disk\");"));

        let _ = fs::remove_dir_all(&dir);
    }

    // ─── WAVE 3A-END: DX Hardening, Multi-Error, Contextual Keywords & Hallucination Guards ───

    #[test]
    fn wave_3a_end_contextual_preposition_variables() {
        let spec = BedrockSpec::bundled();
        let src = r#"
@addon(target = "26.60", api = "2.11.0")

const to = "overworld"
const from = "nether"
const of = "minecraft:stone"
const with = "my_pack:blade"

func transfer(from, to, by) {
    mut at = 0
    at += by
    return "{from}->{to}:{at}"
}

on join of player {
    mut at = 1
    mut by = 2
    const label = transfer(from, to, by)
    send "Moved {label} at {at}" to player
}

on hit target with with by player {
    mut by = 5
    damage target by by
}

on break of of by player {
    spawn particle "minecraft:totem_particle" at player
}
"#;
        let res = compile_source(src, "ctx_kw.pstdr", None, &spec, true, true, vec![]);
        assert!(
            !has_errors(&res.diagnostics),
            "Contextual prepositions (to, by, of, at, with, from) must compile cleanly: {:?}",
            res.diagnostics
        );
        let js = res.js.unwrap();
        assert!(js.contains("const to = \"overworld\";"));
        assert!(js.contains("let at = 0;"));
        assert!(js.contains("target.applyDamage(by);"));
        assert!(js.contains("player.sendMessage("));
    }

    #[test]
    fn wave_3a_end_e016_help_and_multi_error_recovery() {
        let spec = BedrockSpec::bundled();
        let src = r#"
on unknown_ev of player {
    send "hi" to player
}

on join of player {
    if ( == ) {
        send "broken" to player
    }
    const x = 1
    x = 2
    apply slownes 0 to player for 3s
}
"#;
        let res = compile_source(src, "multi_dx.pstdr", None, &spec, true, true, vec![]);
        assert!(
            res.diagnostics.len() >= 4,
            "Expected multiple diagnostics across top-level & statements, got: {:?}",
            res.diagnostics
        );
        let e016 = res
            .diagnostics
            .iter()
            .find(|d| d.code == "PSTDR-E016" && d.message.contains("Unknown 'on' event"))
            .expect("Expected PSTDR-E016 for unknown 'on' event");
        for expected in [
            "on chat",
            "on leave",
            "on script_event",
            "on entity_trigger",
            "pstdr-c --version",
        ] {
            assert!(
                e016.solution.contains(expected),
                "PSTDR-E016 solution must mention '{expected}', got: {}",
                e016.solution
            );
        }
        assert!(res.diagnostics.iter().any(|d| d.code == "PSTDR-E002"));
        assert!(res.diagnostics.iter().any(|d| d.code == "PSTDR-E011"));
        assert!(res.diagnostics.iter().any(|d| d.code == "PSTDR-E013"));
    }

    #[test]
    fn wave_3a_end_helper_arity_and_world_system_hallucination_guards() {
        let spec = BedrockSpec::bundled();
        let bad = r#"
@addon(target = "26.60", api = "2.11.0")

on join of player {
    player.title("Wave 1", "Get ready!")
    player.subtitle()
    player.actionbar("A", "B")
    player.give_item()
    player.give_item("minecraft:diamond", 1, "Gem", "extra")
    const t = world.get_time()
    const s = system.nonexistent_timer()
}
"#;
        let res = compile_source(bad, "guards.pstdr", None, &spec, true, true, vec![]);
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.message.contains("title()") && d.solution.contains("subtitle")),
            "2-arg title() must be rejected with subtitle suggestion: {:?}",
            res.diagnostics
        );
        assert!(
            res.diagnostics.iter().any(|d| d.message.contains("subtitle()")),
            "0-arg subtitle() must be rejected: {:?}",
            res.diagnostics
        );
        assert!(
            res.diagnostics.iter().any(|d| d.message.contains("actionbar()")),
            "2-arg actionbar() must be rejected: {:?}",
            res.diagnostics
        );
        assert!(
            res.diagnostics
                .iter()
                .filter(|d| d.message.contains("give_item()"))
                .count()
                == 2,
            "0-arg and 4-arg give_item() must be rejected: {:?}",
            res.diagnostics
        );
        let world_err = res
            .diagnostics
            .iter()
            .find(|d| d.message.contains("world.get_time()"))
            .expect("world.get_time() must be rejected");
        for suggestion in [
            "system.current_tick",
            "world.get_absolute_time()",
            "world.get_time_of_day()",
        ] {
            assert!(
                world_err.solution.contains(suggestion),
                "world.get_time() suggestion must include '{suggestion}', got: {}",
                world_err.solution
            );
        }
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.message.contains("system.nonexistent_timer()")),
            "Unknown system method must be rejected: {:?}",
            res.diagnostics
        );

        // Valid APIs compile cleanly
        let good = r#"
@addon(target = "26.60", api = "2.11.0")

on join of player {
    player.title("Wave 1")
    player.subtitle("Get ready!")
    player.actionbar("§aGo!")
    player.give_item("minecraft:diamond", 2, "§bGem")
    const tick = system.current_tick
    const abs_t = world.get_absolute_time()
    const tod = world.get_time_of_day()
    send "{tick} {abs_t} {tod}" to player
}
"#;
        let res_ok = compile_source(good, "guards_ok.pstdr", None, &spec, true, true, vec![]);
        assert!(
            !has_errors(&res_ok.diagnostics),
            "Valid helper & world/system APIs must pass: {:?}",
            res_ok.diagnostics
        );
        let js = res_ok.js.unwrap();
        assert!(js.contains("system.currentTick"));
        assert!(js.contains("world.getAbsoluteTime()"));
        assert!(js.contains("world.getTimeOfDay()"));
    }

    #[test]
    fn wave_3a_end_snake_case_string_array_helpers_and_trace_comments() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

func normalize(raw) {
    const cleaned = raw.trim().lower()
    const loud = cleaned.upper()
    const parts = loud.split(" ")
    parts.push("END")
    if cleaned.starts_with("!") and cleaned.ends_with("?") and parts.contains("END") {
        return parts.join("-").replace("A", "B")
    }
    return raw.toLowerCase()
}

on join of player {
    const out = normalize(" !hello? ")
    send "{out}" to player
}

every 1s {
    const n = world.players().length
    if n > 0 {
        world.send_message("tick")
    }
}
"#;
        let res = compile_source(src, "src/helpers.pstdr", None, &spec, true, true, vec![]);
        assert!(
            !has_errors(&res.diagnostics),
            "snake_case helpers must compile cleanly: {:?}",
            res.diagnostics
        );
        let js = res.js.unwrap();
        assert!(js.contains("raw.trim().toLowerCase()"), "js:\n{js}");
        assert!(js.contains("cleaned.toUpperCase()"), "js:\n{js}");
        assert!(js.contains("loud.split(\" \")"), "js:\n{js}");
        assert!(js.contains("parts.push(\"END\")"), "js:\n{js}");
        assert!(js.contains("cleaned.startsWith(\"!\")"), "js:\n{js}");
        assert!(js.contains("cleaned.endsWith(\"?\")"), "js:\n{js}");
        assert!(js.contains("parts.includes(\"END\")"), "js:\n{js}");
        assert!(js.contains("parts.join(\"-\").replace(\"A\", \"B\")"), "js:\n{js}");
        assert!(js.contains("raw.toLowerCase()"), "camelCase must still work:\n{js}");

        // Source line trace comments above function, event handler, and top-level timer
        assert!(
            js.contains("// @pstdr src/helpers.pstdr:3"),
            "Expected // @pstdr trace comment above func:\n{js}"
        );
        assert!(
            js.contains("// @pstdr src/helpers.pstdr:14"),
            "Expected // @pstdr trace comment above on join:\n{js}"
        );
        assert!(
            js.contains("// @pstdr src/helpers.pstdr:19"),
            "Expected // @pstdr trace comment above every:\n{js}"
        );
    }

    #[test]
    fn wave_3a_end_audit_workspace_stale_manifest_hint() {
        let dir = temp_dir("stale_manifest");
        let spec = BedrockSpec::bundled();
        init_workspace(&dir, "StalePack", &spec).unwrap();

        // Update .pstdr to 2.11.0-beta without running convert_workspace -> BP/manifest.json stays 2.11.0
        fs::write(
            dir.join("src/welcome.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0-beta")

on join of player {
    send "Hello!" to player
}
"#,
        )
        .unwrap();

        let diags = audit_workspace(&dir, &spec).unwrap();
        let stale = diags
            .iter()
            .find(|d| {
                d.code == "PSTDR-E015"
                    && d.solution.contains(
                        "BP/manifest.json is out of sync with @addon(...) — run `pstdr-c convert --workspace` or `pstdr-c build --workspace` first.",
                    )
            });
        assert!(
            stale.is_some(),
            "audit_workspace must detect stale BP/manifest.json with explicit hint: {diags:?}"
        );

        // After convert_workspace, manifest is synced -> audit_workspace passes
        let conv_diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&conv_diags), "conv_diags: {conv_diags:?}");
        let audit_after = audit_workspace(&dir, &spec).unwrap();
        assert!(
            !has_errors(&audit_after),
            "audit_workspace must pass after syncing manifest: {audit_after:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ─── WAVE 3B: Custom Commands, Native UI, Scoreboard & 10 Survival Events ───

    #[test]
    fn wave_3b_on_cmd_codegen_validation_and_merged_startup() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on cmd "my_pack:guess" description "Submit a block guess" permission any (word: string, tries?: int, hard?: bool) by player {
    send "You guessed: {word}" to player
}

on cmd "my_pack:tp" description "Teleport helper" permission admin (target: player, height: float) {
    world.send_message("Teleporting...")
}
"#;
        let res = compile_source(src, "src/cmds.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { world, system, CommandPermissionLevel, CustomCommandParamType } from \"@minecraft/server\";"), "js:\n{js}");
        assert_eq!(js.matches("system.beforeEvents.startup.subscribe").count(), 1, "Must emit single startup subscriber:\n{js}");
        assert!(js.contains("// @pstdr src/cmds.pstdr:3"), "js:\n{js}");
        assert!(js.contains("initEv.customCommandRegistry.registerCommand({"), "js:\n{js}");
        assert!(js.contains("name: \"my_pack:guess\""), "js:\n{js}");
        assert!(js.contains("description: \"Submit a block guess\""), "js:\n{js}");
        assert!(js.contains("permissionLevel: CommandPermissionLevel.Any"), "js:\n{js}");
        assert!(js.contains("{ name: \"word\", type: CustomCommandParamType.String }"), "js:\n{js}");
        assert!(js.contains("{ name: \"tries\", type: CustomCommandParamType.Integer }"), "js:\n{js}");
        assert!(js.contains("{ name: \"hard\", type: CustomCommandParamType.Boolean }"), "js:\n{js}");
        assert!(js.contains("const player = origin.sourceEntity;"), "js:\n{js}");
        assert!(js.contains("if (!player || player.typeId !== \"minecraft:player\") return;"), "js:\n{js}");
        assert!(js.contains("system.run(() => {"), "js:\n{js}");
        assert!(js.contains("permissionLevel: CommandPermissionLevel.Admin"), "js:\n{js}");
        assert!(js.contains("{ name: \"target\", type: CustomCommandParamType.PlayerSelector }"), "js:\n{js}");
        assert!(js.contains("{ name: \"height\", type: CustomCommandParamType.Float }"), "js:\n{js}");

        // Formatter idempotency
        let fmt1 = format_source(src, "src/cmds.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "src/cmds.pstdr").unwrap();
        assert_eq!(fmt1, fmt2);

        // Negative validation: missing namespace colon, duplicate param, required after optional
        let bad = r#"
on cmd "nocolon" description "Bad" (a: string, a: int) {
    world.send_message("x")
}
on cmd "ns:order" description "Bad order" (opt?: bool, req: string) {
    world.send_message("y")
}
"#;
        let bad_res = compile_source(bad, "bad_cmd.pstdr", None, &spec, true, true, vec![]);
        let msgs: Vec<&str> = bad_res.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("namespace colon")), "msgs: {msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("Duplicate command parameter")), "msgs: {msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("cannot appear after an optional")), "msgs: {msgs:?}");

        // Workspace merging: custom item component + on cmd share ONE startup subscriber in _registry.js
        let dir = temp_dir("cmd_merge");
        fs::create_dir_all(dir.join("BP/items")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("BP/items/wand.json"),
            r#"{"format_version":"1.21.90","minecraft:item":{"description":{"identifier":"my_pack:wand"},"components":{"minecraft:icon":"stick"}}}"#,
        ).unwrap();
        fs::write(
            dir.join("src/addon.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")
on right click with "my_pack:wand" by player {
    send "Zap!" to player
}
on cmd "my_pack:zap" description "Zap command" by player {
    send "Slash zap!" to player
}
"#,
        ).unwrap();
        let ws_diags = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&ws_diags), "ws_diags: {ws_diags:?}");
        let reg_js = fs::read_to_string(dir.join("BP/scripts/generated/_registry.js")).unwrap();
        assert_eq!(reg_js.matches("system.beforeEvents.startup.subscribe").count(), 1, "reg_js:\n{reg_js}");
        assert!(reg_js.contains("initEv.itemComponentRegistry.registerCustomComponent"), "reg_js:\n{reg_js}");
        assert!(reg_js.contains("cmd_addon(initEv);"), "reg_js:\n{reg_js}");
        let cmd_js = fs::read_to_string(dir.join("BP/scripts/generated/commands/addon.js")).unwrap();
        assert!(cmd_js.contains("export function registerCommands(initEv)"), "cmd_js:\n{cmd_js}");
        assert!(cmd_js.contains("initEv.customCommandRegistry.registerCommand"), "cmd_js:\n{cmd_js}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wave_3b_ui_forms_codegen_asset_guard_and_conditional_manifest() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on cmd "my_pack:menu" description "Open menus" by player {
    ui action "Class Menu" body "Choose your role:" to player {
        button "Warrior" icon "textures/items/iron_sword" {
            player.give_item("minecraft:iron_sword", 1)
        }
        button "Archer" {
            player.give_item("minecraft:bow", 1)
        }
    }
    ui message "Confirm" body "Proceed?" to player {
        button1 "Yes" {
            send "Accepted" to player
        }
        button2 "No" {
            send "Declined" to player
        }
    }
    ui modal "Settings" to player {
        text_field "Nickname" placeholder "Enter..." default "Steve" as nick
        toggle "Hardcore" default false as hard
        slider "Difficulty" min 1 max 5 step 1 default 2 as diff
        dropdown "Role" options ["Warrior", "Archer"] default 0 as role
        on submit {
            send "{nick}:{hard}:{diff}:{role}" to player
        }
    }
}

on break of "minecraft:bedrock" by player {
    cancel event
    ui message "Protected" body "Cannot break!" to player {
        button1 "OK" {
            send "Understood" to player
        }
        button2 "Close" {
            send "Closed" to player
        }
    }
}
"#;
        let res = compile_source(src, "src/ui.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { ActionFormData, MessageFormData, ModalFormData } from \"@minecraft/server-ui\";"), "js:\n{js}");
        assert!(js.contains("new ActionFormData()"), "js:\n{js}");
        assert!(js.contains(".button(\"Warrior\", \"textures/items/iron_sword\")"), "js:\n{js}");
        assert!(js.contains("new MessageFormData()"), "js:\n{js}");
        assert!(js.contains(".button1(\"Yes\")"), "js:\n{js}");
        assert!(js.contains(".button2(\"No\")"), "js:\n{js}");
        assert!(js.contains("new ModalFormData()"), "js:\n{js}");
        assert!(js.contains(".textField(\"Nickname\", \"Enter...\", { defaultValue: \"Steve\" })"), "js:\n{js}");
        assert!(js.contains(".toggle(\"Hardcore\", { defaultValue: false })"), "js:\n{js}");
        assert!(js.contains(".slider(\"Difficulty\", 1, 5, { valueStep: 1, defaultValue: 2 })"), "js:\n{js}");
        assert!(js.contains(".dropdown(\"Role\", [\"Warrior\", \"Archer\"], { defaultValueIndex: 0 })"), "js:\n{js}");
        assert!(js.contains("if (res.canceled) return;"), "js:\n{js}");
        assert!(js.contains("if (res.canceled || !res.formValues) return;"), "js:\n{js}");
        assert!(js.contains("const [nick, hard, diff, role] = res.formValues;"), "js:\n{js}");
        // Before-event deferral for ui message inside cancellable on break
        assert!(js.contains("system.run(() => {\n            __form.show(player).then"), "js:\n{js}");
        // Source trace comments
        assert!(js.contains("// @pstdr src/ui.pstdr:4"), "js:\n{js}");
        assert!(js.contains("// @pstdr src/ui.pstdr:12"), "js:\n{js}");
        assert!(js.contains("// @pstdr src/ui.pstdr:20"), "js:\n{js}");

        // Formatter idempotency
        let fmt1 = format_source(src, "src/ui.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "src/ui.pstdr").unwrap();
        assert_eq!(fmt1, fmt2);

        // Icon asset guard: custom missing icon -> E017; path traversal -> E009
        let bad_icons = r#"
on join of player {
    ui action "Bad" to player {
        button "Missing" icon "textures/items/custom_nonexistent_blade" {
            send "a" to player
        }
        button "Traversal" icon "../secret.png" {
            send "b" to player
        }
    }
}
"#;
        let bad_res = compile_source(bad_icons, "bad_ui.pstdr", None, &spec, true, true, vec![]);
        let codes: Vec<&str> = bad_res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"PSTDR-E017"), "Expected E017 for missing custom icon: {codes:?}");
        assert!(codes.contains(&"PSTDR-E009"), "Expected E009 for icon path traversal: {codes:?}");

        // Workspace manifest conditional injection: WITH ui -> injects @minecraft/server-ui 2.0.0
        let dir_with_ui = temp_dir("manifest_with_ui");
        fs::create_dir_all(dir_with_ui.join("src")).unwrap();
        fs::write(dir_with_ui.join("src/menu.pstdr"), src).unwrap();
        let diags_ui = convert_workspace(&dir_with_ui, &spec).unwrap();
        assert!(!has_errors(&diags_ui), "diags_ui: {diags_ui:?}");
        let manifest_ui: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir_with_ui.join("BP/manifest.json")).unwrap(),
        ).unwrap();
        let deps_ui = manifest_ui["dependencies"].as_array().unwrap();
        assert!(
            deps_ui.iter().any(|d| d["module_name"] == "@minecraft/server-ui" && d["version"] == "2.0.0"),
            "Expected @minecraft/server-ui 2.0.0 in manifest: {deps_ui:?}"
        );
        let _ = fs::remove_dir_all(&dir_with_ui);

        // Negative test: WITHOUT ui -> @minecraft/server-ui must NOT be injected
        let dir_no_ui = temp_dir("manifest_no_ui");
        fs::create_dir_all(dir_no_ui.join("src")).unwrap();
        fs::write(
            dir_no_ui.join("src/plain.pstdr"),
            "@addon(target = \"26.60\", api = \"2.11.0\")\non join of player {\n    send \"hi\" to player\n}\n",
        ).unwrap();
        let diags_plain = convert_workspace(&dir_no_ui, &spec).unwrap();
        assert!(!has_errors(&diags_plain), "diags_plain: {diags_plain:?}");
        let manifest_plain: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir_no_ui.join("BP/manifest.json")).unwrap(),
        ).unwrap();
        let deps_plain = manifest_plain["dependencies"].as_array().unwrap();
        assert!(
            !deps_plain.iter().any(|d| d["module_name"] == "@minecraft/server-ui"),
            "@minecraft/server-ui must NOT be injected when no ui block exists: {deps_plain:?}"
        );
        let _ = fs::remove_dir_all(&dir_no_ui);
    }

    #[test]
    fn wave_3b_native_scoreboard_indexing_and_display_slots() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on join of player {
    score["kills"][player] = 0
    score["kills"][player] += 1
    score["kills"][player] -= 1
    const k = score["kills"][player]
    send "Kills: {k}" to player
    scoreboard show "kills" title "Top Kills" slot sidebar
    scoreboard show "deaths" slot list
    scoreboard hide slot belowname
}
"#;
        let res = compile_source(src, "src/sb.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("import { world, DisplaySlotId } from \"@minecraft/server\";"), "js:\n{js}");
        assert_eq!(js.matches("function __pstdr_get_or_create_objective").count(), 1, "Helper must be deduplicated:\n{js}");
        assert!(js.contains("__pstdr_get_or_create_objective(\"kills\").setScore(player, 0);"), "js:\n{js}");
        assert!(js.contains("__pstdr_get_or_create_objective(\"kills\").addScore(player, 1);"), "js:\n{js}");
        assert!(js.contains("__pstdr_get_or_create_objective(\"kills\").addScore(player, -(1));"), "js:\n{js}");
        assert!(js.contains("const k = (__pstdr_get_or_create_objective(\"kills\").getScore(player) ?? 0);"), "js:\n{js}");
        assert!(js.contains("world.scoreboard.setObjectiveAtDisplaySlot(DisplaySlotId.Sidebar, { objective: world.scoreboard.getObjective(\"kills\") ?? world.scoreboard.addObjective(\"kills\", \"Top Kills\") });"), "js:\n{js}");
        assert!(js.contains("world.scoreboard.setObjectiveAtDisplaySlot(DisplaySlotId.List, { objective: __pstdr_get_or_create_objective(\"deaths\") });"), "js:\n{js}");
        assert!(js.contains("world.scoreboard.clearObjectiveAtDisplaySlot(DisplaySlotId.BelowName);"), "js:\n{js}");

        let fmt1 = format_source(src, "src/sb.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "src/sb.pstdr").unwrap();
        assert_eq!(fmt1, fmt2);
    }

    #[test]
    fn wave_3b_ten_survival_events_and_before_event_routing() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0-beta")

on place of block by player {
    send "Placed {block.type_id}" to player
}

on place of block by player {
    cancel event
    send "Blocked {block.type_id}" to player
}

on dimension_change of player {
    send "Moved from {from_dimension.id} to {to_dimension.id} ({from_location.x} -> {to_location.x})" to player
}

on health_change of entity {
    world.send_message("{entity.type_id}: {old_value} -> {new_value}")
}

on interact_entity of target by player {
    cancel event
    send "Locked {target.type_id}" to player
}

on projectile_hit_block of block by shooter {
    world.send_message("{projectile.type_id} hit {block.type_id} from {shooter.type_id}")
}

on entity_hit_block of block by attacker {
    world.send_message("{attacker.type_id} punched {block.type_id}")
}

on item_release with "minecraft:bow" by player {
    send "Released {item.type_id} after {use_duration}" to player
}

on explosion {
    cancel event
    world.send_message("Cancelled explosion from {source} affecting {impacted_blocks.length}")
}

on entity_load of entity {
    world.send_message("Loaded {entity.type_id}")
}

on entity_remove of entity {
    world.send_message("Removed {entity.type_id}")
}
"#;
        let res = compile_source(src, "src/survival.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res.diagnostics), "diags: {:?}", res.diagnostics);
        let js = res.js.unwrap();
        assert!(js.contains("world.afterEvents.playerPlaceBlock.subscribe"), "js:\n{js}");
        assert!(js.contains("world.beforeEvents.playerPlaceBlock.subscribe"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.playerDimensionChange.subscribe"), "js:\n{js}");
        assert!(js.contains("const from_dimension = ev.fromDimension;"), "js:\n{js}");
        assert!(js.contains("const to_dimension = ev.toDimension;"), "js:\n{js}");
        assert!(js.contains("const from_location = ev.fromLocation;"), "js:\n{js}");
        assert!(js.contains("const to_location = ev.toLocation;"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.entityHealthChanged.subscribe"), "js:\n{js}");
        assert!(js.contains("const old_value = ev.oldValue;"), "js:\n{js}");
        assert!(js.contains("const new_value = ev.newValue;"), "js:\n{js}");
        assert!(js.contains("world.beforeEvents.playerInteractWithEntity.subscribe"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.projectileHitBlock.subscribe"), "js:\n{js}");
        assert!(js.contains("const block = ev.getBlockHit()?.block;"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.entityHitBlock.subscribe"), "js:\n{js}");
        assert!(js.contains("const block = ev.hitBlock;"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.itemReleaseUse.subscribe"), "js:\n{js}");
        assert!(js.contains("const use_duration = ev.useDuration;"), "js:\n{js}");
        assert!(js.contains("world.beforeEvents.explosion.subscribe"), "js:\n{js}");
        assert!(js.contains("const impacted_blocks = ev.getImpactedBlocks();"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.entityLoad.subscribe"), "js:\n{js}");
        assert!(js.contains("world.afterEvents.entityRemove.subscribe"), "js:\n{js}");
        // Verify before-event deferral inside cancelled handlers
        assert!(js.contains("ev.cancel = true;\n    system.run(() => {"), "js:\n{js}");

        // Formatter idempotency
        let fmt1 = format_source(src, "src/survival.pstdr").unwrap();
        let fmt2 = format_source(&fmt1, "src/survival.pstdr").unwrap();
        assert_eq!(fmt1, fmt2);

        // E016 explain text lists on cmd and new events
        let e016 = explain_code("E016").unwrap();
        for kw in ["on cmd", "on place", "on dimension_change", "on explosion", "on entity_remove"] {
            assert!(e016.contains(kw), "E016 explain missing '{kw}': {e016}");
        }
    }

    #[test]
    fn wave_3b_contextual_keywords_never_break_user_variables() {
        let spec = BedrockSpec::bundled();
        let src = r#"@addon(target = "26.60", api = "2.11.0")

on join of player {
    const title = "Hero"
    const body = "Welcome"
    const icon = "star"
    const slot = 1
    const min = 0
    const max = 10
    const step = 1
    const options = ["a", "b"]
    const cmd = "go"
    const description = "desc"
    const permission = "any"
    const placeholder = "ph"
    const default = "def"
    const submit = true
    const show = true
    const hide = false
    const button = "ok"
    const button1 = "yes"
    const button2 = "no"
    const text_field = "tf"
    const toggle = false
    const slider = 5
    const dropdown = 0
    send "{title}{body}{icon}{slot}{min}{max}{step}{options.length}{cmd}{description}{permission}{placeholder}{default}{submit}{show}{hide}{button}{button1}{button2}{text_field}{toggle}{slider}{dropdown}" to player
}
"#;
        let res = compile_source(src, "ctx.pstdr", None, &spec, true, true, vec![]);
        assert!(
            !has_errors(&res.diagnostics),
            "Contextual keywords must be usable as variables: {:?}",
            res.diagnostics
        );
    }

    // ─── WAVE 3C: Multi-Target ApiProfile, Spec Resync & Doc-Snippet Verification ───

    #[test]
    fn wave_3c_api_profile_modern_2x_vs_legacy_1x_codegen_and_validation() {
        let spec = BedrockSpec::bundled();

        // 1. Profile resolution and < 1.15.0 rejection
        assert_eq!(ApiProfile::from_api_version("2.11.0"), Ok(ApiProfile::Modern2x));
        assert_eq!(ApiProfile::from_api_version("2.11.0-beta"), Ok(ApiProfile::Modern2x));
        assert_eq!(ApiProfile::from_api_version("1.15.0"), Ok(ApiProfile::Legacy1x));
        assert_eq!(ApiProfile::from_api_version("1.16.0"), Ok(ApiProfile::Legacy1x));
        assert_eq!(ApiProfile::from_api_version("1.19.0-beta"), Ok(ApiProfile::Legacy1x));
        let err_old = ApiProfile::from_api_version("1.14.0").unwrap_err();
        assert!(err_old.contains("1.15.0") && err_old.contains("2.11.0"), "err_old: {err_old}");

        let old_src = "@addon(target = \"1.21.20\", api = \"1.14.0\")\non join of player {\n    send \"hi\" to player\n}\n";
        let old_res = compile_source(old_src, "old.pstdr", None, &spec, true, true, vec![]);
        assert!(
            old_res.diagnostics.iter().any(|d| d.code == "PSTDR-E016" && d.message.contains("1.15.0")),
            "Expected < 1.15.0 diagnostic: {:?}",
            old_res.diagnostics
        );

        // 2. Modern 2.x vs Legacy 1.x command / run_fn codegen (runCommand vs runCommandAsync)
        let cmd_v2 = r#"@addon(target = "26.60", api = "2.11.0")
mcfunction "test/ping" {
    say pong
}
on join of player {
    command on player "say hi"
    run_fn on player "test/ping"
}
"#;
        let res_v2 = compile_source(cmd_v2, "v2_cmd.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res_v2.diagnostics), "v2 diags: {:?}", res_v2.diagnostics);
        let js_v2 = res_v2.js.unwrap();
        assert!(js_v2.contains("player.runCommand(\"say hi\");"), "js_v2:\n{js_v2}");
        assert!(js_v2.contains("player.runCommand(\"function test/ping\");"), "js_v2:\n{js_v2}");
        assert!(!js_v2.contains("runCommandAsync"), "js_v2:\n{js_v2}");

        let cmd_v1 = r#"@addon(target = "1.21.50", api = "1.16.0")
mcfunction "test/ping" {
    say pong
}
on join of player {
    command on player "say hi"
    run_fn on player "test/ping"
}
"#;
        let res_v1 = compile_source(cmd_v1, "v1_cmd.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&res_v1.diagnostics), "v1 diags: {:?}", res_v1.diagnostics);
        let js_v1 = res_v1.js.unwrap();
        assert!(js_v1.contains("player.runCommandAsync(\"say hi\");"), "js_v1:\n{js_v1}");
        assert!(js_v1.contains("player.runCommandAsync(\"function test/ping\");"), "js_v1:\n{js_v1}");

        // 3. on chat is Stable on Legacy 1.x (no E018), Beta-only on Modern 2.x
        let chat_v1 = r#"@addon(target = "1.21.50", api = "1.16.0")
on chat msg by player {
    cancel event
    send "Echo: {msg}" to player
}
"#;
        let chat_v1_res = compile_source(chat_v1, "chat_v1.pstdr", None, &spec, true, true, vec![]);
        assert!(!has_errors(&chat_v1_res.diagnostics), "Legacy 1.x on chat must pass: {:?}", chat_v1_res.diagnostics);

        let chat_v2 = r#"@addon(target = "26.60", api = "2.11.0")
on chat msg by player {
    send "Echo: {msg}" to player
}
"#;
        let chat_v2_res = compile_source(chat_v2, "chat_v2.pstdr", None, &spec, true, true, vec![]);
        assert!(
            chat_v2_res.diagnostics.iter().any(|d| d.code == "PSTDR-E018"),
            "Modern 2.x stable on chat must emit E018: {:?}",
            chat_v2_res.diagnostics
        );

        // 4. on cmd rejected on Legacy 1.x
        let on_cmd_v1 = r#"@addon(target = "1.21.50", api = "1.16.0")
on cmd "my_pack:ping" description "Ping" by player {
    send "pong" to player
}
"#;
        let on_cmd_v1_res = compile_source(on_cmd_v1, "cmd_v1.pstdr", None, &spec, true, true, vec![]);
        assert!(
            on_cmd_v1_res.diagnostics.iter().any(|d| d.code == "PSTDR-E016" && d.message.contains("on cmd")),
            "Legacy 1.x must reject on cmd: {:?}",
            on_cmd_v1_res.diagnostics
        );

        // 5. runCommandAsync rejected on Modern 2.x (safe and unsafe)
        let bad_async_v2 = r#"@addon(target = "26.60", api = "2.11.0")
unsafe raw_cmd(p) {
    p.runCommandAsync("say bad");
}
"#;
        let bad_async_res = compile_source(bad_async_v2, "bad_async.pstdr", None, &spec, true, true, vec![]);
        assert!(
            bad_async_res.diagnostics.iter().any(|d| d.code == "PSTDR-E016" && d.message.contains("runCommandAsync")),
            "Modern 2.x must reject runCommandAsync: {:?}",
            bad_async_res.diagnostics
        );

        // 6. Legacy 1.x workspace: conditional @minecraft/server-ui "1.3.0" + worldInitialize + audit passes for Stable on chat
        let dir_v1 = temp_dir("legacy_1x_ws");
        fs::create_dir_all(dir_v1.join("src")).unwrap();
        fs::write(
            dir_v1.join("src/legacy_ui.pstdr"),
            r#"@addon(target = "1.21.50", api = "1.16.0")

on chat msg by player {
    if msg == "!menu" {
        cancel event
        ui action "Legacy Menu" to player {
            button "Ping" {
                command on player "say pong"
            }
        }
    }
}
"#,
        ).unwrap();
        let diags_v1 = convert_workspace(&dir_v1, &spec).unwrap();
        assert!(!has_errors(&diags_v1), "diags_v1: {diags_v1:?}");
        let manifest_v1: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir_v1.join("BP/manifest.json")).unwrap(),
        ).unwrap();
        let deps_v1 = manifest_v1["dependencies"].as_array().unwrap();
        assert!(
            deps_v1.iter().any(|d| d["module_name"] == "@minecraft/server" && d["version"] == "1.16.0"),
            "Expected @minecraft/server 1.16.0: {deps_v1:?}"
        );
        assert!(
            deps_v1.iter().any(|d| d["module_name"] == "@minecraft/server-ui" && d["version"] == "1.3.0"),
            "Expected @minecraft/server-ui 1.3.0 for Legacy 1.x: {deps_v1:?}"
        );
        let audit_v1 = audit_workspace(&dir_v1, &spec).unwrap();
        assert!(
            !has_errors(&audit_v1),
            "Legacy 1.x stable on chat must pass audit_workspace: {audit_v1:?}"
        );
        let _ = fs::remove_dir_all(&dir_v1);
    }

    #[test]
    fn wave_3c_resynced_bedrock_spec_validation() {
        let modern = BedrockSpec::bundled();
        assert_eq!(modern.minecraft_version, "26.60");
        assert_eq!(modern.script_api_version, "2.11.0");
        assert_eq!(modern.server_ui_api_version, "2.0.0");
        assert!(!modern.spec_version.is_empty());
        for snd in [
            "random.levelup",
            "random.orb",
            "random.toast",
            "note.pling",
            "mob.wither.hurt",
            "mob.enderdragon.growl",
        ] {
            assert!(modern.is_valid_sound(snd), "Missing required vanilla sound '{snd}'");
        }
        assert!(modern.has_server_symbol("CustomCommandRegistry", "registerCommand"));
        assert!(modern.has_server_symbol("CustomCommandParamType", "PlayerSelector"));
        assert!(modern.has_server_symbol("CommandPermissionLevel", "Any"));
        assert!(modern.has_server_symbol("DisplaySlotId", "Sidebar"));
        assert!(modern.has_server_ui_class("ActionFormData"));
        assert!(modern.has_server_ui_class("ModalFormData"));
        assert!(modern.has_server_ui_class("MessageFormData"));
        assert!(modern.is_deprecated_api("runCommandAsync"));
        assert!(!modern.is_valid_world_method("get_time"));

        let legacy = BedrockSpec::for_profile(ApiProfile::Legacy1x);
        assert_eq!(legacy.minecraft_version, "1.21.50");
        assert_eq!(legacy.script_api_version, "1.16.0");
        assert_eq!(legacy.server_ui_api_version, "1.3.0");
        assert!(!legacy.is_beta_event("chatSend"));
        assert!(!legacy.is_deprecated_api("runCommandAsync"));
        assert!(legacy.is_valid_sound("random.levelup"));
        assert!(legacy.has_server_symbol("Player", "runCommandAsync"));
        assert!(legacy.has_server_ui_class("ActionFormData"));
    }

    #[test]
    fn wave_3c_doc_snippets_compile_and_validate() {
        let spec = BedrockSpec::bundled();
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

        fn extract_pstdr_blocks(md: &str) -> Vec<String> {
            let mut blocks = Vec::new();
            let mut in_block = false;
            let mut buf = String::new();
            for line in md.lines() {
                if !in_block && line.trim() == "```pstdr" {
                    in_block = true;
                    buf.clear();
                } else if in_block && line.trim() == "```" {
                    in_block = false;
                    blocks.push(buf.clone());
                } else if in_block {
                    buf.push_str(line);
                    buf.push('\n');
                }
            }
            blocks
        }

        // Validate all complete standalone .pstdr snippets in README.md (including Stable 2.11.0 on cmd + ui action + scoreboard)
        let readme = fs::read_to_string(repo_root.join("README.md")).unwrap();
        let readme_blocks = extract_pstdr_blocks(&readme);
        assert!(
            readme_blocks.len() >= 3,
            "Expected at least 3 .pstdr snippets in README.md, found {}",
            readme_blocks.len()
        );
        for (idx, snippet) in readme_blocks.iter().enumerate() {
            let res = compile_source(
                snippet,
                &format!("README.md#snippet_{idx}"),
                None,
                &spec,
                true,
                true,
                vec![],
            );
            assert!(
                !has_errors(&res.diagnostics),
                "README.md snippet #{idx} failed compilation:\n{snippet}\ndiags: {:?}",
                res.diagnostics
            );
        }

        // Validate the complete Stable 2.11.0 example in llms.txt (contains on cmd + ui action + score + state)
        let llms = fs::read_to_string(repo_root.join("llms.txt")).unwrap();
        let llms_blocks = extract_pstdr_blocks(&llms);
        let full_llms_example = llms_blocks
            .iter()
            .find(|b| b.contains("on cmd"))
            .expect("llms.txt must contain a complete 'on cmd' example");
        let llms_res = compile_source(
            full_llms_example,
            "llms.txt#example",
            None,
            &spec,
            true,
            true,
            vec![],
        );
        assert!(
            !has_errors(&llms_res.diagnostics),
            "llms.txt example failed compilation:\n{full_llms_example}\ndiags: {:?}",
            llms_res.diagnostics
        );

        // Validate a representative Legacy 1.x LTS snippet
        let legacy_snippet = r#"@addon(target = "1.21.50", api = "1.16.0")

on chat msg by player {
    if msg.lower().starts_with("!heal") {
        cancel event
        apply instant_health 1 to player for 1s
        command on player "playsound random.levelup @s"
    }
}
"#;
        let leg_res = compile_source(
            legacy_snippet,
            "docs/legacy_1x.pstdr",
            None,
            &spec,
            true,
            true,
            vec![],
        );
        assert!(
            !has_errors(&leg_res.diagnostics),
            "Legacy 1.x snippet failed compilation: {:?}",
            leg_res.diagnostics
        );
        assert!(leg_res.js.unwrap().contains("player.runCommandAsync"));
    }

    #[test]
    fn qa_on_place_bus_scoped_beta_guard() {
        let spec = BedrockSpec::bundled();

        // 1. `on place` WITHOUT `cancel event` (afterEvents.playerPlaceBlock) passes on Stable 2.11.0
        let stable_v2_dir = temp_dir("on_place_stable_v2");
        fs::create_dir_all(stable_v2_dir.join("src")).unwrap();
        fs::write(
            stable_v2_dir.join("src/place.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0")

on place of block by player {
    send "Placed {block.type_id}" to player
}
"#,
        )
        .unwrap();
        let check_v2 = check_path(&stable_v2_dir.join("src"), Some(&stable_v2_dir), &spec).unwrap();
        assert!(!has_errors(&check_v2), "Stable 2.x on place (after) must pass check: {check_v2:?}");
        let conv_v2 = convert_workspace(&stable_v2_dir, &spec).unwrap();
        assert!(!has_errors(&conv_v2), "Stable 2.x on place (after) must pass convert: {conv_v2:?}");
        let audit_v2 = audit_workspace(&stable_v2_dir, &spec).unwrap();
        assert!(!has_errors(&audit_v2), "Stable 2.x on place (after) must pass audit: {audit_v2:?}");
        let _ = fs::remove_dir_all(&stable_v2_dir);

        // 2. `on place` WITHOUT `cancel event` passes on Legacy 1.x Stable
        let stable_v1_dir = temp_dir("on_place_stable_v1");
        fs::create_dir_all(stable_v1_dir.join("src")).unwrap();
        fs::write(
            stable_v1_dir.join("src/place.pstdr"),
            r#"@addon(target = "1.21.50", api = "1.16.0")

on place of block by player {
    send "Placed {block.type_id}" to player
}
"#,
        )
        .unwrap();
        let conv_v1 = convert_workspace(&stable_v1_dir, &spec).unwrap();
        assert!(!has_errors(&conv_v1), "Stable 1.x on place (after) must pass convert: {conv_v1:?}");
        let audit_v1 = audit_workspace(&stable_v1_dir, &spec).unwrap();
        assert!(!has_errors(&audit_v1), "Stable 1.x on place (after) must pass audit: {audit_v1:?}");
        let _ = fs::remove_dir_all(&stable_v1_dir);

        // 3. `on place` WITH `cancel event` (beforeEvents.playerPlaceBlock) fails with E018 on Stable 2.11.0
        let cancel_stable = r#"@addon(target = "26.60", api = "2.11.0")

on place of block by player {
    cancel event
    send "Blocked {block.type_id}" to player
}
"#;
        let cancel_stable_res =
            compile_source(cancel_stable, "cancel_place.pstdr", None, &spec, true, true, vec![]);
        assert!(
            cancel_stable_res.diagnostics.iter().any(|d| d.code == "PSTDR-E018"),
            "Stable on place with cancel event must emit PSTDR-E018: {:?}",
            cancel_stable_res.diagnostics
        );

        // 4. `on place` WITH `cancel event` passes on `2.11.0-beta`
        let beta_dir = temp_dir("on_place_beta_v2");
        fs::create_dir_all(beta_dir.join("src")).unwrap();
        fs::write(
            beta_dir.join("src/place_beta.pstdr"),
            r#"@addon(target = "26.60", api = "2.11.0-beta")

on place of block by player {
    cancel event
    send "Blocked {block.type_id}" to player
}
"#,
        )
        .unwrap();
        let conv_beta = convert_workspace(&beta_dir, &spec).unwrap();
        assert!(!has_errors(&conv_beta), "Beta on place with cancel event must pass convert: {conv_beta:?}");
        let audit_beta = audit_workspace(&beta_dir, &spec).unwrap();
        assert!(!has_errors(&audit_beta), "Beta on place with cancel event must pass audit: {audit_beta:?}");
        let _ = fs::remove_dir_all(&beta_dir);
    }

    #[test]
    fn qa_manifest_min_engine_version_syncs_on_profile_switch() {
        let dir = temp_dir("manifest_profile_switch");
        let spec = BedrockSpec::bundled();
        init_workspace(&dir, "SwitchPack", &spec).unwrap();

        let initial_manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/manifest.json")).unwrap(),
        )
        .unwrap();
        let orig_uuid = initial_manifest["header"]["uuid"].as_str().unwrap().to_string();
        let orig_name = initial_manifest["header"]["name"].as_str().unwrap().to_string();
        let orig_desc = initial_manifest["header"]["description"].as_str().unwrap().to_string();
        assert_eq!(initial_manifest["header"]["min_engine_version"], serde_json::json!([1, 26, 60]));

        // Switch workspace script to Legacy 1.x LTS (1.21.50 / 1.16.0)
        fs::write(
            dir.join("src/welcome.pstdr"),
            "@addon(target = \"1.21.50\", api = \"1.16.0\")\n\non join of player {\n    send \"Legacy\" to player\n}\n",
        )
        .unwrap();
        let diags_v1 = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags_v1), "diags_v1: {diags_v1:?}");

        let legacy_manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            legacy_manifest["header"]["min_engine_version"],
            serde_json::json!([1, 21, 50]),
            "Switching to Legacy 1.x must update min_engine_version to [1, 21, 50]"
        );
        assert_eq!(legacy_manifest["header"]["uuid"].as_str(), Some(orig_uuid.as_str()));
        assert_eq!(legacy_manifest["header"]["name"].as_str(), Some(orig_name.as_str()));
        assert_eq!(legacy_manifest["header"]["description"].as_str(), Some(orig_desc.as_str()));

        // Switch back to Modern 2.x (26.60 / 2.11.0)
        fs::write(
            dir.join("src/welcome.pstdr"),
            "@addon(target = \"26.60\", api = \"2.11.0\")\n\non join of player {\n    send \"Modern\" to player\n}\n",
        )
        .unwrap();
        let diags_v2 = convert_workspace(&dir, &spec).unwrap();
        assert!(!has_errors(&diags_v2), "diags_v2: {diags_v2:?}");

        let modern_manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("BP/manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            modern_manifest["header"]["min_engine_version"],
            serde_json::json!([1, 26, 60]),
            "Switching back to Modern 2.x must update min_engine_version to [1, 26, 60]"
        );
        assert_eq!(modern_manifest["header"]["uuid"].as_str(), Some(orig_uuid.as_str()));

        let _ = fs::remove_dir_all(&dir);
    }
}
