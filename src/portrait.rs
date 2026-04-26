use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::PortraitConfig;
use crate::persona::Character;
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

/// Resolve portrait paths for a given theme + character.
///
/// Derived-stem lookup order:
/// 1. shortName (normalized)
/// 2. Full character name (normalized)
/// 3. First name only
///
/// For each candidate stem, tries exact match then prefix match in each
/// size directory (small/medium/large/original).
///
/// The legacy `manifest.json` role-keyed override was removed in the
/// B14 agent taxonomy cleanup: role is a job assignment, not a persona
/// selector, so the manifest's role-key override served the wrong
/// portrait whenever --persona and --role referred to different
/// characters (the granny→ponder class of bug).
pub fn resolve_portrait(theme_slug: &str, agent: &Character) -> PortraitPaths {
    let theme_dir = portrait_cache_dir().join(theme_slug);
    resolve_portrait_in_dir(&theme_dir, agent)
}

/// Resolve portrait paths against an explicit theme directory. Test
/// surface for `resolve_portrait` — swap the cache root without touching
/// XDG env vars. Caller is responsible for the theme-dir layout
/// (`<theme_dir>/{small,medium,large,original}/<stem>[-suffix].png`).
fn resolve_portrait_in_dir(theme_dir: &Path, agent: &Character) -> PortraitPaths {
    let mut paths = PortraitPaths {
        small: None,
        medium: None,
        large: None,
        original: None,
    };

    if !theme_dir.exists() {
        return paths;
    }

    // Build candidate stems from character fields.
    let mut stems: Vec<String> = Vec::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_stem_lowercases_and_slugifies() {
        assert_eq!(normalize_stem("Granny Weatherwax"), "granny-weatherwax");
        assert_eq!(normalize_stem("DEATH"), "death");
        assert_eq!(normalize_stem("Lu-Tze"), "lu-tze");
        assert_eq!(
            normalize_stem("Lord Havelock Vetinari"),
            "lord-havelock-vetinari"
        );
    }

    #[test]
    fn normalize_stem_strips_punctuation() {
        assert_eq!(
            normalize_stem("Dr. Leonard \"Bones\" McCoy"),
            "dr-leonard-bones-mccoy"
        );
        assert_eq!(normalize_stem("B.A. Baracus"), "ba-baracus");
    }

    #[test]
    fn resolve_portrait_no_cache_returns_empty() {
        // Nonexistent theme dir — no ambient state required.
        let character = make_character("Granny Weatherwax", Some("Granny"));
        let paths = resolve_portrait("__nonexistent_theme_fixture__", &character);
        assert!(!paths.has_any());
    }

    /// Test helper: build a Character with just the fields portrait
    /// resolution actually consumes (character + short_name).
    fn make_character(name: &str, short: Option<&str>) -> Character {
        Character {
            character: name.to_string(),
            short_name: short.map(str::to_string),
            visual: None,
            ocean: None,
            style: String::new(),
            expertise: String::new(),
            r#trait: String::new(),
            backstory_role: String::new(),
            backstory_role_description: String::new(),
            quirks: Vec::new(),
            catchphrases: Vec::new(),
            emoji: None,
            helper: None,
        }
    }

    /// Lay down `<theme_dir>/<size>/<filename>` (zero-byte file is enough
    /// for resolution — we never read content).
    fn touch(theme_dir: &Path, size: &str, filename: &str) -> PathBuf {
        let dir = theme_dir.join(size);
        fs::create_dir_all(&dir).expect("create size dir");
        let path = dir.join(filename);
        fs::write(&path, b"").expect("write portrait stub");
        path
    }

    /// Regression test for the granny→ponder bug class
    /// (orc finding-033, B14 taxonomy cleanup).
    ///
    /// Two characters in the same theme — Granny Weatherwax and Ponder
    /// Stibbons — each must resolve to her/his own portrait file. The
    /// pre-fix code path consulted a manifest.json role-key index and
    /// could return the wrong character's portrait when --persona and
    /// --role pointed at different characters. Today the resolver
    /// derives stems from the Character only, so role cannot influence
    /// the result.
    #[test]
    fn resolve_portrait_returns_each_characters_own_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let theme_dir = tmp.path().join("discworld");
        let granny_file = touch(&theme_dir, "large", "granny-35211.png");
        let ponder_file = touch(&theme_dir, "large", "ponder-55233.png");

        let granny = make_character("Granny Weatherwax", Some("Granny"));
        let ponder = make_character("Ponder Stibbons", Some("Ponder"));

        let granny_paths = resolve_portrait_in_dir(&theme_dir, &granny);
        assert_eq!(
            granny_paths.large.as_deref(),
            Some(granny_file.as_path()),
            "Granny must resolve to her own file, not Ponder's"
        );

        let ponder_paths = resolve_portrait_in_dir(&theme_dir, &ponder);
        assert_eq!(
            ponder_paths.large.as_deref(),
            Some(ponder_file.as_path()),
            "Ponder must resolve to his own file, not Granny's"
        );
    }

    /// CDN portraits are written as `<stem>-<hash>.png` (e.g.
    /// `granny-35211.png`). Resolution must find them via prefix match
    /// on the unhashed stem.
    #[test]
    fn resolve_portrait_prefix_matches_hashed_filenames() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let theme_dir = tmp.path().join("discworld");
        let file = touch(&theme_dir, "large", "granny-35211.png");

        let granny = make_character("Granny Weatherwax", Some("Granny"));
        let paths = resolve_portrait_in_dir(&theme_dir, &granny);
        assert_eq!(paths.large.as_deref(), Some(file.as_path()));
    }

    /// short_name is the first stem candidate; an exact `<short>.png`
    /// must win over the full-name file.
    #[test]
    fn resolve_portrait_short_name_wins_over_full_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let theme_dir = tmp.path().join("discworld");
        let short_file = touch(&theme_dir, "large", "granny.png");
        let _full_file = touch(&theme_dir, "large", "granny-weatherwax.png");

        let granny = make_character("Granny Weatherwax", Some("Granny"));
        let paths = resolve_portrait_in_dir(&theme_dir, &granny);
        assert_eq!(
            paths.large.as_deref(),
            Some(short_file.as_path()),
            "exact short_name match must win over full-name match"
        );
    }

    /// best_for_size falls through the preference chain
    /// (small → medium → large → original) when the requested size is
    /// missing.
    #[test]
    fn resolve_portrait_size_fallback_picks_next_available() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let theme_dir = tmp.path().join("discworld");
        let large_file = touch(&theme_dir, "large", "granny-35211.png");

        let granny = make_character("Granny Weatherwax", Some("Granny"));
        let paths = resolve_portrait_in_dir(&theme_dir, &granny);
        assert_eq!(paths.best_for_size("small"), Some(large_file.as_path()));
        assert_eq!(paths.best_for_size("medium"), Some(large_file.as_path()));
        assert_eq!(paths.best_for_size("large"), Some(large_file.as_path()));
    }

    /// Confirms — at the type level, not just by behavior — that
    /// resolution depends only on theme directory + Character, never
    /// on a role string.
    #[test]
    fn resolve_portrait_signature_does_not_take_a_role() {
        // If someone reintroduces a role parameter, this test stops
        // compiling. Documents the B14 contract: role is a job
        // assignment, never a portrait selector.
        fn _accepts_only_dir_and_character(dir: &Path, agent: &Character) -> PortraitPaths {
            resolve_portrait_in_dir(dir, agent)
        }
    }
}
