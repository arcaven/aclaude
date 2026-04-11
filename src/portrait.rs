use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::config::PortraitConfig;
use crate::persona::PersonaAgent;
use crate::terminal::{self, DisplayTool};

/// Portrait file paths by size.
pub struct PortraitPaths {
    pub small: Option<PathBuf>,
    pub medium: Option<PathBuf>,
    pub large: Option<PathBuf>,
    pub original: Option<PathBuf>,
}

impl PortraitPaths {
    /// Get the best available path for a given size, falling back through sizes.
    pub fn best_for_size(&self, preferred: &str) -> Option<&Path> {
        let order: &[&Option<PathBuf>] = match preferred {
            "small" => &[&self.small, &self.medium, &self.large, &self.original],
            "medium" => &[&self.medium, &self.large, &self.original, &self.small],
            "large" => &[&self.large, &self.original, &self.medium, &self.small],
            _ => &[&self.original, &self.large, &self.medium, &self.small],
        };
        order.iter().find_map(|o| o.as_deref())
    }

    pub fn has_any(&self) -> bool {
        self.small.is_some()
            || self.medium.is_some()
            || self.large.is_some()
            || self.original.is_some()
    }

    pub fn available_sizes(&self) -> Vec<&str> {
        let mut sizes = Vec::new();
        if self.small.is_some() {
            sizes.push("small");
        }
        if self.medium.is_some() {
            sizes.push("medium");
        }
        if self.large.is_some() {
            sizes.push("large");
        }
        if self.original.is_some() {
            sizes.push("original");
        }
        sizes
    }
}

/// Global portrait cache directory.
pub fn portrait_cache_dir() -> PathBuf {
    let data_dir = crate::paths::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share"));
    data_dir.join("forestage/portraits")
}

/// Cached manifest: theme-slug -> { role -> filename-stem }.
static MANIFEST: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();

fn load_manifest() -> &'static HashMap<String, HashMap<String, String>> {
    MANIFEST.get_or_init(|| {
        let path = portrait_cache_dir().join("manifest.json");
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };
        serde_json::from_str(&content).unwrap_or_default()
    })
}

/// Normalize a name for portrait filename matching.
fn normalize_stem(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Resolve portrait paths for a given theme/agent/role.
///
/// Resolution order:
/// 1. Manifest entry (authoritative)
/// 2. shortName (normalized)
/// 3. Full character name (normalized)
/// 4. First name only
///
/// For each candidate stem, tries exact match then prefix match in each size directory.
pub fn resolve_portrait(
    theme_slug: &str,
    agent: &PersonaAgent,
    role: Option<&str>,
) -> PortraitPaths {
    let cache_dir = portrait_cache_dir();
    let theme_dir = cache_dir.join(theme_slug);

    let mut paths = PortraitPaths {
        small: None,
        medium: None,
        large: None,
        original: None,
    };

    if !theme_dir.exists() {
        return paths;
    }

    // Build candidate stems
    let manifest = load_manifest();
    let mut stems: Vec<String> = Vec::new();

    // 1. Manifest (authoritative)
    if let Some(role_key) = role {
        if let Some(stem) = manifest.get(theme_slug).and_then(|m| m.get(role_key)) {
            stems.push(stem.clone());
        }
    }

    // 2-4. Derived stems (fallback)
    if stems.is_empty() {
        if let Some(short) = &agent.short_name {
            let s = normalize_stem(short);
            if !s.is_empty() {
                stems.push(s);
            }
        }
        let char_stem = normalize_stem(&agent.character);
        if !char_stem.is_empty() && !stems.contains(&char_stem) {
            stems.push(char_stem);
        }
        let first_name = agent
            .character
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();
        if !first_name.is_empty() && !stems.contains(&first_name) {
            stems.push(first_name);
        }
    }

    // Search each size directory
    for (size_name, slot) in [
        ("small", &mut paths.small),
        ("medium", &mut paths.medium),
        ("large", &mut paths.large),
        ("original", &mut paths.original),
    ] {
        let size_dir = theme_dir.join(size_name);
        if !size_dir.is_dir() {
            continue;
        }

        for stem in &stems {
            // Exact match
            let exact = size_dir.join(format!("{stem}.png"));
            if exact.exists() {
                *slot = Some(exact);
                break;
            }

            // Prefix match
            if let Ok(entries) = fs::read_dir(&size_dir) {
                let prefix_match = entries
                    .filter_map(std::result::Result::ok)
                    .find(|e| {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        name.ends_with(".png") && name.starts_with(stem.as_str())
                    })
                    .map(|e| e.path());
                if let Some(p) = prefix_match {
                    *slot = Some(p);
                    break;
                }
            }
        }
    }

    paths
}

/// Check if the terminal supports inline image display.
pub fn terminal_supports_images() -> bool {
    !matches!(
        terminal::detect_image_support(),
        terminal::ImageSupport::Unsupported
    )
}

/// Display a portrait image inline.
///
/// Uses three-tier terminal detection and a display tool fallback chain.
/// Returns true if the image was displayed successfully.
pub fn display_portrait(path: &Path, align: &str, cfg: &PortraitConfig) -> bool {
    let (should_try, tool) = terminal::resolve_display_intent(&cfg.display);
    if !should_try {
        return false;
    }
    if !path.exists() {
        return false;
    }
    let Some(tool) = tool else {
        return false;
    };

    match tool {
        DisplayTool::KittenIcat => try_kitten_icat(path, align),
        DisplayTool::WeztermImgcat => try_wezterm_imgcat(path),
    }
}

/// Display an image using `kitten icat`.
///
/// When inside tmux, adds `--passthrough detect` so kitten auto-wraps
/// graphics commands in tmux DCS passthrough sequences.
fn try_kitten_icat(path: &Path, align: &str) -> bool {
    let path_str = path.to_string_lossy();
    let in_tmux = terminal::inside_tmux();

    let mut args = vec!["icat", "--align", align, "--transfer-mode=stream"];
    if in_tmux {
        args.push("--passthrough");
        args.push("detect");
    }
    args.push(&path_str);

    let result = Command::new("kitten")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .status();

    if result.is_ok_and(|s| s.success()) {
        return true;
    }

    // Fallback without --align (some older kitten versions)
    let mut fallback_args = vec!["icat"];
    if in_tmux {
        fallback_args.push("--passthrough");
        fallback_args.push("detect");
    }
    fallback_args.push(&path_str);

    Command::new("kitten")
        .args(&fallback_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Display an image using `wezterm imgcat`.
fn try_wezterm_imgcat(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    Command::new("wezterm")
        .args(["imgcat", &path_str])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Get portrait cache statistics: (themes_with_portraits, total_images).
pub fn cache_status() -> (usize, usize) {
    let cache_dir = portrait_cache_dir();
    if !cache_dir.exists() {
        return (0, 0);
    }

    let mut themes = 0;
    let mut images = 0;

    if let Ok(entries) = fs::read_dir(&cache_dir) {
        for entry in entries.filter_map(std::result::Result::ok) {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            // Skip manifest.json etc.
            let name = entry.file_name();
            if name == "manifest.json" {
                continue;
            }

            let mut theme_has_images = false;
            let theme_dir = entry.path();

            for size in ["small", "medium", "large", "original"] {
                let size_dir = theme_dir.join(size);
                if let Ok(files) = fs::read_dir(&size_dir) {
                    let count = files
                        .filter_map(std::result::Result::ok)
                        .filter(|f| f.file_name().to_string_lossy().ends_with(".png"))
                        .count();
                    if count > 0 {
                        theme_has_images = true;
                        images += count;
                    }
                }
            }

            if theme_has_images {
                themes += 1;
            }
        }
    }

    (themes, images)
}
