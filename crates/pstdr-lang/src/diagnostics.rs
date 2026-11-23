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
            "{}[{}]: {}\n  --> {}:{}:{}\n  solusi: {}\n\n",
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
            "{}:{}:{}: {} [{}] {} (Solusi: {})\n",
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
            "PSTDR-E001: Kata kunci atau simbol paradigma asing di mode aman.\n\
             Penyebab: Menggunakan 'class', 'extends', 'super', 'new', 'this', 'constructor', 'var', 'let', 'function', 'nil', '~=', '..', 'then', 'end', 'import', 'export', 'require', atau 'eval' di luar blok/deklarasi 'unsafe'.\n\
             Salah : let dmg = 5\n\
             Benar : mut dmg = 5",
        ),
        "E002" => Some(
            "PSTDR-E002: Tidak dapat mengubah variabel konstan ('const').\n\
             Penyebab: Melakukan assignment (=, +=, -=, *=, /=) ke variabel yang dideklarasikan dengan 'const'.\n\
             Salah : const hp = 10\n        hp = 5\n\
             Benar : mut hp = 10\n        hp = 5",
        ),
        "E003" => Some(
            "PSTDR-E003: Reserved keyword dipakai sebagai identifier.\n\
             Penyebab: Memakai kata cadangan .pstdr (const, mut, state, func, on, if, apply, dll.) sebagai nama variabel, parameter, atau fungsi.\n\
             Salah : const if = 1\n\
             Benar : const kondisi = 1",
        ),
        "E004" => Some(
            "PSTDR-E004: 'cancel event' di konteks non-cancellable.\n\
             Penyebab: Menulis 'cancel event' di dalam 'func', 'after', 'every', 'on hit', atau 'on join'.\n\
             Salah : every 1s { cancel event }\n\
             Benar : on break of \"minecraft:stone\" by player { cancel event }",
        ),
        "E005" => Some(
            "PSTDR-E005: Variabel konteks tidak tersedia di scope ini.\n\
             Penyebab: Mengakses 'player' di dalam 'every 1s { ... }', atau 'target' di dalam 'on right click'.\n\
             Salah : every 1s { send \"hi\" to player }\n\
             Benar : every 1s { for p in world.players() { send \"hi\" to p } }",
        ),
        "E006" => Some(
            "PSTDR-E006: 'unsafe' tidak boleh dipakai sebagai nilai ekspresi.\n\
             Penyebab: Menulis misalnya 'const x = unsafe { 1 }'.\n\
             Salah : const x = unsafe { 1 }\n\
             Benar : unsafe get_raw() { return 1; }",
        ),
        "E007" => Some(
            "PSTDR-E007: File target pada bind_target() tidak ditemukan.\n\
             Penyebab: Path relatif dari root proyek tidak ada di disk.\n\
             Benar : Pastikan file ada di Explorer, mis. bind_target(\"BP/items/frost_blade.json\")",
        ),
        "E008" => Some(
            "PSTDR-E008: Ketidakcocokan identifier antara file JSON target dan skrip .pstdr.\n\
             Penyebab: Nilai \"identifier\" di BP/items/*.json atau BP/blocks/*.json berbeda dengan ID item/blok di skrip.\n\
             Benar : Samakan \"identifier\" JSON dengan konstanta/string di event .pstdr.",
        ),
        "E009" => Some(
            "PSTDR-E009: Path traversal atau path absolut pada bind_target().\n\
             Penyebab: Path mengandung '..' atau diawali '/' / huruf drive.\n\
             Salah : bind_target(\"../secret.json\")\n\
             Benar : bind_target(\"BP/items/frost_blade.json\")",
        ),
        "E010" => Some(
            "PSTDR-E010: Interval 'every' terlalu kecil (< 1 tick / 50ms).\n\
             Penyebab: 'every 0s' atau 'every 10ms' dapat membekukan tick loop Bedrock.\n\
             Salah : every 0s { ... }\n\
             Benar : every 50ms { ... } atau every 1s { ... }",
        ),
        "E011" => Some(
            "PSTDR-E011: Level efek status harus >= 1.\n\
             Penyebab: Level efek di .pstdr berbasis manusia mulai dari 1 (compiler otomatis mengurangi 1 untuk amplifier Bedrock).\n\
             Salah : apply speed 0 to player for 3s\n\
             Benar : apply speed 1 to player for 3s",
        ),
        "E012" => Some(
            "PSTDR-E012: Kurung kurawal '{' tidak memiliki penutup '}'.\n\
             Penyebab: Blok fungsi, event, atau blok 'unsafe' belum ditutup dengan '}'.\n\
             Benar : Pastikan setiap '{' berpasangan dengan '}' yang sesuai.",
        ),
        "E013" => Some(
            "PSTDR-E013: ID efek/suara vanilla tidak ditemukan di spesifikasi Bedrock terbaru.\n\
             Penyebab: Salah ketik nama efek pada 'apply' atau suara vanilla pada 'play_sound'.\n\
             Salah : apply slownes 2 to target for 3s\n\
             Benar : apply slowness 2 to target for 3s",
        ),
        "E014" => Some(
            "PSTDR-E014: Workspace PackStudio tidak terdeteksi.\n\
             Penyebab: Folder 'BP/' maupun 'pstdr.toml' tidak ditemukan di direktori aktif.\n\
             Benar : Jalankan di root proyek Add-On atau gunakan 'pstdr-c convert --from <file> to <out>'.",
        ),
        "E015" => Some(
            "PSTDR-E015: Audit Gate gagal.\n\
             Penyebab: 'min_engine_version' tidak berformat [1, x, x], UUID BP/RP bentrok, atau entri terlarang ditemukan.\n\
             Aturan Resmi: Elemen pertama min_engine_version WAJIB bernilai 1 untuk semua versi Bedrock,\n\
             termasuk Bedrock 26.60 (pakai [1, 26, 60]) maupun versi lama (mis. [1, 21, 50]).\n\
             Format [26, 60, 0] TIDAK VALID — angka pertama selalu 1.\n\
             Benar : Perbaiki BP/manifest.json agar memiliki \"min_engine_version\": [1, 26, 60] (target 26.60)\n\
                     atau [1, 21, 50] (target 1.21.50 / lama).",
        ),
        "E016" => Some(
            "PSTDR-E016: Kesalahan sintaks umum.\n\
             Penyebab: Token tidak terduga, tanda kutip belum ditutup, atau struktur kalimat .pstdr belum lengkap.\n\
             Benar : Periksa baris yang ditandai dan sesuaikan dengan tata bahasa .pstdr.",
        ),
        _ => None,
    }
}
