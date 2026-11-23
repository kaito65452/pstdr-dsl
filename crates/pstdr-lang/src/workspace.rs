use crate::codegen::{emit_code, CodegenOptions, ComponentKind, EmitMode};
use crate::diagnostics::{has_errors, Diagnostic, DiagnosticCode, Span};
use crate::formatter::format_source;
use crate::parser::parse_source;
use crate::spec::BedrockSpec;
use crate::validator::{validate_program, ValidationContext};
use crate::wiring::{generate_uuid_v4, is_valid_v3_min_engine, resolve_min_engine_version, update_main_js, update_manifest, wire_bind_targets};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub pack_name: String,
    pub bedrock_target: String,
    pub script_api: String,
    pub src_dir: String,
    pub js_out_dir: String,
    pub ts_out_dir: String,
    pub emit_ts: bool,
    pub strict_vanilla_ids: bool,
    pub source_map_comments: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            pack_name: "PackStudio Add-On".to_string(),
            bedrock_target: "26.60".to_string(),
            script_api: "2.11.0".to_string(),
            src_dir: "src".to_string(),
            js_out_dir: "BP/scripts/generated".to_string(),
            ts_out_dir: "build/ts".to_string(),
            emit_ts: true,
            strict_vanilla_ids: true,
            source_map_comments: true,
        }
    }
}

impl WorkspaceConfig {
    pub fn load_from_root(root: &Path) -> Self {
        let mut cfg = Self::default();
        let toml_path = root.join("pstdr.toml");
        let Ok(raw) = fs::read_to_string(toml_path) else {
            return cfg;
        };

        for line in raw.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let key = k.trim();
            let val = v.trim().trim_matches('"');
            match key {
                "name" | "pack_name" => cfg.pack_name = val.to_string(),
                "bedrock_target" | "target" => cfg.bedrock_target = val.to_string(),
                "script_api" | "api" => cfg.script_api = val.to_string(),
                "src_dir" => cfg.src_dir = val.to_string(),
                "js_out_dir" => cfg.js_out_dir = val.to_string(),
                "ts_out_dir" => cfg.ts_out_dir = val.to_string(),
                "emit_ts" => cfg.emit_ts = val == "true",
                "strict_vanilla_ids" => cfg.strict_vanilla_ids = val == "true",
                "source_map_comments" => cfg.source_map_comments = val == "true",
                _ => {}
            }
        }
        cfg
    }
}

#[derive(Debug, Clone)]
pub struct CompileResult {
    pub js: Option<String>,
    pub ts: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut cur = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        if cur.join("pstdr.toml").is_file() || cur.join("BP").is_dir() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

pub fn collect_pstdr_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("pstdr") {
            files.push(path.to_path_buf());
        }
        return Ok(files);
    }
    if !path.is_dir() {
        return Err(format!("Path tidak ditemukan: '{}'", path.display()));
    }
    collect_recursive(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Gagal membaca direktori '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Gagal membaca entri direktori: {e}"))?;
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | "node_modules" | ".git" | "build") {
                continue;
            }
            collect_recursive(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("pstdr") {
            out.push(p);
        }
    }
    Ok(())
}

pub fn compile_source(
    source: &str,
    file: &str,
    workspace_root: Option<&Path>,
    spec: &BedrockSpec,
    strict_vanilla_ids: bool,
    source_map_comments: bool,
    custom_components: Vec<(String, ComponentKind)>,
) -> CompileResult {
    let (program, parse_diags) = parse_source(source, file);
    let Some(prog) = program else {
        return CompileResult {
            js: None,
            ts: None,
            diagnostics: parse_diags,
        };
    };

    let vctx = ValidationContext {
        workspace_root,
        spec,
        strict_vanilla_ids,
    };
    let (_info, val_diags) = validate_program(&prog, &vctx);
    if has_errors(&val_diags) {
        return CompileResult {
            js: None,
            ts: None,
            diagnostics: val_diags,
        };
    }

    let opts = CodegenOptions {
        source_file: file.to_string(),
        source_map_comments,
        custom_components,
    };
    let js = emit_code(&prog, &opts, EmitMode::JavaScript);
    let ts = emit_code(&prog, &opts, EmitMode::TypeScript);

    CompileResult {
        js: Some(js),
        ts: Some(ts),
        diagnostics: val_diags,
    }
}

pub fn convert_single_file(
    from: &Path,
    to: &Path,
    spec: &BedrockSpec,
    emit_ts_sibling: bool,
) -> Result<Vec<Diagnostic>, String> {
    let source = fs::read_to_string(from)
        .map_err(|e| format!("Gagal membaca '{}': {e}", from.display()))?;
    let file_label = from.display().to_string();
    let ws_root = find_workspace_root(from);

    let res = compile_source(
        &source,
        &file_label,
        ws_root.as_deref(),
        spec,
        true,
        true,
        Vec::new(),
    );
    if has_errors(&res.diagnostics) {
        return Ok(res.diagnostics);
    }

    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Gagal membuat folder '{}': {e}", parent.display()))?;
        }
    }

    let is_ts_out = to.extension().and_then(|e| e.to_str()) == Some("ts");
    let primary = if is_ts_out {
        res.ts.as_deref().unwrap_or("")
    } else {
        res.js.as_deref().unwrap_or("")
    };
    fs::write(to, primary).map_err(|e| format!("Gagal menulis '{}': {e}", to.display()))?;

    if emit_ts_sibling && !is_ts_out {
        let ts_path = to.with_extension("ts");
        if let Some(ts_code) = res.ts {
            fs::write(&ts_path, ts_code)
                .map_err(|e| format!("Gagal menulis '{}': {e}", ts_path.display()))?;
        }
    }

    Ok(res.diagnostics)
}

pub fn convert_workspace(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let cfg = WorkspaceConfig::load_from_root(root);
    let src_dir = root.join(&cfg.src_dir);
    let files = if src_dir.is_dir() {
        collect_pstdr_files(&src_dir)?
    } else {
        collect_pstdr_files(root)?
    };

    let mut all_diags = Vec::new();
    let mut generated_stems = Vec::new();
    let mut target_ver = cfg.bedrock_target.clone();
    let mut api_ver = cfg.script_api.clone();

    let js_dir = root.join(&cfg.js_out_dir);
    let ts_dir = root.join(&cfg.ts_out_dir);

    for file_path in &files {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Gagal membaca '{}': {e}", file_path.display()))?;
        let rel = file_path
            .strip_prefix(root)
            .unwrap_or(file_path)
            .display()
            .to_string();

        let (prog_opt, parse_diags) = parse_source(&source, &rel);
        let Some(prog) = prog_opt else {
            all_diags.extend(parse_diags);
            continue;
        };

        if let Some(h) = &prog.header {
            target_ver = h.target.clone();
            api_ver = h.api.clone();
        }

        let vctx = ValidationContext {
            workspace_root: Some(root),
            spec,
            strict_vanilla_ids: cfg.strict_vanilla_ids,
        };
        let (_info, val_diags) = validate_program(&prog, &vctx);
        let failed = has_errors(&val_diags);
        all_diags.extend(val_diags);
        if failed {
            continue;
        }

        let stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script")
            .to_string();

        let registered = wire_bind_targets(root, &prog.bind_targets, &stem)?;

        let opts = CodegenOptions {
            source_file: rel,
            source_map_comments: cfg.source_map_comments,
            custom_components: registered,
        };
        let js = emit_code(&prog, &opts, EmitMode::JavaScript);
        let ts = emit_code(&prog, &opts, EmitMode::TypeScript);

        fs::create_dir_all(&js_dir)
            .map_err(|e| format!("Gagal membuat folder '{}': {e}", js_dir.display()))?;
        let js_out = js_dir.join(format!("{stem}.js"));
        fs::write(&js_out, js)
            .map_err(|e| format!("Gagal menulis '{}': {e}", js_out.display()))?;

        if cfg.emit_ts {
            fs::create_dir_all(&ts_dir)
                .map_err(|e| format!("Gagal membuat folder '{}': {e}", ts_dir.display()))?;
            let ts_out = ts_dir.join(format!("{stem}.ts"));
            fs::write(&ts_out, ts)
                .map_err(|e| format!("Gagal menulis '{}': {e}", ts_out.display()))?;
        }

        generated_stems.push(stem);
    }

    if !has_errors(&all_diags) && !generated_stems.is_empty() {
        update_main_js(root, &generated_stems)?;
        update_manifest(root, &target_ver, &api_ver)?;
    }

    Ok(all_diags)
}

pub fn check_path(
    path: &Path,
    workspace_root: Option<&Path>,
    spec: &BedrockSpec,
) -> Result<Vec<Diagnostic>, String> {
    let files = collect_pstdr_files(path)?;
    let cfg = workspace_root
        .map(WorkspaceConfig::load_from_root)
        .unwrap_or_default();
    let mut all_diags = Vec::new();

    for f in files {
        let source = fs::read_to_string(&f)
            .map_err(|e| format!("Gagal membaca '{}': {e}", f.display()))?;
        let label = workspace_root
            .and_then(|r| f.strip_prefix(r).ok())
            .unwrap_or(&f)
            .display()
            .to_string();

        let (prog_opt, parse_diags) = parse_source(&source, &label);
        let Some(prog) = prog_opt else {
            all_diags.extend(parse_diags);
            continue;
        };

        let vctx = ValidationContext {
            workspace_root,
            spec,
            strict_vanilla_ids: cfg.strict_vanilla_ids,
        };
        let (_info, val_diags) = validate_program(&prog, &vctx);
        all_diags.extend(val_diags);
    }

    Ok(all_diags)
}

pub fn fmt_path(path: &Path, check_only: bool) -> Result<(usize, Vec<Diagnostic>), String> {
    let files = collect_pstdr_files(path)?;
    let mut changed = 0usize;
    let mut all_diags = Vec::new();

    for f in files {
        let source = fs::read_to_string(&f)
            .map_err(|e| format!("Gagal membaca '{}': {e}", f.display()))?;
        let label = f.display().to_string();
        match format_source(&source, &label) {
            Ok(formatted) => {
                if formatted != source {
                    changed += 1;
                    if !check_only {
                        fs::write(&f, formatted)
                            .map_err(|e| format!("Gagal menulis '{}': {e}", f.display()))?;
                    }
                }
            }
            Err(diags) => all_diags.extend(diags),
        }
    }

    Ok((changed, all_diags))
}

pub fn audit_workspace(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let cfg = WorkspaceConfig::load_from_root(root);
    let src_dir = root.join(&cfg.src_dir);
    let scan_target = if src_dir.is_dir() { src_dir } else { root.to_path_buf() };
    let mut diags = check_path(&scan_target, Some(root), spec)?;

    let bp_manifest = root.join("BP/manifest.json");
    let mut bp_uuid: Option<String> = None;

    if bp_manifest.is_file() {
        let raw = fs::read_to_string(&bp_manifest)
            .map_err(|e| format!("Gagal membaca BP/manifest.json: {e}"))?;
        match serde_json::from_str::<Value>(&raw) {
            Ok(val) => {
                let fmt_ver = val
                    .get("format_version")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2);
                let header = val.get("header");
                bp_uuid = header
                    .and_then(|h| h.get("uuid"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string());

                let min_ok = if fmt_ver == 3 {
                    header
                        .and_then(|h| h.get("min_engine_version"))
                        .and_then(|v| v.as_str())
                        .map(is_valid_v3_min_engine)
                        .unwrap_or(false)
                } else {
                    header
                        .and_then(|h| h.get("min_engine_version"))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.len() == 3
                                && arr[0].as_u64() == Some(1)
                                && arr[1].as_u64().is_some()
                                && arr[2].as_u64().is_some()
                        })
                        .unwrap_or(false)
                };

                if !min_ok {
                    if fmt_ver == 3 {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E015,
                            &Span::file_only("BP/manifest.json"),
                            "Audit Gate gagal: manifest format_version: 3 mewajibkan 'min_engine_version' berupa string SemVer \"1.x.x\".",
                            "Atur \"min_engine_version\": \"1.26.60\" pada header BP/manifest.json. Angka pertama wajib 1 (contoh: \"1.26.60\").",
                        ));
                    } else {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E015,
                            &Span::file_only("BP/manifest.json"),
                            "Audit Gate gagal: 'min_engine_version' harus berformat [1, x, x].",
                            "Atur \"min_engine_version\": [1, 26, 60] pada header BP/manifest.json. \
                             Angka pertama wajib 1, meski target game adalah 26.60 (mis. [1, 26, 60]) \
                             atau versi lama (mis. [1, 21, 50]).",
                        ));
                    }
                }
            }
            Err(e) => {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E015,
                    &Span::file_only("BP/manifest.json"),
                    format!("Audit Gate gagal: BP/manifest.json tidak valid ({e})."),
                    "Perbaiki format JSON pada BP/manifest.json.",
                ));
            }
        }
    }

    let rp_manifest = root.join("RP/manifest.json");
    if rp_manifest.is_file() {
        if let Ok(raw) = fs::read_to_string(&rp_manifest) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                let rp_uuid = val
                    .get("header")
                    .and_then(|h| h.get("uuid"))
                    .and_then(|u| u.as_str());
                if let (Some(bp), Some(rp)) = (bp_uuid.as_deref(), rp_uuid) {
                    if bp == rp {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E015,
                            &Span::file_only("RP/manifest.json"),
                            format!("Audit Gate gagal: UUID header BP dan RP bentrok ('{bp}')."),
                            "Buat UUID v4 unik untuk masing-masing header BP dan RP.",
                        ));
                    }
                }
            }
        }
    }

    Ok(diags)
}

pub fn init_workspace(root: &Path, pack_name: &str, spec: &BedrockSpec) -> Result<(), String> {
    fs::create_dir_all(root.join("src")).map_err(|e| format!("Gagal membuat src/: {e}"))?;
    fs::create_dir_all(root.join("BP/items")).map_err(|e| format!("Gagal membuat BP/items/: {e}"))?;
    fs::create_dir_all(root.join("BP/scripts/generated"))
        .map_err(|e| format!("Gagal membuat BP/scripts/generated/: {e}"))?;
    fs::create_dir_all(root.join("RP/models/entity"))
        .map_err(|e| format!("Gagal membuat RP/models/entity/: {e}"))?;

    let toml_content = format!(
        "[pack]\nname = \"{pack_name}\"\nbedrock_target = \"{}\"\nscript_api = \"{}\"\n\n\
         [paths]\nsrc_dir = \"src\"\njs_out_dir = \"BP/scripts/generated\"\nts_out_dir = \"build/ts\"\n\n\
         [compiler]\nemit_ts = true\nstrict_vanilla_ids = true\nsource_map_comments = true\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(root.join("pstdr.toml"), toml_content)
        .map_err(|e| format!("Gagal menulis pstdr.toml: {e}"))?;

    let fmt_ver = 2u64;
    let mod_ver = if fmt_ver == 3 {
        json!("1.0.0")
    } else {
        json!([1, 0, 0])
    };
    let min_engine = resolve_min_engine_version(&spec.bedrock_version)
        .unwrap_or([1, 26, 60]);
    let bp_manifest = json!({
        "format_version": fmt_ver,
        "header": {
            "name": format!("{pack_name} BP"),
            "description": "Behavior Pack powered by PackStudio (.pstdr)",
            "uuid": generate_uuid_v4(),
            "version": mod_ver,
            "min_engine_version": min_engine
        },
        "modules": [
            {
                "type": "data",
                "uuid": generate_uuid_v4(),
                "version": mod_ver
            },
            {
                "type": "script",
                "language": "javascript",
                "uuid": generate_uuid_v4(),
                "entry": "scripts/main.js",
                "version": mod_ver
            }
        ],
        "dependencies": [
            {
                "module_name": "@minecraft/server",
                "version": spec.script_api_version
            }
        ]
    });
    fs::write(
        root.join("BP/manifest.json"),
        format!("{}\n", serde_json::to_string_pretty(&bp_manifest).unwrap()),
    )
    .map_err(|e| format!("Gagal menulis BP/manifest.json: {e}"))?;

    let sample_script = format!(
        "@addon(target = \"{}\", api = \"{}\")\n\n\
         on join of player {{\n    send \"§aSelamat datang di {pack_name}!\" to player\n}}\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(root.join("src/welcome.pstdr"), sample_script)
        .map_err(|e| format!("Gagal menulis src/welcome.pstdr: {e}"))?;

    Ok(())
}

pub fn new_script(root: &Path, name: &str, spec: &BedrockSpec) -> Result<PathBuf, String> {
    let clean = name.trim().trim_end_matches(".pstdr");
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).map_err(|e| format!("Gagal membuat src/: {e}"))?;
    let path = src_dir.join(format!("{clean}.pstdr"));
    if path.exists() {
        return Err(format!("File '{}' sudah ada.", path.display()));
    }
    let content = format!(
        "@addon(target = \"{}\", api = \"{}\")\n\n\
         const ITEM_ID = \"my_pack:{clean}\"\n\n\
         on right click with ITEM_ID by player {{\n    player.actionbar(\"§bMenggunakan {clean}!\")\n}}\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(&path, content).map_err(|e| format!("Gagal menulis '{}': {e}", path.display()))?;
    Ok(path)
}

pub fn doctor_report(root: Option<&Path>, spec: &BedrockSpec) -> String {
    let mut out = String::new();
    out.push_str("pstdr-c doctor (v0.1.0-alpha)\n");
    out.push_str(&format!(
        "  Spec source      : {}\n",
        spec.source_label
    ));
    out.push_str(&format!(
        "  Bedrock target   : {} (@minecraft/server {})\n",
        spec.bedrock_version, spec.script_api_version
    ));
    out.push_str(&format!(
        "  Vanilla catalog  : {} effects, {} sounds\n",
        spec.effects.len(),
        spec.sounds.len()
    ));
    match root {
        Some(r) => {
            out.push_str(&format!("  Workspace root   : {}\n", r.display()));
            out.push_str(&format!(
                "  pstdr.toml       : {}\n",
                if r.join("pstdr.toml").is_file() { "OK" } else { "tidak ada (pakai default)" }
            ));
            out.push_str(&format!(
                "  BP/manifest.json : {}\n",
                if r.join("BP/manifest.json").is_file() { "OK" } else { "belum dibuat" }
            ));
        }
        None => {
            out.push_str("  Workspace root   : tidak terdeteksi (jalankan di root proyek atau pakai 'pstdr-c init')\n");
        }
    }
    out
}
