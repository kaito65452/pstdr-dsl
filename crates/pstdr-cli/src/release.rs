use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseArtifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub version: String,
    pub released_at: String,
    pub spec_version: String,
    pub key_id: String,
    pub changelog: Vec<String>,
    pub artifacts: BTreeMap<String, ReleaseArtifact>,
}

pub fn cmd_release(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let Some(sub) = args.first().map(|s| s.as_str()) else {
        print_release_help();
        return Err((
            2,
            "Usage: pstdr-c release <keygen|manifest|sign|verify> [options]".to_string(),
        ));
    };
    if matches!(sub, "--help" | "-h" | "help") {
        print_release_help();
        return Ok(0);
    }

    let rest = &args[1..];
    match sub {
        "keygen" => cmd_keygen(cwd, rest),
        "manifest" => cmd_manifest(cwd, rest),
        "sign" => cmd_sign(cwd, rest),
        "verify" => cmd_verify(cwd, rest),
        other => Err((
            2,
            format!(
                "Unknown release subcommand '{other}'. Use keygen, manifest, sign, or verify."
            ),
        )),
    }
}

fn print_release_help() {
    println!(
        "pstdr-c release — Offline Ed25519 release signing & manifest tools\n\n\
         SUBCOMMANDS:\n\
         \x20 pstdr-c release keygen --out <dir>\n\
         \x20 pstdr-c release manifest --version <semver> --spec-version <str> --key-id <id>\n\
         \x20                          --artifacts <name=path:url>... [--changelog <line>...]\n\
         \x20                          [--released-at <rfc3339>] --out <pstdr-release.json>\n\
         \x20 pstdr-c release sign --manifest <file> --key <private.key> --out <file.sig>\n\
         \x20 pstdr-c release verify --manifest <file> --sig <file.sig> --pubkey <hex_or_file>"
    );
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}

pub fn format_fingerprint(public_key_bytes: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key_bytes);
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn cmd_keygen(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let mut out_dir: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                let val = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --out requires a directory path.".to_string()))?;
                out_dir = Some(resolve_path(cwd, val));
                i += 2;
            }
            "--help" | "-h" => {
                println!("Usage: pstdr-c release keygen --out <dir>");
                return Ok(0);
            }
            other => {
                return Err((2, format!("Unknown flag '{other}' for 'release keygen'.")));
            }
        }
    }

    let dir = out_dir
        .ok_or_else(|| (2u8, "Missing required flag --out <dir> for 'release keygen'.".to_string()))?;
    fs::create_dir_all(&dir)
        .map_err(|e| (3u8, format!("Failed to create directory '{}': {e}", dir.display())))?;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let priv_hex = hex::encode(signing_key.to_bytes());
    let pub_bytes = verifying_key.to_bytes();
    let pub_hex = hex::encode(pub_bytes);
    let fingerprint = format_fingerprint(&pub_bytes);

    let priv_path = dir.join("pstdr-signing.key");
    let pub_path = dir.join("pstdr-signing.pub");

    fs::write(&priv_path, format!("{priv_hex}\n"))
        .map_err(|e| (3u8, format!("Failed to write '{}': {e}", priv_path.display())))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&priv_path, perms).map_err(|e| {
            (
                3u8,
                format!("Failed to set 0600 permissions on '{}': {e}", priv_path.display()),
            )
        })?;
    }

    fs::write(&pub_path, format!("{pub_hex}\n"))
        .map_err(|e| (3u8, format!("Failed to write '{}': {e}", pub_path.display())))?;

    println!("Generated Ed25519 release signing keypair:");
    println!("  Private Key (0600) : {}", priv_path.display());
    println!("  Public Key File    : {}", pub_path.display());
    println!("  Public Key (hex)   : {pub_hex}");
    println!("  Key Fingerprint    : {fingerprint}");
    println!();
    println!("IMPORTANT: Keep pstdr-signing.key offline and NEVER commit it to Git.");
    Ok(0)
}

fn parse_artifact_spec(spec: &str) -> Result<(String, String, String), String> {
    let (name, rest) = spec.split_once('=').ok_or_else(|| {
        format!("Invalid artifact spec '{spec}': expected '<name>=<path>:<url>'.")
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("Invalid artifact spec '{spec}': artifact name cannot be empty."));
    }

    let (path, url) = if let Some(idx) = rest.find(":https://") {
        (&rest[..idx], &rest[idx + 1..])
    } else if let Some(idx) = rest.find(":http://") {
        (&rest[..idx], &rest[idx + 1..])
    } else if let Some((p, u)) = rest.split_once(':') {
        (p, u)
    } else {
        return Err(format!(
            "Invalid artifact spec '{spec}': expected '<name>=<path>:<url>'."
        ));
    };

    let path = path.trim();
    let url = url.trim();
    if path.is_empty() || url.is_empty() {
        return Err(format!(
            "Invalid artifact spec '{spec}': both <path> and <url> must be non-empty."
        ));
    }

    Ok((name.to_string(), path.to_string(), url.to_string()))
}

fn current_utc_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_secs_rfc3339(secs)
}

/// Converts Unix epoch seconds to an ISO-8601 / RFC-3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`)
/// using Howard Hinnant's civil-from-days algorithm (zero external date crates).
fn format_unix_secs_rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn cmd_manifest(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let mut version: Option<String> = None;
    let mut spec_version: Option<String> = None;
    let mut key_id: Option<String> = None;
    let mut released_at: Option<String> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut artifact_specs: Vec<String> = Vec::new();
    let mut changelog: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--version" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --version requires a SemVer value.".to_string()))?;
                version = Some(v.clone());
                i += 2;
            }
            "--spec-version" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --spec-version requires a value.".to_string()))?;
                spec_version = Some(v.clone());
                i += 2;
            }
            "--key-id" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --key-id requires a value.".to_string()))?;
                key_id = Some(v.clone());
                i += 2;
            }
            "--released-at" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --released-at requires a timestamp.".to_string()))?;
                released_at = Some(v.clone());
                i += 2;
            }
            "--out" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --out requires an output file path.".to_string()))?;
                out_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--artifacts" | "--artifact" => {
                i += 1;
                let mut count = 0usize;
                while i < args.len() && !args[i].starts_with("--") {
                    artifact_specs.push(args[i].clone());
                    i += 1;
                    count += 1;
                }
                if count == 0 {
                    return Err((
                        2,
                        "Flag --artifacts requires at least one '<name>=<path>:<url>' entry."
                            .to_string(),
                    ));
                }
            }
            "--changelog" => {
                i += 1;
                let mut count = 0usize;
                while i < args.len() && !args[i].starts_with("--") {
                    changelog.push(args[i].clone());
                    i += 1;
                    count += 1;
                }
                if count == 0 {
                    return Err((
                        2,
                        "Flag --changelog requires at least one changelog entry.".to_string(),
                    ));
                }
            }
            "--help" | "-h" => {
                print_release_help();
                return Ok(0);
            }
            other => {
                return Err((2, format!("Unknown flag '{other}' for 'release manifest'.")));
            }
        }
    }

    let version = version.ok_or_else(|| (2u8, "Missing required flag --version <semver>.".to_string()))?;
    let spec_version = spec_version
        .ok_or_else(|| (2u8, "Missing required flag --spec-version <str>.".to_string()))?;
    let key_id = key_id.ok_or_else(|| (2u8, "Missing required flag --key-id <id>.".to_string()))?;
    let out_path =
        out_path.ok_or_else(|| (2u8, "Missing required flag --out <pstdr-release.json>.".to_string()))?;

    if artifact_specs.is_empty() {
        return Err((
            2,
            "At least one artifact must be provided via --artifacts <name=path:url>.".to_string(),
        ));
    }

    let mut artifacts = BTreeMap::new();
    for raw_spec in &artifact_specs {
        let (name, rel_path, url) = parse_artifact_spec(raw_spec).map_err(|e| (2u8, e))?;
        let file_path = resolve_path(cwd, &rel_path);
        let bytes = fs::read(&file_path).map_err(|e| {
            (
                3u8,
                format!("Failed to read artifact '{}' at '{}': {e}", name, file_path.display()),
            )
        })?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let size = bytes.len() as u64;
        artifacts.insert(name, ReleaseArtifact { url, sha256, size });
    }

    let manifest = ReleaseManifest {
        schema_version: 1,
        version,
        released_at: released_at.unwrap_or_else(current_utc_rfc3339),
        spec_version,
        key_id,
        changelog,
        artifacts,
    };

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                (3u8, format!("Failed to create directory '{}': {e}", parent.display()))
            })?;
        }
    }

    let pretty = serde_json::to_string_pretty(&manifest)
        .map_err(|e| (3u8, format!("Failed to serialize release manifest: {e}")))?;
    fs::write(&out_path, format!("{pretty}\n"))
        .map_err(|e| (3u8, format!("Failed to write '{}': {e}", out_path.display())))?;

    println!(
        "Wrote release manifest ({} artifact(s)) to {}",
        manifest.artifacts.len(),
        out_path.display()
    );
    Ok(0)
}

fn load_signing_key(key_path: &Path) -> Result<SigningKey, (u8, String)> {
    let raw = fs::read_to_string(key_path)
        .map_err(|e| (3u8, format!("Failed to read private key '{}': {e}", key_path.display())))?;
    let trimmed = raw.trim();
    let bytes = hex::decode(trimmed).map_err(|e| {
        (
            2u8,
            format!("Invalid hex in private key '{}': {e}", key_path.display()),
        )
    })?;
    let seed: [u8; 32] = match bytes.len() {
        32 => bytes.as_slice().try_into().unwrap(),
        64 => bytes[..32].try_into().unwrap(),
        n => {
            return Err((
                2,
                format!(
                    "Invalid Ed25519 private key length in '{}': expected 32 bytes (64 hex chars), found {n} bytes.",
                    key_path.display()
                ),
            ));
        }
    };
    Ok(SigningKey::from_bytes(&seed))
}

fn load_verifying_key(cwd: &Path, pubkey_arg: &str) -> Result<VerifyingKey, (u8, String)> {
    let candidate = resolve_path(cwd, pubkey_arg);
    let hex_str = if candidate.is_file() {
        fs::read_to_string(&candidate)
            .map_err(|e| (3u8, format!("Failed to read public key '{}': {e}", candidate.display())))?
            .trim()
            .to_string()
    } else {
        pubkey_arg.trim().to_string()
    };

    let bytes = hex::decode(&hex_str)
        .map_err(|e| (2u8, format!("Invalid Ed25519 public key hex '{pubkey_arg}': {e}")))?;
    let key_bytes: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        (
            2u8,
            format!(
                "Invalid Ed25519 public key length: expected 32 bytes (64 hex chars), found {} bytes.",
                bytes.len()
            ),
        )
    })?;

    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| (2u8, format!("Invalid Ed25519 public key: {e}")))
}

fn cmd_sign(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let mut manifest_path: Option<PathBuf> = None;
    let mut key_path: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --manifest requires a file path.".to_string()))?;
                manifest_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--key" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --key requires a private key file path.".to_string()))?;
                key_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--out" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --out requires a signature output path.".to_string()))?;
                out_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: pstdr-c release sign --manifest <pstdr-release.json> --key <private.key> --out <pstdr-release.json.sig>"
                );
                return Ok(0);
            }
            other => {
                return Err((2, format!("Unknown flag '{other}' for 'release sign'.")));
            }
        }
    }

    let manifest_path = manifest_path
        .ok_or_else(|| (2u8, "Missing required flag --manifest <file>.".to_string()))?;
    let key_path =
        key_path.ok_or_else(|| (2u8, "Missing required flag --key <private.key>.".to_string()))?;
    let out_path =
        out_path.ok_or_else(|| (2u8, "Missing required flag --out <file.sig>.".to_string()))?;

    let manifest_bytes = fs::read(&manifest_path).map_err(|e| {
        (3u8, format!("Failed to read manifest '{}': {e}", manifest_path.display()))
    })?;
    let signing_key = load_signing_key(&key_path)?;
    let signature: Signature = signing_key.sign(&manifest_bytes);
    let sig_hex = hex::encode(signature.to_bytes());

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                (3u8, format!("Failed to create directory '{}': {e}", parent.display()))
            })?;
        }
    }

    fs::write(&out_path, format!("{sig_hex}\n"))
        .map_err(|e| (3u8, format!("Failed to write signature '{}': {e}", out_path.display())))?;

    println!("Signed {} -> {}", manifest_path.display(), out_path.display());
    Ok(0)
}

fn cmd_verify(cwd: &Path, args: &[String]) -> Result<u8, (u8, String)> {
    let mut manifest_path: Option<PathBuf> = None;
    let mut sig_path: Option<PathBuf> = None;
    let mut pubkey_arg: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --manifest requires a file path.".to_string()))?;
                manifest_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--sig" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --sig requires a signature file path.".to_string()))?;
                sig_path = Some(resolve_path(cwd, v));
                i += 2;
            }
            "--pubkey" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| (2u8, "Flag --pubkey requires a hex string or file path.".to_string()))?;
                pubkey_arg = Some(v.clone());
                i += 2;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: pstdr-c release verify --manifest <pstdr-release.json> --sig <pstdr-release.json.sig> --pubkey <hex_or_file>"
                );
                return Ok(0);
            }
            other => {
                return Err((2, format!("Unknown flag '{other}' for 'release verify'.")));
            }
        }
    }

    let manifest_path = manifest_path
        .ok_or_else(|| (2u8, "Missing required flag --manifest <file>.".to_string()))?;
    let sig_path =
        sig_path.ok_or_else(|| (2u8, "Missing required flag --sig <file.sig>.".to_string()))?;
    let pubkey_arg = pubkey_arg
        .ok_or_else(|| (2u8, "Missing required flag --pubkey <hex_or_file>.".to_string()))?;

    let manifest_bytes = fs::read(&manifest_path).map_err(|e| {
        (3u8, format!("Failed to read manifest '{}': {e}", manifest_path.display()))
    })?;
    let sig_raw = fs::read_to_string(&sig_path)
        .map_err(|e| (3u8, format!("Failed to read signature '{}': {e}", sig_path.display())))?;
    let sig_bytes_vec = hex::decode(sig_raw.trim()).map_err(|e| {
        (
            1u8,
            format!("Invalid hex in signature '{}': {e}", sig_path.display()),
        )
    })?;
    let sig_bytes: [u8; 64] = sig_bytes_vec.as_slice().try_into().map_err(|_| {
        (
            1u8,
            format!(
                "Invalid Ed25519 signature length in '{}': expected 64 bytes (128 hex chars), found {} bytes.",
                sig_path.display(),
                sig_bytes_vec.len()
            ),
        )
    })?;
    let signature = Signature::from_bytes(&sig_bytes);
    let verifying_key = load_verifying_key(cwd, &pubkey_arg)?;

    if verifying_key.verify_strict(&manifest_bytes, &signature).is_ok() {
        println!(
            "Signature VALID for {} (fingerprint: {})",
            manifest_path.display(),
            format_fingerprint(&verifying_key.to_bytes())
        );
        Ok(0)
    } else {
        eprintln!(
            "Signature verification FAILED for {}: manifest bytes do not match signature or public key.",
            manifest_path.display()
        );
        Ok(1)
    }
}
