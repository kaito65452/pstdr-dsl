use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    E001,
    E002,
    E003,
    E004,
    E005,
    E006,
    E007,
    E008,
    E009,
    E010,
    E011,
    E012,
    E013,
    E014,
    E015,
    E016,
    E017,
}

impl DiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::E001 => "PSTDR-E001",
            Self::E002 => "PSTDR-E002",
            Self::E003 => "PSTDR-E003",
            Self::E004 => "PSTDR-E004",
            Self::E005 => "PSTDR-E005",
            Self::E006 => "PSTDR-E006",
            Self::E007 => "PSTDR-E007",
            Self::E008 => "PSTDR-E008",
            Self::E009 => "PSTDR-E009",
            Self::E010 => "PSTDR-E010",
            Self::E011 => "PSTDR-E011",
            Self::E012 => "PSTDR-E012",
            Self::E013 => "PSTDR-E013",
            Self::E014 => "PSTDR-E014",
            Self::E015 => "PSTDR-E015",
            Self::E016 => "PSTDR-E016",
            Self::E017 => "PSTDR-E017",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_column: usize,
}

impl Span {
    pub fn new(file: impl Into<String>, line: usize, column: usize, end_column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            end_column: end_column.max(column),
        }
    }

    pub fn file_only(file: impl Into<String>) -> Self {
        Self::new(file, 1, 1, 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_column: usize,
    pub message: String,
    pub solution: String,
}

impl Diagnostic {
    pub fn error(
        code: DiagnosticCode,
        span: &Span,
        message: impl Into<String>,
        solution: impl Into<String>,
    ) -> Self {
        Self {
            code: code.as_str().to_string(),
            severity: Severity::Error,
            file: span.file.clone(),
            line: span.line,
            column: span.column,
            end_column: span.end_column,
            message: message.into(),
            solution: solution.into(),
        }
    }

    pub fn warning(
        code: DiagnosticCode,
        span: &Span,
        message: impl Into<String>,
        solution: impl Into<String>,
    ) -> Self {
        Self {
            code: code.as_str().to_string(),
            severity: Severity::Warning,
            file: span.file.clone(),
            line: span.line,
            column: span.column,
            end_column: span.end_column,
            message: message.into(),
            solution: solution.into(),
        }
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

pub fn diagnostics_to_json(diags: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(diags).unwrap_or_else(|_| "[]".to_string())
}

pub fn format_diagnostics_pretty(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        out.push_str(&format!(
            "{}[{}]: {}\n  --> {}:{}:{}\n  help: {}\n\n",
            sev, d.code, d.message, d.file, d.line, d.column, d.solution
        ));
    }
    out
}

pub fn format_diagnostics_compact(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "ERR",
            Severity::Warning => "WARN",
        };
        out.push_str(&format!(
            "{}:{}:{}: {} [{}] {} (Help: {})\n",
            d.file, d.line, d.column, sev, d.code, d.message, d.solution
        ));
    }
    out
}

pub fn explain_code(raw_code: &str) -> Option<&'static str> {
    let norm = raw_code.trim().to_ascii_uppercase();
    let code = norm.strip_prefix("PSTDR-").unwrap_or(&norm);
    match code {
        "E001" => Some(
            "PSTDR-E001: Foreign-paradigm keyword or symbol in safe mode.\n\
             Cause: Using 'class', 'extends', 'super', 'new', 'this', 'constructor', 'var', 'let', 'function', 'nil', '~=', '..', 'then', 'end', 'import', 'export', 'require', or 'eval' outside an 'unsafe' block/declaration.\n\
             Wrong : let dmg = 5\n\
             Right : mut dmg = 5",
        ),
        "E002" => Some(
            "PSTDR-E002: Cannot reassign constant ('const') variable.\n\
             Cause: Performing assignment (=, +=, -=, *=, /=) on a variable declared with 'const'.\n\
             Wrong : const hp = 10\n        hp = 5\n\
             Right : mut hp = 10\n        hp = 5",
        ),
        "E003" => Some(
            "PSTDR-E003: Reserved keyword used as an identifier.\n\
             Cause: Using a reserved .pstdr keyword (const, mut, state, func, on, if, apply, etc.) as a variable, parameter, or function name.\n\
             Wrong : const if = 1\n\
             Right : const condition = 1",
        ),
        "E004" => Some(
            "PSTDR-E004: 'cancel event' used in a non-cancellable context.\n\
             Cause: Writing 'cancel event' inside 'func', 'after', 'every', 'on hit', or 'on join'.\n\
             Wrong : every 1s { cancel event }\n\
             Right : on break of \"minecraft:stone\" by player { cancel event }",
        ),
        "E005" => Some(
            "PSTDR-E005: Context variable is not available in this scope.\n\
             Cause: Accessing 'player' inside 'every 1s { ... }', or 'target' inside 'on right click'.\n\
             Wrong : every 1s { send \"hi\" to player }\n\
             Right : every 1s { for p in world.players() { send \"hi\" to p } }",
        ),
        "E006" => Some(
            "PSTDR-E006: 'unsafe' cannot be used as an expression value.\n\
             Cause: Writing e.g. 'const x = unsafe { 1 }'.\n\
             Wrong : const x = unsafe { 1 }\n\
             Right : unsafe get_raw() { return 1; }",
        ),
        "E007" => Some(
            "PSTDR-E007: Target file in bind_target() not found.\n\
             Cause: The relative path from the project root does not exist on disk.\n\
             Right : Make sure the file exists, e.g. bind_target(\"BP/items/frost_blade.json\")",
        ),
        "E008" => Some(
            "PSTDR-E008: Identifier mismatch between target JSON file and .pstdr script.\n\
             Cause: The \"identifier\" value in BP/items/*.json or BP/blocks/*.json differs from the item/block ID used in the script.\n\
             Right : Match the JSON \"identifier\" with the constant/string in your .pstdr event handler.",
        ),
        "E009" => Some(
            "PSTDR-E009: Path traversal or absolute path in bind_target().\n\
             Cause: Path contains '..' or starts with '/' / a drive letter.\n\
             Wrong : bind_target(\"../secret.json\")\n\
             Right : bind_target(\"BP/items/frost_blade.json\")",
        ),
        "E010" => Some(
            "PSTDR-E010: 'every' interval is too small (< 1 tick / 50ms).\n\
             Cause: 'every 0s' or 'every 10ms' can freeze the Bedrock tick loop.\n\
             Wrong : every 0s { ... }\n\
             Right : every 50ms { ... } or every 1s { ... }",
        ),
        "E011" => Some(
            "PSTDR-E011: Status effect level must be >= 1.\n\
             Cause: Effect levels in .pstdr are 1-based for readability (the compiler automatically subtracts 1 for Bedrock's amplifier).\n\
             Wrong : apply speed 0 to player for 3s\n\
             Right : apply speed 1 to player for 3s",
        ),
        "E012" => Some(
            "PSTDR-E012: Opening brace '{' has no matching closing brace '}'.\n\
             Cause: A function, event, or 'unsafe' block was not closed with '}'.\n\
             Right : Ensure every '{' has a matching '}'.",
        ),
        "E013" => Some(
            "PSTDR-E013: Vanilla effect/sound ID not found in the latest Bedrock specification.\n\
             Cause: Typo in effect name in 'apply' or vanilla sound name in 'play_sound'.\n\
             Wrong : apply slownes 2 to target for 3s\n\
             Right : apply slowness 2 to target for 3s",
        ),
        "E014" => Some(
            "PSTDR-E014: PackStudio workspace not detected.\n\
             Cause: Neither 'BP/' nor 'pstdr.toml' was found in the current directory.\n\
             Right : Run inside the Add-On project root or use 'pstdr-c convert --from <file> to <out>'.",
        ),
        "E015" => Some(
            "PSTDR-E015: Audit Gate failed.\n\
             Cause: 'min_engine_version' is not formatted as [1, x, x] (or \"1.x.x\" for v3), BP/RP UUIDs collide, or a forbidden entry was found.\n\
             Official Rule: The first element of min_engine_version MUST be 1 for all Bedrock versions,\n\
             including Bedrock 26.60 (use [1, 26, 60]) as well as older versions (e.g. [1, 21, 50]).\n\
             Format [26, 60, 0] is INVALID — the first number is always 1.\n\
             Right : Update BP/manifest.json to have \"min_engine_version\": [1, 26, 60] (for 26.60)\n\
                     or [1, 21, 50] (for 1.21.50 / legacy).",
        ),
        "E016" => Some(
            "PSTDR-E016: General syntax error.\n\
             Cause: Unexpected token, unclosed quote, or incomplete .pstdr statement structure.\n\
             Right : Check the highlighted line and fix it to match .pstdr grammar.",
        ),
        "E017" => Some(
            "PSTDR-E017: Invalid texture reference or missing image file.\n\
             Cause: An item/block JSON file in BP references a custom texture alias without a Resource Pack (RP/),\n\
             the alias is not registered in RP/textures/item_texture.json or terrain_texture.json,\n\
             or the physical image file (.png/.tga) is missing inside RP/.\n\
             Forbidden formats: alias cannot be empty, end with .png/.tga/.jpg, or contain paths ('/', '\\') or namespaces (':').\n\
             Right : Create RP/textures/item_texture.json along with the corresponding image file, or use a valid vanilla alias (e.g. 'snowball', 'diamond').",
        ),
        _ => None,
    }
}
