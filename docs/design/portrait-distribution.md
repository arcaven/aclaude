# Portrait Distribution — Implementation Design

Concrete changes needed in Cloudflare and forestage to ship portrait
distribution for the 45 themes that already have portraits.

> **Status note (2026-04-26):** the `personas` role→stem map originally
> described in §2 and emitted by §3's gen-manifest.sh has been retired
> under the B14 agent taxonomy (orc finding-033). Portrait resolution
> now derives stems from the `Character` (shortName → full name → first
> name, prefix-matched against the size directory). See
> [`docs/agent-taxonomy.md`](../agent-taxonomy.md) for the taxonomy and
> [`src/portrait.rs::resolve_portrait`](../../src/portrait.rs) for the
> code. The historical `personas` section in this design is preserved
> below as design context only — the script no longer emits it and
> `src/download.rs` no longer reads it.

---

## 1. Cloudflare R2 Setup

### Bucket

```
Bucket name:  forestage-portraits
Region:       auto (Cloudflare chooses closest)
```

### Custom domain

```
Domain:    portraits.darkatelier.org
Type:      CNAME → <bucket>.r2.dev  (or Cloudflare custom domain binding)
HTTPS:     automatic via Cloudflare
```

### Bucket layout after upload

```
v1/
  manifest.json
  themes/
    1984.tar.gz
    1984.sha256
    a-team.tar.gz
    a-team.sha256
    ...                    (45 theme packs)
    west-wing.tar.gz
    west-wing.sha256
```

No subdirectories per theme inside the bucket — flat list of archives
plus checksums. The manifest is the index.

### CORS (optional, for future web use)

```json
[{
  "AllowedOrigins": ["*"],
  "AllowedMethods": ["GET", "HEAD"],
  "AllowedHeaders": ["If-None-Match"],
  "ExposeHeaders": ["ETag"],
  "MaxAgeSeconds": 86400
}]
```

### Cache headers

Set on upload via rclone flags:
- `manifest.json`: `Cache-Control: public, max-age=3600` (1h — etag handles freshness)
- `*.tar.gz`: `Cache-Control: public, max-age=604800, immutable` (7d — content-addressed by sha256)
- `*.sha256`: `Cache-Control: public, max-age=604800, immutable`

---

## 2. Manifest Format

**Current (B14, post-finding-033):**

```json
{
  "schema": 1,
  "updated": "2026-04-26T00:00:00Z",
  "base_url": "https://portraits.darkatelier.org/v1",
  "themes": {
    "dune": {
      "pack_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "pack_bytes": 4812000,
      "persona_count": 10
    },
    "star-wars": {
      "pack_sha256": "...",
      "pack_bytes": 7200000,
      "persona_count": 11
    }
  }
}
```

`themes` is the only section. Pack URL is derived from `base_url` +
`/themes/{slug}.tar.gz`. No per-theme URL field — the convention is the
schema. Portrait resolution does not need a manifest hint at all; see
[Portrait Resolution](#portrait-resolution) below.

**Historical (pre-B14):** the manifest also carried a `personas` field
keyed by `theme → role → filename-stem` (e.g. `"dev": "paul-54212"`).
That map was a role-keyed lookup index used by an earlier
`portrait.rs` lookup path. It was retired when role became a job
assignment rather than a character key — `--persona` and `--role`
could refer to different characters, which made the map serve the
wrong portrait (the granny→ponder bug class). Existing CDN manifests
may still carry the field; serde ignores it.

---

## Portrait Resolution

Portrait lookup happens entirely on the client. The manifest tells
forestage *which themes have packs*; nothing in the manifest tells
forestage *which file is whose portrait*. Resolution derives the
filename from the `Character` itself.

For a character with `character: "Granny Weatherwax"` and
`shortName: "Granny"`, `portrait::resolve_portrait(theme_slug, agent)`
walks these candidate stems in order:

1. `shortName`, slugified — `"granny"`
2. Full `character` name, slugified — `"granny-weatherwax"`
3. First word of the character name, slugified — `"granny"`
   (deduplicated against earlier entries)

For each size directory (`small/medium/large/original`) under
`<cache>/<theme>/`, each candidate is tried first as an exact match
(`<stem>.png`) and then as a prefix match (any `<stem>*.png`). The
prefix branch handles the CDN's hashed filenames — packs ship
`granny-35211.png`, `ponder-55233.png`, etc., and the unhashed
`granny` stem matches by prefix. The first match per size wins.

What this means in practice:

- The pack tarball can use any naming scheme as long as filenames
  start with the character's `shortName` or first name. CDN packs
  use a five-digit OCEAN suffix; that's a publishing convention,
  not a contract.
- Two characters whose stems collide (Naomi Holden and Naomi
  Nagata, say, on themes that mix both) need disambiguating
  short names — the resolver has no theme-aware tiebreak beyond
  prefix-match alphabetical iteration order.
- The manifest's old `personas` field (theme → role → stem) was a
  pre-B14 lookup index. Its role key became meaningless when role
  became a job assignment, and the override could serve the wrong
  character's portrait whenever `--persona` and `--role` referred
  to different characters (the granny→ponder bug class). Removed.

See [`docs/agent-taxonomy.md`](../agent-taxonomy.md) for the broader
persona/identity/role taxonomy and `src/portrait.rs` for the
implementation. The behavioral test `resolve_portrait_returns_each_characters_own_file`
in `src/portrait.rs` is the regression guard.

---

## 3. Publishing Scripts

### `scripts/portraits/pack-portraits.sh`

Run from pennyfarthing repo against the existing portrait directory.

```bash
#!/usr/bin/env bash
set -euo pipefail

PORTRAITS_DIR="${1:?Usage: pack-portraits.sh <portraits-dir> <dist-dir>}"
DIST_DIR="${2:?Usage: pack-portraits.sh <portraits-dir> <dist-dir>}"

mkdir -p "$DIST_DIR"

count=0
for theme_dir in "$PORTRAITS_DIR"/*/; do
    theme=$(basename "$theme_dir")
    # Skip legacy flat-layout size dirs
    [[ "$theme" == "small" || "$theme" == "medium" || "$theme" == "large" || "$theme" == "original" ]] && continue
    # Must have at least original/ with images
    [[ ! -d "$theme_dir/original" ]] && continue

    echo "Packing $theme..."
    tar czf "$DIST_DIR/${theme}.tar.gz" -C "$theme_dir" .
    openssl dgst -sha256 -r "$DIST_DIR/${theme}.tar.gz" | cut -d' ' -f1 > "$DIST_DIR/${theme}.sha256"
    count=$((count + 1))
done

echo "Packed $count themes to $DIST_DIR"
```

### `scripts/portraits/gen-manifest.sh`

Generates manifest.json from packed themes. Reads each pack's
sha256, byte size, and original-image count to fill `themes`.

Usage:
```bash
./scripts/portraits/gen-manifest.sh dist/portraits
./scripts/portraits/gen-manifest.sh dist/portraits https://example.com/v1
```

Output is written to stdout — redirect to `dist/portraits/manifest.json`.

The script no longer needs a themes-yaml directory or `yq`. The pre-
B14 version walked theme YAMLs (`.agents` keys, OCEAN fields,
`shortName`) to build a role→stem map for the manifest's `personas`
section. Resolution stopped consulting that map (see
[Portrait Resolution](#portrait-resolution)) and the section was
dropped along with the YAML walk.

See the script in-tree for the current 60-line implementation.

### `scripts/portraits/upload-r2.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="${1:?Usage: upload-r2.sh <dist-dir>}"

echo "Uploading theme packs..."
rclone sync "$DIST_DIR/" r2:forestage-portraits/v1/themes/ \
    --include "*.tar.gz" --include "*.sha256" \
    --header-upload "Cache-Control: public, max-age=604800, immutable" \
    --progress

echo "Uploading manifest..."
rclone copyto "$DIST_DIR/manifest.json" r2:forestage-portraits/v1/manifest.json \
    --header-upload "Cache-Control: public, max-age=3600" \
    --progress

echo "Done. Verify: curl -I https://portraits.darkatelier.org/v1/manifest.json"
```

---

## 4. Rust Client Changes

### 4a. Config: add `auto_download` to `PortraitConfig`

```rust
// config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortraitConfig {
    /// Image display mode: "auto" (detect terminal), "always", "never".
    #[serde(default = "default_portrait_display")]
    pub display: String,
    /// Auto-download missing portrait packs on session start.
    #[serde(default = "default_true")]
    pub auto_download: bool,
}

impl Default for PortraitConfig {
    fn default() -> Self {
        Self {
            display: default_portrait_display(),
            auto_download: true,
        }
    }
}
```

TOML: `[portrait] auto_download = false`
Env: `FORESTAGE_PORTRAIT__AUTO_DOWNLOAD=false`

### 4b. New module: `src/download.rs`

All network operations. Subprocess-based, sync. No new crate
dependencies.

The original design (preserved below as historical context) declared a
`personas: HashMap<String, HashMap<String, String>>` field on
`RemoteManifest` and called `merge_local_manifest()` after extraction
to write a local `manifest.json` keyed by theme→role→stem. Both have
been removed. Today the deserializer ignores the CDN's leftover
`personas` field (serde default), and `ensure_portraits` ends after
writing the `.complete` sentinel — `portrait::resolve_portrait`
derives stems from the `Character` directly. See
[Portrait Resolution](#portrait-resolution).

The current source is in `src/download.rs` — the live file is
authoritative; the historical excerpt below shows the original
approach.

```rust
//! Portrait pack download via subprocess (curl + tar + openssl).
//!
//! No reqwest, no async — this runs before the TUI event loop.
//! Pattern matches portrait.rs display (std::process::Command).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{ForestageError, Result};
use crate::portrait::portrait_cache_dir;

const MANIFEST_URL: &str = "https://portraits.darkatelier.org/v1/manifest.json";
const MANIFEST_CHECK_INTERVAL_SECS: u64 = 86400; // 24h

/// Remote manifest schema.
#[derive(Debug, serde::Deserialize)]
struct RemoteManifest {
    #[allow(dead_code)]
    schema: u32,
    base_url: String,
    themes: HashMap<String, ThemeEntry>,
    personas: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize)]
struct ThemeEntry {
    pack_sha256: String,
    #[allow(dead_code)]
    pack_bytes: u64,
    #[allow(dead_code)]
    persona_count: u32,
}

/// Cache metadata — tracks etag and last-checked per manifest.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct CacheMeta {
    etag: Option<String>,
    last_checked: u64,
}

/// Ensure portraits exist for the given theme. Downloads if missing.
///
/// Returns Ok(true) if portraits are available (cached or downloaded),
/// Ok(false) if download was skipped or failed gracefully.
pub fn ensure_portraits(theme: &str) -> Result<bool> {
    let cache = portrait_cache_dir();
    let theme_dir = cache.join(theme);
    let sentinel = theme_dir.join(".complete");

    // Hot path: already downloaded
    if sentinel.exists() {
        return Ok(true);
    }

    // Check curl is available
    if !command_exists("curl") {
        return Err(ForestageError::Session {
            message: "curl not found — needed for portrait download. Install curl or set [portrait] auto_download = false".to_string(),
        });
    }

    // Fetch manifest (rate-limited to once per 24h)
    let manifest = match fetch_manifest(&cache)? {
        Some(m) => m,
        None => return Ok(false), // no manifest available
    };

    // Check theme exists in manifest
    let entry = match manifest.themes.get(theme) {
        Some(e) => e,
        None => return Ok(false), // theme not in remote manifest
    };

    // Download, verify, extract
    let pack_url = format!("{}/themes/{}.tar.gz", manifest.base_url, theme);
    let tmp_pack = cache.join(format!(".{theme}.tar.gz.tmp"));

    // Download
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp_pack)
        .arg(&pack_url)
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            let _ = fs::remove_file(&tmp_pack);
            return Ok(false); // download failed, non-fatal
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
        _ => return Ok(false), // extraction failed, non-fatal
    }

    // Write sentinel
    let _ = fs::write(&sentinel, "");

    // Merge persona mapping into local manifest.json
    if let Some(persona_map) = manifest.personas.get(theme) {
        merge_local_manifest(&cache, theme, persona_map)?;
    }

    Ok(true)
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
        // Use cached manifest if available
        if let Ok(cached) = fs::read_to_string(&manifest_cache) {
            if let Ok(m) = serde_json::from_str(&cached) {
                return Ok(Some(m));
            }
        }
    }

    // Fetch with etag
    let tmp = cache.join(".manifest.tmp");
    fs::create_dir_all(cache).map_err(|e| ForestageError::Session {
        message: format!("failed to create cache dir: {e}"),
    })?;

    let mut curl_args: Vec<String> = vec![
        "-fsSL".to_string(),
        "-o".to_string(),
        tmp.to_string_lossy().to_string(),
        "-D".to_string(),
        "-".to_string(), // headers to stdout
    ];

    if let Some(etag) = &meta.etag {
        curl_args.push("-H".to_string());
        curl_args.push(format!("If-None-Match: {etag}"));
    }

    curl_args.push(MANIFEST_URL.to_string());

    let output = Command::new("curl")
        .args(&curl_args)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            // Parse new manifest
            let body = match fs::read_to_string(&tmp) {
                Ok(b) => b,
                Err(_) => {
                    let _ = fs::remove_file(&tmp);
                    return Ok(None);
                }
            };

            // Extract etag from response headers
            let headers = String::from_utf8_lossy(&out.stdout);
            let new_etag = headers
                .lines()
                .find(|l| l.to_lowercase().starts_with("etag:"))
                .map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
                .flatten();

            // Save to cache
            let _ = fs::rename(&tmp, &manifest_cache);
            let new_meta = CacheMeta {
                etag: new_etag,
                last_checked: now,
            };
            let _ = fs::write(&meta_path, serde_json::to_string(&new_meta).unwrap_or_default());

            serde_json::from_str(&body).ok().map(Some).unwrap_or(Ok(None))
        }
        Ok(out) => {
            let _ = fs::remove_file(&tmp);
            // 304 Not Modified — use cached
            let code = out.status.code().unwrap_or(0);
            if code == 22 {
                // curl -f returns 22 for HTTP errors; check if 304
                // Update last_checked even on 304
                let new_meta = CacheMeta {
                    etag: meta.etag,
                    last_checked: now,
                };
                let _ = fs::write(&meta_path, serde_json::to_string(&new_meta).unwrap_or_default());
            }

            if let Ok(cached) = fs::read_to_string(&manifest_cache) {
                return Ok(serde_json::from_str(&cached).ok());
            }
            Ok(None)
        }
        Err(_) => Ok(None), // network error, non-fatal
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

/// Merge a theme's persona map into the local manifest.json.
///
/// The local manifest is the exact format portrait.rs expects:
/// { "theme-slug": { "role": "filename-stem" } }
fn merge_local_manifest(
    cache: &Path,
    theme: &str,
    persona_map: &HashMap<String, String>,
) -> Result<()> {
    let manifest_path = cache.join("manifest.json");

    let mut manifest: HashMap<String, HashMap<String, String>> =
        fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

    manifest.insert(theme.to_string(), persona_map.clone());

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| ForestageError::Session {
        message: format!("failed to serialize manifest: {e}"),
    })?;

    fs::write(&manifest_path, json).map_err(|e| ForestageError::Session {
        message: format!("failed to write manifest: {e}"),
    })
}

/// Check if a command exists on PATH.
fn command_exists(cmd: &str) -> bool {
    Command::new("command")
        .args(["-v", cmd])
        .output()
        .is_ok_and(|o| o.status.success())
}
```

### 4c. CLI: add `Portraits` subcommand

```rust
// In main.rs Commands enum:

/// Manage portrait images
Portraits {
    #[command(subcommand)]
    action: PortraitAction,
},

// New enum:
#[derive(Subcommand)]
enum PortraitAction {
    /// Download portrait pack for a theme
    Download {
        /// Theme slug (e.g. "dune"). Omit for current theme.
        theme: Option<String>,
        /// Download all available themes
        #[arg(long)]
        all: bool,
    },
    /// Show portrait cache status
    Status,
    /// Clean cached portraits
    Clean {
        /// Theme to clean (omit for all)
        theme: Option<String>,
    },
}
```

### 4d. Integration: call `ensure_portraits` before TUI

```rust
// In main.rs, the default interactive branch (line ~258):
_ => {
    // Auto-download portraits before TUI launch
    if cfg.portrait.auto_download {
        if let Err(e) = forestage::download::ensure_portraits(&cfg.persona.theme) {
            eprintln!("portrait download: {e}");
        }
    }
    // forestage TUI (custom ratatui over NDJSON)
    let rt = tokio::runtime::Builder::new_current_thread()
        // ...
```

Also call it before `persona show --portrait`:
```rust
// In PersonaAction::Show, before resolve_portrait:
if show_portrait && cfg.portrait.auto_download {
    let _ = forestage::download::ensure_portraits(&name);
}
```

### 4e. Expose in `lib.rs`

```rust
pub mod download;
```

---

## 5. Flat Layout Migration

> **Status:** never built. The "match against persona entries" step
> below was specified pre-B14; under the current taxonomy a migration
> would derive stems from `Character`, not from a role-keyed map.
> Left here as design context.

Add to `PortraitAction`:

```rust
/// Migrate legacy flat-layout portraits to per-theme directories
Migrate,
```

Implementation:
1. Detect `portraits/original/*.png` (flat layout marker)
2. For each image, match filename against all theme YAMLs' persona entries
3. Move matched images to `portraits/{theme}/{size}/{filename}`
4. Move unmatched to `portraits/.orphaned/`
5. Remove empty `portraits/{size}/` directories
6. Report: "Migrated N, orphaned M, freed Xmb"

---

## 6. Changes Summary

Original implementation pass:

| File | Change |
|------|--------|
| `src/download.rs` | **New** — manifest fetch, pack download, SHA256 verify, extract |
| `src/config.rs` | Add `auto_download: bool` to `PortraitConfig` |
| `src/main.rs` | Add `Portraits` subcommand, call `ensure_portraits` before TUI |
| `src/lib.rs` | Add `pub mod download` |
| `Cargo.toml` | No new dependencies |
| `scripts/portraits/` | **New** — pack, gen-manifest, upload scripts |

**Zero new Rust dependencies.** Uses `curl`, `tar`, `openssl` via
subprocess — all present on macOS and standard Linux.

Post-B14 cleanup pass (this revision):

| File | Change |
|------|--------|
| `src/download.rs` | Drop `RemoteManifest.personas` field, drop `merge_local_manifest()`, drop the call site in `ensure_portraits` |
| `src/portrait.rs` | Drop `manifest.json` role-key override (PR #58); resolution becomes character-derived |
| `scripts/portraits/gen-manifest.sh` | Drop `personas` section emission and the YAML walk; signature simplifies to `(dist-dir, [base-url])` |
| `docs/design/portrait-distribution.md` | Add §Portrait Resolution; mark `personas` map as historical |

---

## 7. Testing Plan

| Test | What |
|------|------|
| Unit: `verify_sha256` | Known hash matches, mismatch returns false |
| Unit: `CacheMeta` serde | Round-trips through JSON |
| Integration: `ensure_portraits` | Mock HTTP with local file server, verify full flow |
| Manual: fresh install | `forestage` with no cache → downloads theme → portrait renders |
| Manual: cached | Second launch → no network, instant |
| Manual: offline | No network → warns, continues without portrait |
| Manual: `portraits download --all` | All themes downloaded |
| Manual: `portraits status` | Shows counts |
| Manual: `portraits clean dune` | Removes theme dir + sentinel |
| Behavioral: `resolve_portrait_returns_each_characters_own_file` | Granny + Ponder each resolve to their own pack file (granny→ponder regression guard) |
