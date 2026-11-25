use pstdr_lang::{
    audit_workspace, check_path, convert_single_file, convert_workspace, diagnostics_to_json,
    doctor_report, explain_code, find_workspace_root, fmt_path, format_diagnostics_compact,
    format_diagnostics_pretty, has_errors, init_workspace, new_script, BedrockSpec, Diagnostic,
    DiagnosticCode, Span,
};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime};

const VERSION: &str = "0.1.0-alpha.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Pretty,
    Compact,
    Json,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run_cli(&args) {
        Ok(code) => ExitCode::from(code),
        Err((code, msg)) => {
            if !msg.is_empty() {
                eprintln!("{msg}");
            }
            ExitCode::from(code)
        }
    }
}

fn version_string() -> String {
    let spec = BedrockSpec::bundled();
    format!(
        "pstdr-c {VERSION} (spec: Bedrock {} / @minecraft/server {} / 1.15–1.19 LTS)",
        spec.bedrock_version, spec.script_api_version
    )
}

fn print_help() {
    println!(
        "pstdr-c v{VERSION} — Compiler CLI for the .pstdr language (PackStudio)\n\n\
         USAGE:\n\
         \x20 pstdr-c build --workspace [--format pretty|compact|json] [--spec <file>]\n\
         \x20                                                Run check -> convert -> audit sequentially\n\
         \x20 pstdr-c fmt <path> [--check]                  Format .pstdr code in a file/directory\n\
         \x20 pstdr-c fmt --workspace [--check]             Format all .pstdr files in the workspace\n\
         \x20 pstdr-c convert --from <in.pstdr> to <out.js> [--ts] [--spec <file>]\n\
         \x20 pstdr-c convert --workspace [--spec <file>]   Compile entire workspace to BP/ & build/ts/\n\
         \x20 pstdr-c check <path|--workspace> [--format pretty|compact|json] [--spec <file>]\n\
         \x20 pstdr-c watch [--workspace] [--spec <file>]   Watch & auto-compile on file changes\n\
         \x20 pstdr-c audit --workspace [--format pretty|compact|json] [--spec <file>]\n\
         \x20 pstdr-c explain <CODE>                        Detailed explanation of an error code (e.g. E002)\n\
         \x20 pstdr-c init [pack_name]                      Initialize a new PackStudio project\n\
         \x20 pstdr-c new <script_name>                     Create a template file src/<name>.pstdr\n\
         \x20 pstdr-c spec [info|dump] [--spec <file>]      Show / dump the active Bedrock specification\n\
         \x20 pstdr-c doctor [--spec <file>]                Environment & workspace diagnostics\n\n\
         EXIT CODES:\n\
         \x20 0 = Success | 1 = Compile/audit error | 2 = Invalid arguments/workspace | 3 = I/O error"
    );
}

fn parse_format_flag(args: &[String]) -> Result<OutputFormat, (u8, String)> {
    let mut fmt = OutputFormat::Pretty;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--format" {
            let val = args
                .get(i + 1)
                .ok_or_else(|| (2u8, "Flag --format requires a value (pretty|compact|json).".to_string()))?;
            fmt = match val.as_str() {
                "pretty" => OutputFormat::Pretty,
                "compact" => OutputFormat::Compact,
                "json" => OutputFormat::Json,
                other => {
                    return Err((
                        2,
                        format!("Unknown output format '{other}'. Use pretty, compact, or json."),
                    ))
                }
            };
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(fmt)
}

fn parse_spec_flag(args: &[String]) -> Result<Option<PathBuf>, (u8, String)> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--spec" {
            let val = args
                .get(i + 1)
                .ok_or_else(|| (2u8, "Flag --spec requires a JSON file path.".to_string()))?;
            return Ok(Some(PathBuf::from(val)));
        }
        i += 1;
    }
    Ok(None)
}

fn render_diagnostics(diags: &[Diagnostic], fmt: OutputFormat) {
    if diags.is_empty() {
        if fmt == OutputFormat::Json {
            println!("[]");
        }
        return;
    }
    match fmt {
        OutputFormat::Pretty => eprint!("{}", format_diagnostics_pretty(diags)),
        OutputFormat::Compact => eprint!("{}", format_diagnostics_compact(diags)),
        OutputFormat::Json => println!("{}", diagnostics_to_json(diags)),
    }
}

fn require_workspace(cwd: &Path, fmt: OutputFormat) -> Result<PathBuf, (u8, String)> {
    match find_workspace_root(cwd) {
        Some(root) => Ok(root),
        None => {
            let d = Diagnostic::error(
                DiagnosticCode::E014,
                &Span::file_only("."),
                "PackStudio workspace not detected (BP/ or pstdr.toml not found).",
                "Run from the Add-On project root, run 'pstdr-c init', or use 'pstdr-c convert --from <file> to <out>'.",
            );
            render_diagnostics(&[d], fmt);
            Err((2, String::new()))
        }
    }
}

fn run_cli(args: &[String]) -> Result<u8, (u8, String)> {
    let cwd = env::current_dir().map_err(|e| (3u8, format!("Failed to read current directory: {e}")))?;
    run_cli_in(&cwd, args)
}

fn run_cli_in(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return Ok(0);
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("{}", version_string());
        return Ok(0);
    }

    let sub = args[0].as_str();
    let rest = &args[1..];

    match sub {
        "build" => cmd_build(cwd, rest),
        "fmt" => cmd_fmt(cwd, rest),
        "convert" => cmd_convert(cwd, rest),
        "check" => cmd_check(cwd, rest),
        "watch" => cmd_watch(cwd, rest),
        "audit" => cmd_audit(cwd, rest),
        "explain" => cmd_explain(rest),
        "init" => cmd_init(cwd, rest),
        "new" => cmd_new(cwd, rest),
        "spec" => cmd_spec(cwd, rest),
        "doctor" => cmd_doctor(cwd, rest),
        other => Err((
            2,
            format!("Unknown command '{other}'. Run 'pstdr-c --help' for the list of commands."),
        )),
    }
}

fn cmd_build(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let fmt = parse_format_flag(args)?;
    let custom_spec = parse_spec_flag(args)?;
    let root = require_workspace(cwd, fmt)?;
    let spec = BedrockSpec::load(custom_spec.as_deref(), Some(&root)).map_err(|e| (3u8, e))?;

    // Step 1: check
    let src = root.join("src");
    let check_target = if src.is_dir() { src } else { root.clone() };
    let check_diags = check_path(&check_target, Some(&root), &spec).map_err(|e| (3u8, e))?;
    if has_errors(&check_diags) {
        render_diagnostics(&check_diags, fmt);
        return Ok(1);
    }

    // Step 2: convert
    let convert_diags = convert_workspace(&root, &spec).map_err(|e| (3u8, e))?;
    if has_errors(&convert_diags) {
        render_diagnostics(&convert_diags, fmt);
        return Ok(1);
    }

    // Step 3: audit
    let audit_diags = audit_workspace(&root, &spec).map_err(|e| (3u8, e))?;
    render_diagnostics(&audit_diags, fmt);
    if has_errors(&audit_diags) {
        return Ok(1);
    }

    if fmt != OutputFormat::Json {
        println!("Build succeeded (check -> convert -> audit): 0 errors.");
    }
    Ok(0)
}

fn cmd_fmt(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let fmt = parse_format_flag(args)?;
    let check_only = args.iter().any(|a| a == "--check");
    let use_ws = args.iter().any(|a| a == "--workspace");

    let target = if use_ws {
        let root = require_workspace(cwd, fmt)?;
        let src = root.join("src");
        if src.is_dir() { src } else { root }
    } else {
        let pos = args
            .iter()
            .find(|a| !a.starts_with('-'))
            .ok_or_else(|| (2u8, "Usage: pstdr-c fmt <path> | --workspace [--check]".to_string()))?;
        PathBuf::from(pos)
    };

    let (changed, diags) = fmt_path(&target, check_only).map_err(|e| (3u8, e))?;
    render_diagnostics(&diags, fmt);
    if has_errors(&diags) {
        return Ok(1);
    }
    if check_only && changed > 0 {
        eprintln!("{changed} file(s) require formatting (run without --check).");
        return Ok(1);
    }
    if fmt != OutputFormat::Json {
        if check_only {
            println!("All .pstdr files are properly formatted.");
        } else {
            println!("Formatting complete ({changed} file(s) updated).");
        }
    }
    Ok(0)
}

fn cmd_convert(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let fmt = parse_format_flag(args)?;
    let custom_spec = parse_spec_flag(args)?;
    let use_ws = args.iter().any(|a| a == "--workspace");

    if use_ws {
        let root = require_workspace(cwd, fmt)?;
        let spec = BedrockSpec::load(custom_spec.as_deref(), Some(&root)).map_err(|e| (3u8, e))?;
        let diags = convert_workspace(&root, &spec).map_err(|e| (3u8, e))?;
        render_diagnostics(&diags, fmt);
        if has_errors(&diags) {
            return Ok(1);
        }
        if fmt != OutputFormat::Json {
            println!("Workspace conversion succeeded -> BP/scripts/generated/ & build/ts/");
        }
        return Ok(0);
    }

    let mut from_path: Option<PathBuf> = None;
    let mut to_path: Option<PathBuf> = None;
    let emit_ts = args.iter().any(|a| a == "--ts");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --from requires a .pstdr file path.".to_string()))?;
                from_path = Some(PathBuf::from(v));
                i += 2;
            }
            "to" | "--to" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Keyword 'to' requires an output file path.".to_string()))?;
                to_path = Some(PathBuf::from(v));
                i += 2;
            }
            "--spec" | "--format" => i += 2,
            _ => i += 1,
        }
    }

    let (Some(from), Some(to)) = (from_path, to_path) else {
        return Err((
            2,
            "Usage: pstdr-c convert --from <file.pstdr> to <file.js> [--ts] OR pstdr-c convert --workspace".to_string(),
        ));
    };

    let ws_root = find_workspace_root(&from);
    let spec = BedrockSpec::load(custom_spec.as_deref(), ws_root.as_deref()).map_err(|e| (3u8, e))?;
    let diags = convert_single_file(&from, &to, &spec, emit_ts).map_err(|e| (3u8, e))?;
    render_diagnostics(&diags, fmt);
    if has_errors(&diags) {
        return Ok(1);
    }
    if fmt != OutputFormat::Json {
        println!("Successfully compiled {} -> {}", from.display(), to.display());
    }
    Ok(0)
}

fn cmd_check(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let fmt = parse_format_flag(args)?;
    let custom_spec = parse_spec_flag(args)?;
    let use_ws = args.iter().any(|a| a == "--workspace");

    let (target, ws_root) = if use_ws {
        let root = require_workspace(cwd, fmt)?;
        let src = root.join("src");
        let t = if src.is_dir() { src } else { root.clone() };
        (t, Some(root))
    } else {
        let mut target_arg: Option<PathBuf> = None;
        let mut i = 0;
        while i < args.len() {
            if matches!(args[i].as_str(), "--format" | "--spec") {
                i += 2;
                continue;
            }
            if !args[i].starts_with('-') {
                target_arg = Some(PathBuf::from(&args[i]));
                break;
            }
            i += 1;
        }
        let t = target_arg.ok_or_else(|| {
            (2u8, "Usage: pstdr-c check <path|--workspace> [--format pretty|compact|json]".to_string())
        })?;
        let ws = find_workspace_root(&t);
        (t, ws)
    };

    let spec = BedrockSpec::load(custom_spec.as_deref(), ws_root.as_deref()).map_err(|e| (3u8, e))?;
    let diags = check_path(&target, ws_root.as_deref(), &spec).map_err(|e| (3u8, e))?;
    render_diagnostics(&diags, fmt);
    if has_errors(&diags) {
        return Ok(1);
    }
    if fmt != OutputFormat::Json {
        println!("Check passed: 0 errors.");
    }
    Ok(0)
}

fn cmd_watch(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let custom_spec = parse_spec_flag(args)?;
    let root = require_workspace(cwd, OutputFormat::Pretty)?;
    let spec = BedrockSpec::load(custom_spec.as_deref(), Some(&root)).map_err(|e| (3u8, e))?;
    let src_dir = if root.join("src").is_dir() {
        root.join("src")
    } else {
        root.clone()
    };

    println!("Watching .pstdr changes in '{}' (Ctrl+C to stop)...", src_dir.display());
    let mut last_state: HashMap<PathBuf, SystemTime> = HashMap::new();

    loop {
        let mut current: HashMap<PathBuf, SystemTime> = HashMap::new();
        if let Ok(files) = pstdr_lang::workspace::collect_pstdr_files(&src_dir) {
            for f in files {
                if let Ok(meta) = fs::metadata(&f) {
                    if let Ok(mtime) = meta.modified() {
                        current.insert(f, mtime);
                    }
                }
            }
        }

        if current != last_state {
            last_state = current;
            match convert_workspace(&root, &spec) {
                Ok(diags) => {
                    if has_errors(&diags) {
                        eprint!("{}", format_diagnostics_compact(&diags));
                    } else {
                        println!("[watch] Build succeeded.");
                    }
                }
                Err(e) => eprintln!("[watch] Error: {e}"),
            }
        }

        thread::sleep(Duration::from_millis(600));
    }
}

fn cmd_audit(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let fmt = parse_format_flag(args)?;
    let custom_spec = parse_spec_flag(args)?;
    let root = require_workspace(cwd, fmt)?;
    let spec = BedrockSpec::load(custom_spec.as_deref(), Some(&root)).map_err(|e| (3u8, e))?;
    let diags = audit_workspace(&root, &spec).map_err(|e| (3u8, e))?;
    render_diagnostics(&diags, fmt);
    if has_errors(&diags) {
        return Ok(1);
    }
    if fmt != OutputFormat::Json {
        println!("Audit Gate passed: all scripts and manifests are valid.");
    }
    Ok(0)
}

fn cmd_explain(args: &[String]) -> Result<u8, (u8, String)> {
    let code = args
        .first()
        .ok_or_else(|| (2u8, "Usage: pstdr-c explain <CODE> (e.g. pstdr-c explain E002)".to_string()))?;
    match explain_code(code) {
        Some(text) => {
            println!("{text}");
            Ok(0)
        }
        None => Err((2, format!("Unknown diagnostic code '{code}' (available: E001–E018)."))),
    }
}

fn cmd_init(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let pack_name = args.first().map(|s| s.as_str()).unwrap_or("PackStudio Add-On");
    let spec = BedrockSpec::bundled();
    init_workspace(cwd, pack_name, &spec).map_err(|e| (3u8, e))?;
    println!("Workspace '{pack_name}' initialized successfully at {}.", cwd.display());
    Ok(0)
}

fn cmd_new(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let name = args
        .first()
        .ok_or_else(|| (2u8, "Usage: pstdr-c new <script_name>".to_string()))?;
    let root = find_workspace_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let spec = BedrockSpec::load(None, Some(&root)).map_err(|e| (3u8, e))?;
    let created = new_script(&root, name, &spec).map_err(|e| (3u8, e))?;
    println!("Created new script: {}", created.display());
    Ok(0)
}

fn cmd_spec(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let custom_spec = parse_spec_flag(args)?;
    let ws_root = find_workspace_root(cwd);
    let spec = BedrockSpec::load(custom_spec.as_deref(), ws_root.as_deref()).map_err(|e| (3u8, e))?;
    let action = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or("info");
    match action {
        "info" => {
            println!("Spec source        : {}", spec.source_label);
            println!("Bedrock version    : {}", spec.bedrock_version);
            println!("@minecraft/server  : {}", spec.script_api_version);
            println!("Vanilla effects    : {}", spec.effects.len());
            println!("Vanilla sounds     : {}", spec.sounds.len());
            Ok(0)
        }
        "dump" => {
            println!("{}", pstdr_lang::spec::BUNDLED_SPEC_JSON);
            Ok(0)
        }
        other => Err((2, format!("Unknown spec subcommand '{other}'. Use 'info' or 'dump'."))),
    }
}

fn cmd_doctor(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let custom_spec = parse_spec_flag(args)?;
    let ws_root = find_workspace_root(cwd);
    let spec = BedrockSpec::load(custom_spec.as_deref(), ws_root.as_deref()).map_err(|e| (3u8, e))?;
    print!("{}", doctor_report(ws_root.as_deref(), &spec));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_tmp(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = env::temp_dir().join(format!("pstdr_cli_{tag}_{nanos}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn phase_c_explain_and_single_file_convert() {
        let code = run_cli(&["explain".into(), "E002".into()]).unwrap();
        assert_eq!(code, 0);

        let dir = mk_tmp("single");
        let src = dir.join("demo.pstdr");
        let out = dir.join("demo.js");
        fs::write(&src, "on join of player {\n    send \"Hello\" to player\n}\n").unwrap();

        let code = run_cli(&[
            "convert".into(),
            "--from".into(),
            src.display().to_string(),
            "to".into(),
            out.display().to_string(),
            "--ts".into(),
        ])
        .unwrap();
        assert_eq!(code, 0);
        assert!(out.is_file());
        assert!(out.with_extension("ts").is_file());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_c_fmt_check_and_error_exit_code() {
        let dir = mk_tmp("fmt");
        let f = dir.join("bad.pstdr");
        fs::write(&f, "on join of player {\n    const a = 1\n    a = 2\n}\n").unwrap();

        let code = run_cli(&[
            "check".into(),
            f.display().to_string(),
            "--format".into(),
            "json".into(),
        ])
        .unwrap();
        assert_eq!(code, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wave_3a_end_cli_build_workspace_and_version_string() {
        let ver = version_string();
        assert!(
            ver.contains(VERSION)
                && ver.contains("Bedrock 26.60")
                && ver.contains("@minecraft/server 2.11.0")
                && ver.contains("1.15–1.19 LTS"),
            "Unexpected version string: {ver}"
        );

        let dir = mk_tmp("build_ws");
        let spec = BedrockSpec::bundled();
        init_workspace(&dir, "BuildPack", &spec).unwrap();

        // 1. Clean workspace -> build --workspace succeeds (check -> convert -> audit)
        let code = run_cli_in(&dir, &["build".into(), "--workspace".into()]).unwrap();
        assert_eq!(code, 0, "pstdr-c build --workspace must succeed on valid workspace");
        assert!(
            dir.join("BP/scripts/generated/welcome.js").is_file(),
            "build --workspace must emit generated JS"
        );

        // 2. Introduce a check error -> build --workspace stops immediately at check (exit 1)
        fs::write(
            dir.join("src/broken.pstdr"),
            "on join of player {\n    const t = world.get_time()\n}\n",
        )
        .unwrap();
        let code_fail = run_cli_in(
            &dir,
            &[
                "build".into(),
                "--workspace".into(),
                "--format".into(),
                "json".into(),
            ],
        )
        .unwrap();
        assert_eq!(code_fail, 1, "build --workspace must fail with exit code 1 on check error");
        assert!(
            !dir.join("BP/scripts/generated/broken.js").exists(),
            "build --workspace must stop before emitting broken.js"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
