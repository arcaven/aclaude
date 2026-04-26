//! Portrait pack download via subprocess (curl + tar + openssl).
//!
//! No reqwest, no async — runs before the TUI event loop starts.
//! Subprocess pattern matches portrait.rs display (std::process::Command).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::PortraitConfig;
use crate::error::{ForestageError, Result};
use crate::portrait::portrait_cache_dir;

const MANIFEST_URL: &str = "https://portraits.darkatelier.org/v1/manifest.json";
const MANIFEST_CHECK_INTERVAL_SECS: u64 = 86400; // 24h

/// Remote manifest schema.
///
/// The CDN's `personas` field (legacy role→stem map) is intentionally
/// not declared here. serde ignores unknown JSON fields by default, so
/// the existing manifest still deserializes; the field is unused under
/// the B14 agent taxonomy because portrait resolution derives stems
/// from the Character itself (see `portrait::resolve_portrait`).
#[derive(Debug, serde::Deserialize)]
struct RemoteManifest {
    #[allow(dead_code)]
    schema: u32,
    base_url: String,
    themes: HashMap<String, ThemeEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct ThemeEntry {
    pack_sha256: String,
    #[allow(dead_code)]
    pack_bytes: u64,
    #[allow(dead_code)]
    persona_count: u32,
}

/// Cache metadata — tracks etag and last-checked timestamp.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct CacheMeta {
    etag: Option<String>,
    last_checked: u64,
}

/// Ensure portraits exist for the given theme. Downloads if missing.
///
/// Skips entirely if portraits are disabled (`display = "never"`) or
/// auto-download is off. Returns Ok(true) if portraits are available,
/// Ok(false) if download was skipped or failed gracefully.
pub fn ensure_portraits(theme: &str, config: &PortraitConfig) -> Result<bool> {
    // Don't download if portraits are disabled or auto-download is off
    if config.display == "never" || !config.auto_download {
        return Ok(false);
    }

    let cache = portrait_cache_dir();
    let theme_dir = cache.join(theme);
    let sentinel = theme_dir.join(".complete");

    // Hot path: already downloaded
    if sentinel.exists() {
        return Ok(true);
    }

    // Check curl is available
    if !command_exists("curl") {
        eprintln!(
            "warning: curl not found — portrait download skipped. Install curl or set [portrait] auto_download = false"
        );
        return Ok(false);
    }

    // Fetch manifest (rate-limited to once per 24h)
    let manifest = match fetch_manifest(&cache)? {
        Some(m) => m,
        None => return Ok(false),
    };

    // Check theme exists in manifest
    let entry = match manifest.themes.get(theme) {
        Some(e) => e,
        None => return Ok(false),
    };

    // Ensure cache directory exists before writing temp files
    fs::create_dir_all(&cache).map_err(|e| ForestageError::Session {
        message: format!("failed to create portrait cache dir: {e}"),
    })?;

    let pack_url = format!("{}/themes/{}.tar.gz", manifest.base_url, theme);
    eprintln!("Downloading portrait pack for \"{theme}\"");
    eprintln!("  from: {pack_url}");
    eprintln!("  to:   {}", theme_dir.display());
    eprintln!("  (disable with: [portrait] auto_download = false)");

    // Download
    let tmp_pack = cache.join(format!(".{theme}.tar.gz.tmp"));

    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp_pack)
        .arg(&pack_url)
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            let _ = fs::remove_file(&tmp_pack);
            eprintln!("warning: portrait download failed for {theme}");
            return Ok(false);
        }
    }

    // Verify SHA256
    if !verify_sha256(&tmp_pack, &entry.pack_sha256)? {
        let _ = fs::remove_file(&tmp_pack);
        eprintln!("warning: portrait pack SHA256 mismatch for {theme}, skipping");
        return Ok(false);
    }

    // Extract
    fs::create_dir_all(&theme_dir).map_err(|e| ForestageError::Session {
        message: format!("failed to create portrait dir: {e}"),
    })?;

    let extract = Command::new("tar")
        .args(["xzf"])
        .arg(&tmp_pack)
        .arg("-C")
        .arg(&theme_dir)
        .status();

    let _ = fs::remove_file(&tmp_pack);

    match extract {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("warning: portrait extraction failed for {theme}");
            return Ok(false);
        }
    }

    // Write sentinel
    let _ = fs::write(&sentinel, "");

    eprintln!("Portrait pack installed for {theme}");
    Ok(true)
}

/// Download portrait packs for all themes in the remote manifest.
pub fn download_all(config: &PortraitConfig) -> Result<(usize, usize)> {
    let cache = portrait_cache_dir();
    let manifest = match fetch_manifest(&cache)? {
        Some(m) => m,
        None => {
            eprintln!("Could not fetch portrait manifest");
            return Ok((0, 0));
        }
    };

    let mut downloaded = 0;
    let mut skipped = 0;

    for theme in manifest.themes.keys() {
        let sentinel = cache.join(theme).join(".complete");
        if sentinel.exists() {
            skipped += 1;
            continue;
        }
        match ensure_portraits(theme, config) {
            Ok(true) => downloaded += 1,
            _ => skipped += 1,
        }
    }

    Ok((downloaded, skipped))
}

/// List available themes from the remote manifest.
pub fn list_remote() -> Result<Vec<(String, u32)>> {
    let cache = portrait_cache_dir();
    let manifest = match fetch_manifest(&cache)? {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };

    let mut themes: Vec<(String, u32)> = manifest
        .themes
        .iter()
        .map(|(name, entry)| (name.clone(), entry.persona_count))
        .collect();
    themes.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(themes)
}

/// Fetch remote manifest, rate-limited by cache metadata.
fn fetch_manifest(cache: &Path) -> Result<Option<RemoteManifest>> {
    let meta_path = cache.join(".cache_meta.json");
    let manifest_cache = cache.join(".manifest_cache.json");

    // Check rate limit
    let meta: CacheMeta = fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now - meta.last_checked < MANIFEST_CHECK_INTERVAL_SECS {
        // Use cached manifest if within check interval
        if let Ok(cached) = fs::read_to_string(&manifest_cache) {
            if let Ok(m) = serde_json::from_str(&cached) {
                return Ok(Some(m));
            }
        }
    }

    // Fetch manifest via curl, with etag for conditional request
    let tmp = cache.join(".manifest.tmp");
    fs::create_dir_all(cache).map_err(|e| ForestageError::Session {
        message: format!("failed to create cache dir: {e}"),
    })?;

    let header_file = cache.join(".manifest_headers.tmp");
    let mut curl_args = vec![
        "-fsSL".to_string(),
        "-o".to_string(),
        tmp.to_string_lossy().to_string(),
        "-D".to_string(),
        header_file.to_string_lossy().to_string(),
    ];

    if let Some(etag) = &meta.etag {
        curl_args.push("-H".to_string());
        curl_args.push(format!("If-None-Match: {etag}"));
    }

    curl_args.push(MANIFEST_URL.to_string());

    let status = Command::new("curl").args(&curl_args).status();

    match status {
        Ok(s) if s.success() => {
            // New manifest downloaded
            let body = match fs::read_to_string(&tmp) {
                Ok(b) => b,
                Err(_) => {
                    let _ = fs::remove_file(&tmp);
                    let _ = fs::remove_file(&header_file);
                    return Ok(None);
                }
            };

            // Extract etag from response headers
            let new_etag = fs::read_to_string(&header_file).ok().and_then(|headers| {
                headers
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("etag:"))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
            });

            let _ = fs::rename(&tmp, &manifest_cache);
            let _ = fs::remove_file(&header_file);

            let new_meta = CacheMeta {
                etag: new_etag,
                last_checked: now,
            };
            let _ = fs::write(
                &meta_path,
                serde_json::to_string(&new_meta).unwrap_or_default(),
            );

            Ok(serde_json::from_str(&body).ok())
        }
        _ => {
            let _ = fs::remove_file(&tmp);
            let _ = fs::remove_file(&header_file);

            // Update last_checked even on failure to avoid hammering
            let new_meta = CacheMeta {
                etag: meta.etag,
                last_checked: now,
            };
            let _ = fs::write(
                &meta_path,
                serde_json::to_string(&new_meta).unwrap_or_default(),
            );

            // Fall back to cached manifest
            if let Ok(cached) = fs::read_to_string(&manifest_cache) {
                return Ok(serde_json::from_str(&cached).ok());
            }
            Ok(None)
        }
    }
}

/// Verify file SHA256 using openssl.
fn verify_sha256(path: &Path, expected: &str) -> Result<bool> {
    let output = Command::new("openssl")
        .args(["dgst", "-sha256", "-r"])
        .arg(path)
        .output()
        .map_err(|e| ForestageError::Session {
            message: format!("openssl not found for SHA256 verification: {e}"),
        })?;

    if !output.status.success() {
        return Ok(false);
    }

    let hash = String::from_utf8_lossy(&output.stdout);
    let computed = hash.split_whitespace().next().unwrap_or("");
    Ok(computed == expected)
}

/// Check if a command exists on PATH.
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Remove cached portraits for a theme.
pub fn clean_theme(theme: &str) -> Result<bool> {
    let cache = portrait_cache_dir();
    let theme_dir = cache.join(theme);
    if theme_dir.exists() {
        fs::remove_dir_all(&theme_dir).map_err(|e| ForestageError::Session {
            message: format!("failed to remove portrait cache for {theme}: {e}"),
        })?;
        Ok(true)
    } else {
        Ok(false)
    }
}
