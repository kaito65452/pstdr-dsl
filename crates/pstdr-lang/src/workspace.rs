use crate::assets::{
    validate_bedrock_schema_gates, validate_workspace_assets_with_index, WorkspaceAssetIndex,
};
use crate::ast::{AddonDirective, ApiProfile, EventKind, Program, TopLevelItem};
use crate::codegen::{
    block_contains_cancel, emit_block_component_module_grouped, emit_code, emit_code_with_context,
    emit_commands_module, emit_item_component_module_grouped, emit_shared_module_with_externals,
    emit_world_module, program_uses_ui, CodegenOptions, ComponentKind, EmitMode,
    ExternalSymbolKind, SharedContext,
};
use crate::diagnostics::{has_errors, Diagnostic, DiagnosticCode, Span};
use crate::formatter::format_source;
use crate::parser::parse_source;
use crate::partition::{partition_program, TargetEntry};
use crate::spec::BedrockSpec;
use crate::validator::{
    collect_module_exports, eval_static_string, has_path_traversal, normalize_mcfunction_path,
    resolve_import_module_path, validate_program, ExportedSymbolKind, ModuleExports,
    ValidationContext,
};
use crate::wiring::{
    emit_registry_module_for_profile, generate_uuid_v4, is_valid_v3_min_engine,
    resolve_min_engine_version, update_main_js, update_manifest_with_ui, wire_bind_targets,
    wire_target_json, RegistryEntry,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
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
    pub strict_asset_refs: bool,
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
            strict_asset_refs: true,
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
                "strict_asset_refs" => cfg.strict_asset_refs = val == "true",
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
        return Err(format!("Path not found: '{}'", path.display()));
    }
    collect_recursive(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
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

fn sort_and_dedup_diags(diags: &mut Vec<Diagnostic>) {
    diags.sort_by(|a, b| {
        a.file.cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.column.cmp(&b.column))
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| a.message.cmp(&b.message))
    });
    diags.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && a.column == b.column
            && a.code == b.code
            && a.message == b.message
    });
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
    let (program, mut diags) = parse_source(source, file);
    let vctx = ValidationContext::simple(workspace_root, spec, strict_vanilla_ids);
    let mut bound_block_ids = Vec::new();
    if let Some(prog) = &program {
        let (info, val_diags) = validate_program(prog, &vctx);
        bound_block_ids = info.bound_block_ids.into_iter().map(|(_, id, _)| id).collect();
        diags.extend(val_diags);
    }
    sort_and_dedup_diags(&mut diags);

    if has_errors(&diags) || program.is_none() {
        return CompileResult {
            js: None,
            ts: None,
            diagnostics: diags,
        };
    }

    let prog = program.unwrap();
    let opts = CodegenOptions {
        source_file: file.to_string(),
        source_map_comments,
        custom_components,
        bound_block_ids,
    };
    let js = emit_code(&prog, &opts, EmitMode::JavaScript);
    let ts = emit_code(&prog, &opts, EmitMode::TypeScript);

    CompileResult {
        js: Some(js),
        ts: Some(ts),
        diagnostics: diags,
    }
}

pub fn convert_single_file(
    from: &Path,
    to: &Path,
    spec: &BedrockSpec,
    emit_ts_sibling: bool,
) -> Result<Vec<Diagnostic>, String> {
    let source = fs::read_to_string(from)
        .map_err(|e| format!("Failed to read '{}': {e}", from.display()))?;
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
                .map_err(|e| format!("Failed to create directory '{}': {e}", parent.display()))?;
        }
    }

    let is_ts_out = to.extension().and_then(|e| e.to_str()) == Some("ts");
    let primary = if is_ts_out {
        res.ts.as_deref().unwrap_or("")
    } else {
        res.js.as_deref().unwrap_or("")
    };
    fs::write(to, primary).map_err(|e| format!("Failed to write '{}': {e}", to.display()))?;

    if emit_ts_sibling && !is_ts_out {
        let ts_path = to.with_extension("ts");
        if let Some(ts_code) = res.ts {
            fs::write(&ts_path, ts_code)
                .map_err(|e| format!("Failed to write '{}': {e}", ts_path.display()))?;
        }
    }

    Ok(res.diagnostics)
}

fn collect_inline_mcfunctions_from_programs<'a>(
    parsed: impl Iterator<Item = &'a Program>,
) -> HashSet<String> {
    let mut found = HashSet::new();
    for prog in parsed {
        for item in &prog.items {
            if let TopLevelItem::McFunctionDecl(mc) = item {
                let norm = normalize_mcfunction_path(&mc.path);
                if !has_path_traversal(&norm) {
                    found.insert(norm);
                }
            }
        }
    }
    found
}

fn is_specific_item_expr(expr: &crate::ast::Expr, const_strings: &HashMap<String, String>) -> bool {
    eval_static_string(expr, const_strings).is_some_and(|s| !s.contains('*'))
}

fn program_has_custom_component_callbacks(prog: &Program) -> bool {
    let mut const_strings = HashMap::new();
    for item in &prog.items {
        if let TopLevelItem::ConstDecl(d) = item {
            if let Some(s) = eval_static_string(&d.value, &const_strings) {
                const_strings.insert(d.name.clone(), s);
            }
        }
    }
    prog.items.iter().any(|item| {
        if let TopLevelItem::EventHandler(ev) = item {
            match &ev.kind {
                EventKind::RightClick { item, .. } => {
                    !block_contains_cancel(&ev.body)
                        && item.as_ref().is_some_and(|e| is_specific_item_expr(e, &const_strings))
                }
                EventKind::HitTarget { item, .. } | EventKind::Consume { item, .. } => {
                    item.as_ref().is_some_and(|e| is_specific_item_expr(e, &const_strings))
                }
                EventKind::BlockTick { .. } => true,
                EventKind::BlockInteract { .. } | EventKind::BreakBlock { .. } => {
                    !block_contains_cancel(&ev.body)
                }
                _ => false,
            }
        } else {
            false
        }
    })
}

fn relative_esm_import(from_file_rel: &str, to_file_rel: &str, mode: EmitMode) -> String {
    let from_parts: Vec<&str> = from_file_rel.split('/').filter(|s| !s.is_empty()).collect();
    let from_dir = if from_parts.len() > 1 {
        &from_parts[..from_parts.len() - 1]
    } else {
        &[]
    };
    let to_parts: Vec<&str> = to_file_rel.split('/').filter(|s| !s.is_empty()).collect();
    let mut common = 0usize;
    while common < from_dir.len()
        && common + 1 < to_parts.len()
        && from_dir[common] == to_parts[common]
    {
        common += 1;
    }
    let ups = from_dir.len() - common;
    let mut out = String::new();
    if ups == 0 {
        out.push_str("./");
    } else {
        for _ in 0..ups {
            out.push_str("../");
        }
    }
    out.push_str(&to_parts[common..].join("/"));
    if mode == EmitMode::TypeScript {
        if let Some(stripped) = out.strip_suffix(".js") {
            return stripped.to_string();
        }
    }
    out
}

fn build_external_import_ctx(
    prog: &Program,
    emitter_rel_js: &str,
    importer_file: &str,
    root: &Path,
    src_dir: &str,
    export_targets: &HashMap<PathBuf, (String, ModuleExports)>,
    mode: EmitMode,
) -> SharedContext {
    let mut ctx = SharedContext::default();
    for item in &prog.items {
        if let TopLevelItem::Import(imp) = item {
            let Some(target_path) =
                resolve_import_module_path(importer_file, &imp.path, Some(root), Some(src_dir))
            else {
                continue;
            };
            let key = fs::canonicalize(&target_path).unwrap_or(target_path);
            let Some((target_js_rel, exports)) = export_targets.get(&key) else {
                continue;
            };
            let rel_import = relative_esm_import(emitter_rel_js, target_js_rel, mode);
            for sym in &imp.symbols {
                if let Some(kind) = exports.symbols.get(&sym.name) {
                    match kind {
                        ExportedSymbolKind::State => {
                            ctx.external_symbols.insert(
                                sym.name.clone(),
                                (rel_import.clone(), ExternalSymbolKind::State),
                            );
                        }
                        ExportedSymbolKind::Const { const_string } => {
                            if let Some(s) = const_string {
                                ctx.const_strings.insert(sym.name.clone(), s.clone());
                            }
                            ctx.external_symbols.insert(
                                sym.name.clone(),
                                (rel_import.clone(), ExternalSymbolKind::Value),
                            );
                        }
                        ExportedSymbolKind::Table | ExportedSymbolKind::Func { .. } => {
                            ctx.external_symbols.insert(
                                sym.name.clone(),
                                (rel_import.clone(), ExternalSymbolKind::Value),
                            );
                        }
                    }
                }
            }
        }
    }
    ctx
}

pub fn build_workspace(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let index = WorkspaceAssetIndex::build(root);

    let (mut diags, parsed_programs) = convert_workspace_with_index_collect(root, &index, spec)?;
    if has_errors(&diags) {
        return Ok(diags);
    }

    let audit_diags = audit_workspace_with_collected(root, &index, spec, &parsed_programs, false)?;
    diags.extend(audit_diags);
    sort_and_dedup_diags(&mut diags);
    Ok(diags)
}

pub fn convert_workspace(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let index = WorkspaceAssetIndex::build(root);
    convert_workspace_with_index(root, &index, spec)
}

pub fn convert_workspace_with_index(
    root: &Path,
    index: &WorkspaceAssetIndex,
    spec: &BedrockSpec,
) -> Result<Vec<Diagnostic>, String> {
    let (diags, _) = convert_workspace_with_index_collect(root, index, spec)?;
    Ok(diags)
}

fn convert_workspace_with_index_collect(
    root: &Path,
    index: &WorkspaceAssetIndex,
    spec: &BedrockSpec,
) -> Result<(Vec<Diagnostic>, Vec<Program>), String> {
    let cfg = &index.config;
    let src_dir = root.join(&cfg.src_dir);
    let files = if src_dir.is_dir() {
        collect_pstdr_files(&src_dir)?
    } else {
        collect_pstdr_files(root)?
    };

    let mut all_diags = Vec::new();

    let mut has_asset_err = false;
    if cfg.strict_asset_refs {
        let asset_diags = validate_workspace_assets_with_index(index, spec);
        has_asset_err = has_errors(&asset_diags);
        all_diags.extend(asset_diags);
    }

    let mut target_ver = cfg.bedrock_target.clone();
    let mut api_ver = cfg.script_api.clone();
    let mut registry = index.target_registry.clone();

    struct ParsedFile {
        path: PathBuf,
        rel: String,
        stem: String,
        module_rel_no_ext: String,
        prog: Option<Program>,
        diags: Vec<Diagnostic>,
    }

    let mut parsed_files = Vec::with_capacity(files.len());
    let src_root_dir = root.join(&cfg.src_dir);
    for file_path in &files {
        let source = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read '{}': {e}", file_path.display()))?;
        let rel = file_path
            .strip_prefix(root)
            .unwrap_or(file_path)
            .display()
            .to_string()
            .replace('\\', "/");
        let stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script")
            .to_string();
        let rel_under_src = file_path
            .strip_prefix(&src_root_dir)
            .or_else(|_| file_path.strip_prefix(root))
            .unwrap_or(file_path);
        let module_rel_no_ext = rel_under_src
            .with_extension("")
            .display()
            .to_string()
            .replace('\\', "/");
        let (prog, diags) = parse_source(&source, &rel);
        parsed_files.push(ParsedFile {
            path: file_path.clone(),
            rel,
            stem,
            module_rel_no_ext,
            prog,
            diags,
        });
    }

    let inline_mcfunctions = collect_inline_mcfunctions_from_programs(
        parsed_files.iter().filter_map(|pf| pf.prog.as_ref()),
    );

    struct ValidatedFile {
        canon_path: PathBuf,
        rel: String,
        stem: String,
        module_rel_no_ext: String,
        prog: Program,
        bound_block_ids: Vec<String>,
    }

    let mut validated_files = Vec::new();

    for pf in parsed_files {
        let mut file_diags = pf.diags;
        let mut bound_block_ids = Vec::new();
        if let Some(prog) = &pf.prog {
            if let Some(h) = &prog.header {
                target_ver = h.target.clone();
                if !crate::spec::is_beta_api_version(&api_ver)
                    || crate::spec::is_beta_api_version(&h.api)
                {
                    api_ver = h.api.clone();
                }
            }

            let vctx = ValidationContext {
                workspace_root: Some(root),
                spec,
                strict_vanilla_ids: cfg.strict_vanilla_ids,
                asset_index: Some(index),
                workspace_mcfunctions: Some(&inline_mcfunctions),
            };
            let (info, val_diags) = validate_program(prog, &vctx);
            bound_block_ids = info.bound_block_ids.into_iter().map(|(_, id, _)| id).collect();
            file_diags.extend(val_diags);
        }
        sort_and_dedup_diags(&mut file_diags);

        let file_failed = has_errors(&file_diags) || pf.prog.is_none();
        all_diags.extend(file_diags);
        if file_failed || has_asset_err {
            continue;
        }

        let prog = pf.prog.unwrap();
        registry.register_bind_targets_cached(root, &prog.bind_targets, &index.bp_items, &index.bp_blocks);

        let canon_path = fs::canonicalize(&pf.path).unwrap_or(pf.path);
        validated_files.push(ValidatedFile {
            canon_path,
            rel: pf.rel,
            stem: pf.stem,
            module_rel_no_ext: pf.module_rel_no_ext,
            prog,
            bound_block_ids,
        });
    }

    if has_errors(&all_diags) || has_asset_err {
        sort_and_dedup_diags(&mut all_diags);
        return Ok((all_diags, Vec::new()));
    }

    let js_dir = root.join(&cfg.js_out_dir);
    let ts_dir = root.join(&cfg.ts_out_dir);

    // Clean stale generated modules before writing new outputs so deleted targets
    // do not leave orphan `.js`/`.ts` files behind.
    clean_generated_dir(&js_dir, "js")?;
    if cfg.emit_ts {
        clean_generated_dir(&ts_dir, "ts")?;
    }

    let mut main_modules: Vec<String> = Vec::new();
    let mut command_modules: Vec<String> = Vec::new();
    let mut workspace_uses_ui = false;

    let mut partitioned_files = Vec::with_capacity(validated_files.len());
    let mut total_cmd_files = 0usize;
    let mut any_partitioned_targets = false;
    let mut target_slug_file_counts: HashMap<(ComponentKind, String), usize> = HashMap::new();

    for vf in validated_files {
        if program_uses_ui(&vf.prog) {
            workspace_uses_ui = true;
        }
        let part = partition_program(&vf.prog, &registry);
        if !part.commands.is_empty() {
            total_cmd_files += 1;
        }
        if part.should_partition(vf.prog.bind_targets.len()) {
            any_partitioned_targets = true;
        }
        for slug in part.items.keys() {
            *target_slug_file_counts
                .entry((ComponentKind::Item, slug.clone()))
                .or_insert(0) += 1;
        }
        for slug in part.blocks.keys() {
            *target_slug_file_counts
                .entry((ComponentKind::Block, slug.clone()))
                .or_insert(0) += 1;
        }
        partitioned_files.push((vf, part));
    }
    let merge_commands_into_registry =
        total_cmd_files > 1 || (total_cmd_files >= 1 && any_partitioned_targets);

    // Precompute export destinations so every `import { ... } from "..."` resolves to its exact
    // generated JS/TS module path regardless of whether exporter or importer is partitioned.
    let mut export_targets: HashMap<PathBuf, (String, ModuleExports)> = HashMap::new();
    for (vf, part) in &partitioned_files {
        let collides_across_files = part
            .items
            .keys()
            .any(|s| target_slug_file_counts.get(&(ComponentKind::Item, s.clone())).copied().unwrap_or(0) > 1)
            || part
                .blocks
                .keys()
                .any(|s| target_slug_file_counts.get(&(ComponentKind::Block, s.clone())).copied().unwrap_or(0) > 1);
        let should_partition = part.should_partition(vf.prog.bind_targets.len())
            || collides_across_files
            || (merge_commands_into_registry && !part.commands.is_empty());
        let shared_slug = vf.module_rel_no_ext.replace('/', "_");
        let export_js_rel = if should_partition {
            format!("_shared/{shared_slug}.shared.js")
        } else {
            format!("{}.js", vf.module_rel_no_ext)
        };
        export_targets.insert(
            vf.canon_path.clone(),
            (export_js_rel, collect_module_exports(&vf.prog)),
        );
    }

    struct MergedComponentGroup {
        entry: TargetEntry,
        header: Option<AddonDirective>,
        primary_source: String,
        handlers: Vec<(crate::ast::EventHandler, SharedContext, String)>,
        handlers_ts: Vec<(crate::ast::EventHandler, SharedContext, String)>,
    }

    let mut collected_programs: Vec<Program> = Vec::with_capacity(partitioned_files.len());
    let mut merged_items: BTreeMap<String, MergedComponentGroup> = BTreeMap::new();
    let mut merged_blocks: BTreeMap<String, MergedComponentGroup> = BTreeMap::new();

    for (vf, part) in partitioned_files {
        for item in &vf.prog.items {
            if let TopLevelItem::McFunctionDecl(mc) = item {
                let norm = normalize_mcfunction_path(&mc.path);
                if has_path_traversal(&norm) {
                    return Err(format!(
                        "Refusing to write mcfunction with unsafe path '{}'",
                        mc.path
                    ));
                }
                let content = sanitize_mcfunction_body(&mc.raw_body);
                let out_path = root
                    .join("BP/functions")
                    .join(format!("{norm}.mcfunction"));
                write_generated_file(&out_path, &content)?;
            }
        }

        let fallback_header = AddonDirective {
            target: target_ver.clone(),
            api: api_ver.clone(),
            span: Span::file_only(&vf.rel),
        };
        let collides_across_files = part
            .items
            .keys()
            .any(|s| target_slug_file_counts.get(&(ComponentKind::Item, s.clone())).copied().unwrap_or(0) > 1)
            || part
                .blocks
                .keys()
                .any(|s| target_slug_file_counts.get(&(ComponentKind::Block, s.clone())).copied().unwrap_or(0) > 1);
        let should_partition = part.should_partition(vf.prog.bind_targets.len())
            || collides_across_files
            || (merge_commands_into_registry && !part.commands.is_empty());

        if !should_partition {
            let registered = if program_has_custom_component_callbacks(&vf.prog) {
                wire_bind_targets(root, &vf.prog.bind_targets, &vf.stem)?
            } else {
                Vec::new()
            };
            let out_rel_js = format!("{}.js", vf.module_rel_no_ext);
            let ext_ctx_js = build_external_import_ctx(
                &vf.prog,
                &out_rel_js,
                &vf.rel,
                root,
                &cfg.src_dir,
                &export_targets,
                EmitMode::JavaScript,
            );
            let ext_ctx_ts = build_external_import_ctx(
                &vf.prog,
                &out_rel_js,
                &vf.rel,
                root,
                &cfg.src_dir,
                &export_targets,
                EmitMode::TypeScript,
            );
            let mut prog_for_emit = vf.prog;
            if prog_for_emit.header.is_none() {
                prog_for_emit.header = Some(fallback_header);
            }
            let opts = CodegenOptions {
                source_file: vf.rel,
                source_map_comments: cfg.source_map_comments,
                custom_components: registered,
                bound_block_ids: vf.bound_block_ids,
            };
            let js = emit_code_with_context(&prog_for_emit, &opts, &ext_ctx_js, EmitMode::JavaScript);
            write_generated_file(&js_dir.join(&out_rel_js), &js)?;

            if cfg.emit_ts {
                let ts = emit_code_with_context(&prog_for_emit, &opts, &ext_ctx_ts, EmitMode::TypeScript);
                write_generated_file(&ts_dir.join(format!("{}.ts", vf.module_rel_no_ext)), &ts)?;
            }

            collected_programs.push(prog_for_emit);
            main_modules.push(vf.module_rel_no_ext);
            continue;
        }

        let header = vf.prog.header.clone().unwrap_or(fallback_header);
        let has_shared = !part.shared.is_empty();
        let shared_slug = vf.module_rel_no_ext.replace('/', "_");
        let shared_js_rel = format!("_shared/{shared_slug}.shared.js");
        let mut shared_ctx_js = SharedContext::from_shared_items(
            &part.shared,
            has_shared.then(|| format!("../_shared/{shared_slug}.shared.js")),
        );
        let mut shared_ctx_ts = SharedContext::from_shared_items(
            &part.shared,
            has_shared.then(|| format!("../_shared/{shared_slug}.shared")),
        );

        let ext_for_shared_js = build_external_import_ctx(
            &vf.prog,
            &shared_js_rel,
            &vf.rel,
            root,
            &cfg.src_dir,
            &export_targets,
            EmitMode::JavaScript,
        );
        let ext_for_shared_ts = build_external_import_ctx(
            &vf.prog,
            &shared_js_rel,
            &vf.rel,
            root,
            &cfg.src_dir,
            &export_targets,
            EmitMode::TypeScript,
        );
        // For items/, blocks/, world/, commands/ (all 1-dir deep under generated/)
        let ext_for_bucket_js = build_external_import_ctx(
            &vf.prog,
            &format!("world/{}.js", vf.stem),
            &vf.rel,
            root,
            &cfg.src_dir,
            &export_targets,
            EmitMode::JavaScript,
        );
        let ext_for_bucket_ts = build_external_import_ctx(
            &vf.prog,
            &format!("world/{}.js", vf.stem),
            &vf.rel,
            root,
            &cfg.src_dir,
            &export_targets,
            EmitMode::TypeScript,
        );
        for (sym, (p, kind)) in ext_for_bucket_js.external_symbols {
            shared_ctx_js.external_symbols.insert(sym, (p, kind));
        }
        shared_ctx_js.const_strings.extend(ext_for_bucket_js.const_strings);
        for (sym, (p, kind)) in ext_for_bucket_ts.external_symbols {
            shared_ctx_ts.external_symbols.insert(sym, (p, kind));
        }
        shared_ctx_ts.const_strings.extend(ext_for_bucket_ts.const_strings);

        if has_shared {
            let shared_js = emit_shared_module_with_externals(
                &part.shared,
                &ext_for_shared_js,
                Some(&header),
                &vf.rel,
                cfg.source_map_comments,
                EmitMode::JavaScript,
            );
            write_generated_file(&js_dir.join(&shared_js_rel), &shared_js)?;
            if cfg.emit_ts {
                let shared_ts = emit_shared_module_with_externals(
                    &part.shared,
                    &ext_for_shared_ts,
                    Some(&header),
                    &vf.rel,
                    cfg.source_map_comments,
                    EmitMode::TypeScript,
                );
                write_generated_file(
                    &ts_dir.join(format!("_shared/{shared_slug}.shared.ts")),
                    &shared_ts,
                )?;
            }
        }

        for target in part.items.into_values() {
            let group = merged_items
                .entry(target.entry.slug.clone())
                .or_insert_with(|| MergedComponentGroup {
                    entry: target.entry.clone(),
                    header: Some(header.clone()),
                    primary_source: vf.rel.clone(),
                    handlers: Vec::new(),
                    handlers_ts: Vec::new(),
                });
            for h in target.handlers {
                group.handlers.push((h.clone(), shared_ctx_js.clone(), vf.rel.clone()));
                group.handlers_ts.push((h, shared_ctx_ts.clone(), vf.rel.clone()));
            }
        }

        for target in part.blocks.into_values() {
            let group = merged_blocks
                .entry(target.entry.slug.clone())
                .or_insert_with(|| MergedComponentGroup {
                    entry: target.entry.clone(),
                    header: Some(header.clone()),
                    primary_source: vf.rel.clone(),
                    handlers: Vec::new(),
                    handlers_ts: Vec::new(),
                });
            for h in target.handlers {
                group.handlers.push((h.clone(), shared_ctx_js.clone(), vf.rel.clone()));
                group.handlers_ts.push((h, shared_ctx_ts.clone(), vf.rel.clone()));
            }
        }

        if !part.commands.is_empty() {
            let cmd_js = emit_commands_module(
                &part.commands,
                &shared_ctx_js,
                Some(&header),
                &vf.rel,
                cfg.source_map_comments,
                EmitMode::JavaScript,
            );
            write_generated_file(&js_dir.join(format!("commands/{}.js", vf.stem)), &cmd_js)?;
            if cfg.emit_ts {
                let cmd_ts = emit_commands_module(
                    &part.commands,
                    &shared_ctx_ts,
                    Some(&header),
                    &vf.rel,
                    cfg.source_map_comments,
                    EmitMode::TypeScript,
                );
                write_generated_file(&ts_dir.join(format!("commands/{}.ts", vf.stem)), &cmd_ts)?;
            }
            command_modules.push(vf.stem.clone());
        }

        if !part.world.is_empty() {
            let world_js = emit_world_module(
                &part.world,
                &shared_ctx_js,
                Some(&header),
                &vf.rel,
                cfg.source_map_comments,
                EmitMode::JavaScript,
            );
            write_generated_file(
                &js_dir.join(format!("world/{}.js", vf.stem)),
                &world_js,
            )?;
            if cfg.emit_ts {
                let world_ts = emit_world_module(
                    &part.world,
                    &shared_ctx_ts,
                    Some(&header),
                    &vf.rel,
                    cfg.source_map_comments,
                    EmitMode::TypeScript,
                );
                write_generated_file(
                    &ts_dir.join(format!("world/{}.ts", vf.stem)),
                    &world_ts,
                )?;
            }
            main_modules.push(format!("world/{}", vf.stem));
        }
        collected_programs.push(vf.prog);
    }

    let mut registry_entries: Vec<RegistryEntry> = Vec::new();

    for group in merged_items.into_values() {
        wire_target_json(&group.entry.json_path, &group.entry.component_id)?;
        let item_js = emit_item_component_module_grouped(
            &group.handlers,
            group.header.as_ref(),
            &group.primary_source,
            cfg.source_map_comments,
            EmitMode::JavaScript,
        );
        write_generated_file(
            &js_dir.join(format!("items/{}.js", group.entry.slug)),
            &item_js,
        )?;
        if cfg.emit_ts {
            let item_ts = emit_item_component_module_grouped(
                &group.handlers_ts,
                group.header.as_ref(),
                &group.primary_source,
                cfg.source_map_comments,
                EmitMode::TypeScript,
            );
            write_generated_file(
                &ts_dir.join(format!("items/{}.ts", group.entry.slug)),
                &item_ts,
            )?;
        }
        registry_entries.push(RegistryEntry {
            component_id: group.entry.component_id,
            kind: ComponentKind::Item,
            slug: group.entry.slug,
        });
    }

    for group in merged_blocks.into_values() {
        wire_target_json(&group.entry.json_path, &group.entry.component_id)?;
        let block_js = emit_block_component_module_grouped(
            &group.handlers,
            group.header.as_ref(),
            &group.primary_source,
            cfg.source_map_comments,
            EmitMode::JavaScript,
        );
        write_generated_file(
            &js_dir.join(format!("blocks/{}.js", group.entry.slug)),
            &block_js,
        )?;
        if cfg.emit_ts {
            let block_ts = emit_block_component_module_grouped(
                &group.handlers_ts,
                group.header.as_ref(),
                &group.primary_source,
                cfg.source_map_comments,
                EmitMode::TypeScript,
            );
            write_generated_file(
                &ts_dir.join(format!("blocks/{}.ts", group.entry.slug)),
                &block_ts,
            )?;
        }
        registry_entries.push(RegistryEntry {
            component_id: group.entry.component_id,
            kind: ComponentKind::Block,
            slug: group.entry.slug,
        });
    }

    if !registry_entries.is_empty() || !command_modules.is_empty() {
        let profile = ApiProfile::resolve_or_default(&api_ver);
        let reg_js = emit_registry_module_for_profile(
            &registry_entries,
            &command_modules,
            profile,
            EmitMode::JavaScript,
        );
        write_generated_file(&js_dir.join("_registry.js"), &reg_js)?;
        if cfg.emit_ts {
            let reg_ts = emit_registry_module_for_profile(
                &registry_entries,
                &command_modules,
                profile,
                EmitMode::TypeScript,
            );
            write_generated_file(&ts_dir.join("_registry.ts"), &reg_ts)?;
        }
        main_modules.push("_registry".to_string());
    }

    if !main_modules.is_empty() {
        let active_profile = ApiProfile::resolve_or_default(&api_ver);
        let effective_target = if active_profile == ApiProfile::Legacy1x
            && resolve_min_engine_version(&target_ver).is_ok_and(|v| v[1] >= 26)
        {
            active_profile.default_target_version().to_string()
        } else {
            target_ver.clone()
        };
        update_main_js(root, &main_modules)?;
        update_manifest_with_ui(root, &effective_target, &api_ver, workspace_uses_ui)?;
    }

    sort_and_dedup_diags(&mut all_diags);
    Ok((all_diags, collected_programs))
}

fn sanitize_mcfunction_body(raw_body: &str) -> String {
    let mut lines = Vec::new();
    for line in raw_body.lines() {
        let stripped = strip_mcfunction_line_comment(line).trim();
        if !stripped.is_empty() {
            lines.push(stripped);
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn strip_mcfunction_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut in_quote = false;
    while i < bytes.len() {
        if bytes[i] == b'\\' && in_quote {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn write_generated_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(path, content)
        .map_err(|e| format!("Failed to write '{}': {e}", path.display()))
}

fn clean_generated_dir(dir: &Path, ext: &str) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    clean_generated_recursive(dir, ext, true)
}

fn clean_generated_recursive(dir: &Path, ext: &str, is_root: bool) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let p = entry.path();
        if p.is_dir() {
            clean_generated_recursive(&p, ext, false)?;
            if fs::read_dir(&p).is_ok_and(|mut it| it.next().is_none()) {
                let _ = fs::remove_dir(&p);
            }
        } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
            fs::remove_file(&p)
                .map_err(|e| format!("Failed to remove stale file '{}': {e}", p.display()))?;
        }
    }
    let _ = is_root;
    Ok(())
}

pub fn check_path(
    path: &Path,
    workspace_root: Option<&Path>,
    spec: &BedrockSpec,
) -> Result<Vec<Diagnostic>, String> {
    let owned_index = workspace_root.map(WorkspaceAssetIndex::build);
    check_path_with_index(path, owned_index.as_ref(), spec)
}

pub fn check_path_with_index(
    path: &Path,
    index: Option<&WorkspaceAssetIndex>,
    spec: &BedrockSpec,
) -> Result<Vec<Diagnostic>, String> {
    let (diags, _) = check_path_and_collect(path, index, spec)?;
    Ok(diags)
}

fn check_path_and_collect(
    path: &Path,
    index: Option<&WorkspaceAssetIndex>,
    spec: &BedrockSpec,
) -> Result<(Vec<Diagnostic>, Vec<Program>), String> {
    let files = collect_pstdr_files(path)?;
    let workspace_root = index.map(|i| i.workspace_root.as_path());
    let owned_cfg;
    let cfg = if let Some(idx) = index {
        &idx.config
    } else {
        owned_cfg = WorkspaceConfig::default();
        &owned_cfg
    };

    let mut parsed = Vec::with_capacity(files.len());
    for f in &files {
        let source = fs::read_to_string(f)
            .map_err(|e| format!("Failed to read '{}': {e}", f.display()))?;
        let label = workspace_root
            .and_then(|r| f.strip_prefix(r).ok())
            .unwrap_or(f)
            .display()
            .to_string();
        let (prog_opt, diags) = parse_source(&source, &label);
        parsed.push((prog_opt, diags));
    }

    let inline_mcfunctions = collect_inline_mcfunctions_from_programs(
        parsed.iter().filter_map(|(p, _)| p.as_ref()),
    );

    let mut all_diags = Vec::new();
    let mut programs = Vec::with_capacity(parsed.len());

    for (prog_opt, mut diags) in parsed {
        if let Some(prog) = prog_opt {
            let vctx = ValidationContext {
                workspace_root,
                spec,
                strict_vanilla_ids: cfg.strict_vanilla_ids,
                asset_index: index,
                workspace_mcfunctions: Some(&inline_mcfunctions),
            };
            let (_info, val_diags) = validate_program(&prog, &vctx);
            diags.extend(val_diags);
            programs.push(prog);
        }
        sort_and_dedup_diags(&mut diags);
        all_diags.extend(diags);
    }

    sort_and_dedup_diags(&mut all_diags);
    Ok((all_diags, programs))
}

pub fn fmt_path(path: &Path, check_only: bool) -> Result<(usize, Vec<Diagnostic>), String> {
    let files = collect_pstdr_files(path)?;
    let mut changed = 0usize;
    let mut all_diags = Vec::new();

    for f in files {
        let source = fs::read_to_string(&f)
            .map_err(|e| format!("Failed to read '{}': {e}", f.display()))?;
        let label = f.display().to_string();
        let (_prog, parse_diags) = parse_source(&source, &label);
        if has_errors(&parse_diags) {
            all_diags.extend(parse_diags);
            continue;
        }
        match format_source(&source, &label) {
            Ok(formatted) => {
                if formatted != source {
                    changed += 1;
                    if !check_only {
                        fs::write(&f, formatted)
                            .map_err(|e| format!("Failed to write '{}': {e}", f.display()))?;
                    }
                }
            }
            Err(diags) => all_diags.extend(diags),
        }
    }

    Ok((changed, all_diags))
}

pub fn audit_workspace(root: &Path, spec: &BedrockSpec) -> Result<Vec<Diagnostic>, String> {
    let index = WorkspaceAssetIndex::build(root);
    audit_workspace_with_index(root, &index, spec)
}

pub fn audit_workspace_with_index(
    root: &Path,
    index: &WorkspaceAssetIndex,
    spec: &BedrockSpec,
) -> Result<Vec<Diagnostic>, String> {
    let cfg = &index.config;
    let src_dir = root.join(&cfg.src_dir);
    let scan_target = if src_dir.is_dir() { src_dir } else { root.to_path_buf() };
    let (mut diags, parsed_programs) =
        check_path_and_collect(&scan_target, Some(index), spec)?;

    let rest = audit_workspace_with_collected(root, index, spec, &parsed_programs, true)?;
    diags.extend(rest);
    sort_and_dedup_diags(&mut diags);
    Ok(diags)
}

fn audit_workspace_with_collected(
    root: &Path,
    index: &WorkspaceAssetIndex,
    spec: &BedrockSpec,
    parsed_programs: &[Program],
    run_asset_refs: bool,
) -> Result<Vec<Diagnostic>, String> {
    let cfg = &index.config;
    let mut diags = Vec::new();

    if run_asset_refs && cfg.strict_asset_refs {
        diags.extend(validate_workspace_assets_with_index(index, spec));
    }
    diags.extend(validate_bedrock_schema_gates(index));

    let bp_manifest = root.join("BP/manifest.json");
    let mut bp_uuid: Option<String> = None;
    let mut manifest_server_api: Option<String> = None;

    if bp_manifest.is_file() {
        let raw = fs::read_to_string(&bp_manifest)
            .map_err(|e| format!("Failed to read BP/manifest.json: {e}"))?;
        match serde_json::from_str::<Value>(&raw) {
            Ok(val) => {
                manifest_server_api = val
                    .get("dependencies")
                    .and_then(|d| d.as_array())
                    .and_then(|deps| {
                        deps.iter().find_map(|dep| {
                            let is_server = dep.get("module_name").and_then(|m| m.as_str())
                                == Some("@minecraft/server");
                            if is_server {
                                dep.get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                    });
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
                            "Audit Gate failed: manifest format_version: 3 requires 'min_engine_version' to be a SemVer string \"1.x.x\".",
                            "Set \"min_engine_version\": \"1.26.60\" in the BP/manifest.json header. The first number must be 1 (e.g. \"1.26.60\").",
                        ));
                    } else {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E015,
                            &Span::file_only("BP/manifest.json"),
                            "Audit Gate failed: 'min_engine_version' must be formatted as [1, x, x].",
                            "Set \"min_engine_version\": [1, 26, 60] in the BP/manifest.json header. \
                             The first number must be 1, whether targeting 26.60 (e.g. [1, 26, 60]) \
                             or legacy versions (e.g. [1, 21, 50]).",
                        ));
                    }
                }
            }
            Err(e) => {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E015,
                    &Span::file_only("BP/manifest.json"),
                    format!("Audit Gate failed: BP/manifest.json is invalid ({e})."),
                    "Fix the JSON syntax in BP/manifest.json.",
                ));
            }
        }
    }

    if let Some(raw) = &index.rp_manifest_raw {
        if let Ok(val) = serde_json::from_str::<Value>(raw) {
            let rp_uuid = val
                .get("header")
                .and_then(|h| h.get("uuid"))
                .and_then(|u| u.as_str());
            if let (Some(bp), Some(rp)) = (bp_uuid.as_deref(), rp_uuid) {
                if bp == rp {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::E015,
                        &Span::file_only("RP/manifest.json"),
                        format!("Audit Gate failed: BP and RP header UUIDs collide ('{bp}')."),
                        "Generate a unique UUID v4 for each of the BP and RP headers.",
                    ));
                }
            }
        }
    }

    if let Some(api_ver) = manifest_server_api.as_deref() {
        let mut declared_api: Option<String> = None;
        for prog in parsed_programs {
            if let Some(h) = &prog.header {
                let is_cur_beta = declared_api
                    .as_deref()
                    .is_some_and(crate::spec::is_beta_api_version);
                if !is_cur_beta || crate::spec::is_beta_api_version(&h.api) {
                    declared_api = Some(h.api.clone());
                }
            }
        }
        if let Some(addon_api) = declared_api.as_deref() {
            if api_ver != addon_api {
                diags.push(Diagnostic::error(
                    DiagnosticCode::E015,
                    &Span::file_only("BP/manifest.json"),
                    format!(
                        "Audit Gate failed: BP/manifest.json declares '@minecraft/server' '{api_ver}', but .pstdr '@addon' declares '{addon_api}'."
                    ),
                    "BP/manifest.json is out of sync with @addon(...) — run `pstdr-c convert --workspace` or `pstdr-c build --workspace` first.",
                ));
            }
        }
        if !crate::spec::is_beta_api_version(api_ver) {
            let audit_profile = ApiProfile::resolve_or_default(api_ver);
            if audit_profile == ApiProfile::Legacy1x {
                let legacy_spec = BedrockSpec::for_profile(ApiProfile::Legacy1x);
                audit_beta_api_usage(root, cfg, parsed_programs, api_ver, &legacy_spec, &mut diags)?;
            } else {
                audit_beta_api_usage(root, cfg, parsed_programs, api_ver, spec, &mut diags)?;
            }
        }
    }

    sort_and_dedup_diags(&mut diags);
    Ok(diags)
}

fn audit_beta_api_usage(
    root: &Path,
    cfg: &WorkspaceConfig,
    parsed_programs: &[Program],
    manifest_api: &str,
    spec: &BedrockSpec,
    diags: &mut Vec<Diagnostic>,
) -> Result<(), String> {
    let mut beta_events: Vec<&String> = spec.beta_events.iter().collect();
    beta_events.sort();
    let mut beta_apis: Vec<&String> = spec.beta_apis.iter().collect();
    beta_apis.sort();

    // Precompute exact substring/token needles once so scanning hundreds of generated JS files is O(bytes).
    let event_needles: Vec<(&str, Vec<String>)> = beta_events
        .iter()
        .map(|ev| {
            let ev_lower = ev.trim().to_ascii_lowercase();
            let needles = if let Some(name) = ev_lower.strip_prefix("before:") {
                vec![format!("beforeevents.{name}")]
            } else if let Some(name) = ev_lower.strip_prefix("after:") {
                vec![format!("afterevents.{name}")]
            } else {
                vec![format!("beforeevents.{ev_lower}"), format!("afterevents.{ev_lower}")]
            };
            (ev.as_str(), needles)
        })
        .collect();

    let api_needles: Vec<(&str, Option<&str>)> = beta_apis
        .iter()
        .map(|sym| {
            let member = sym.split_once('.').map(|(_, m)| m);
            (sym.as_str(), member)
        })
        .collect();

    // 1. Scan runtime JS outputs under BP/scripts/ (skip build/ts/ mirrors to avoid duplicate reports).
    let js_dir = root.join(&cfg.js_out_dir);
    let mut js_files = Vec::new();
    if js_dir.is_dir() {
        collect_files_by_ext(&js_dir, "js", &mut js_files)?;
        js_files.sort();
    }

    for path in &js_files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let content_lower = content.to_ascii_lowercase();
        let file_might_have_event = (content_lower.contains("beforeevents.")
            || content_lower.contains("afterevents."))
            && event_needles
                .iter()
                .any(|(_, needles)| needles.iter().any(|n| content_lower.contains(n.as_str())));
        let file_might_have_api = api_needles.iter().any(|(sym, member)| {
            content.contains(sym) || member.is_some_and(|m| content.contains(m))
        });
        if !file_might_have_event && !file_might_have_api {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        for (idx, line) in content.lines().enumerate() {
            if file_might_have_event {
                for beta_ev in &beta_events {
                    if crate::spec::text_uses_beta_event(line, beta_ev) {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E018,
                            &Span::new(&rel, idx + 1, 1, line.len().max(1)),
                            format!(
                                "Audit Gate failed: '{rel}' uses Beta-only event '{beta_ev}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'."
                            ),
                            "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only event.",
                        ));
                    }
                }
            }
            if file_might_have_api {
                for (beta_sym, member) in &api_needles {
                    let quick_hit = line.contains(beta_sym)
                        || member.is_some_and(|m| line.contains(m));
                    if quick_hit && crate::spec::text_uses_beta_api(line, beta_sym) {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E018,
                            &Span::new(&rel, idx + 1, 1, line.len().max(1)),
                            format!(
                                "Audit Gate failed: '{rel}' uses Beta-only API '{beta_sym}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'."
                            ),
                            "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only API call.",
                        ));
                    }
                }
            }
        }
    }

    // 2. If no generated JS files exist yet, also inspect .pstdr sources against BP/manifest.json
    // (catches files that declared `-beta` in `@addon` before `convert` updated the manifest).
    if js_files.is_empty() {
        for prog in parsed_programs {
            for item in &prog.items {
                match item {
                    TopLevelItem::EventHandler(ev)
                        if spec.is_beta_event("chatSend")
                            && matches!(ev.kind, EventKind::ChatSend { .. }) =>
                    {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E018,
                            &ev.span,
                            format!(
                                "Audit Gate failed: 'on chat' uses Beta-only 'chatSend', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'."
                            ),
                            "Re-run 'pstdr-c convert --workspace' with '@addon(api = \"2.11.0-beta\")' so BP/manifest.json uses a '-beta' version, or replace 'on chat' with stable interactions.",
                        ));
                    }
                    TopLevelItem::EventHandler(ev)
                        if spec.is_beta_before_event("playerPlaceBlock")
                            && matches!(ev.kind, EventKind::PlaceBlock { .. })
                            && block_contains_cancel(&ev.body) =>
                    {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::E018,
                            &ev.span,
                            format!(
                                "Audit Gate failed: 'on place' with 'cancel event' uses Beta-only 'beforeEvents.playerPlaceBlock', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'."
                            ),
                            "Remove 'cancel event' to use stable 'afterEvents.playerPlaceBlock', or re-run 'pstdr-c convert --workspace' with '@addon(api = \"2.11.0-beta\")' and enable Beta APIs.",
                        ));
                    }
                    TopLevelItem::UnsafeFuncDecl(u) => {
                        for beta_ev in &beta_events {
                            if crate::spec::text_uses_beta_event(&u.raw_body, beta_ev) {
                                diags.push(Diagnostic::error(
                                    DiagnosticCode::E018,
                                    &u.span,
                                    format!(
                                        "Audit Gate failed: unsafe function '{}' uses Beta-only event '{beta_ev}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'.",
                                        u.name
                                    ),
                                    "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only event.",
                                ));
                            }
                        }
                        for beta_sym in &beta_apis {
                            if crate::spec::text_uses_beta_api(&u.raw_body, beta_sym) {
                                diags.push(Diagnostic::error(
                                    DiagnosticCode::E018,
                                    &u.span,
                                    format!(
                                        "Audit Gate failed: unsafe function '{}' uses Beta-only API '{beta_sym}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'.",
                                        u.name
                                    ),
                                    "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only API call.",
                                ));
                            }
                        }
                    }
                    TopLevelItem::UnsafeVarDecl(u) => {
                        for beta_ev in &beta_events {
                            if crate::spec::text_uses_beta_event(&u.raw_rest, beta_ev) {
                                diags.push(Diagnostic::error(
                                    DiagnosticCode::E018,
                                    &u.span,
                                    format!(
                                        "Audit Gate failed: unsafe variable '{}' uses Beta-only event '{beta_ev}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'.",
                                        u.name
                                    ),
                                    "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only event.",
                                ));
                            }
                        }
                        for beta_sym in &beta_apis {
                            if crate::spec::text_uses_beta_api(&u.raw_rest, beta_sym) {
                                diags.push(Diagnostic::error(
                                    DiagnosticCode::E018,
                                    &u.span,
                                    format!(
                                        "Audit Gate failed: unsafe variable '{}' uses Beta-only API '{beta_sym}', but BP/manifest.json declares stable '@minecraft/server' '{manifest_api}'.",
                                        u.name
                                    ),
                                    "Update '@minecraft/server' in BP/manifest.json (and '@addon(api = \"2.11.0-beta\")') to a '-beta' version and enable Beta APIs in world settings, or remove the Beta-only API call.",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn collect_files_by_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let p = entry.path();
        if p.is_dir() {
            collect_files_by_ext(&p, ext, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(p);
        }
    }
    Ok(())
}

pub fn init_workspace(root: &Path, pack_name: &str, spec: &BedrockSpec) -> Result<(), String> {
    fs::create_dir_all(root.join("src")).map_err(|e| format!("Failed to create src/: {e}"))?;
    fs::create_dir_all(root.join("BP/items")).map_err(|e| format!("Failed to create BP/items/: {e}"))?;
    fs::create_dir_all(root.join("BP/scripts/generated"))
        .map_err(|e| format!("Failed to create BP/scripts/generated/: {e}"))?;
    fs::create_dir_all(root.join("RP/models/entity"))
        .map_err(|e| format!("Failed to create RP/models/entity/: {e}"))?;

    let toml_content = format!(
        "[pack]\nname = \"{pack_name}\"\nbedrock_target = \"{}\"\nscript_api = \"{}\"\n\n\
         [paths]\nsrc_dir = \"src\"\njs_out_dir = \"BP/scripts/generated\"\nts_out_dir = \"build/ts\"\n\n\
         [compiler]\nemit_ts = true\nstrict_vanilla_ids = true\nsource_map_comments = true\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(root.join("pstdr.toml"), toml_content)
        .map_err(|e| format!("Failed to write pstdr.toml: {e}"))?;

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
    .map_err(|e| format!("Failed to write BP/manifest.json: {e}"))?;

    let sample_script = format!(
        "@addon(target = \"{}\", api = \"{}\")\n\n\
         on join of player {{\n    send \"§aWelcome to {pack_name}!\" to player\n}}\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(root.join("src/welcome.pstdr"), sample_script)
        .map_err(|e| format!("Failed to write src/welcome.pstdr: {e}"))?;

    Ok(())
}

pub fn new_script(root: &Path, name: &str, spec: &BedrockSpec) -> Result<PathBuf, String> {
    let clean = name.trim().trim_end_matches(".pstdr");
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).map_err(|e| format!("Failed to create src/: {e}"))?;
    let path = src_dir.join(format!("{clean}.pstdr"));
    if path.exists() {
        return Err(format!("File '{}' already exists.", path.display()));
    }
    let content = format!(
        "@addon(target = \"{}\", api = \"{}\")\n\n\
         const ITEM_ID = \"my_pack:{clean}\"\n\n\
         on right click with ITEM_ID by player {{\n    player.actionbar(\"§bUsing {clean}!\")\n}}\n",
        spec.bedrock_version, spec.script_api_version
    );
    fs::write(&path, content).map_err(|e| format!("Failed to write '{}': {e}", path.display()))?;
    Ok(path)
}

pub fn doctor_report(root: Option<&Path>, spec: &BedrockSpec) -> String {
    let mut out = String::new();
    out.push_str(&format!("pstdr-c doctor (v{})\n", env!("CARGO_PKG_VERSION")));
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
                if r.join("pstdr.toml").is_file() { "OK" } else { "missing (using defaults)" }
            ));
            out.push_str(&format!(
                "  BP/manifest.json : {}\n",
                if r.join("BP/manifest.json").is_file() { "OK" } else { "not created yet" }
            ));
        }
        None => {
            out.push_str("  Workspace root   : not detected (run in project root or use 'pstdr-c init')\n");
        }
    }
    out
}
