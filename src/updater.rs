use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{AclaudeError, Result};

/// GitHub release metadata (subset).
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    published_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Alpha,
}

impl Channel {
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Stable => "aclaude",
            Self::Alpha => "aclaude-a",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "stable" => Self::Stable,
            _ => Self::Alpha,
        }
    }
}

/// Version directory: ~/.local/share/aclaude/versions/{version}/
fn versions_dir() -> PathBuf {
    crate::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("aclaude/versions")
}

/// Check for available updates on GitHub.
pub fn check_for_update(channel: Channel) -> Result<Option<String>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("aclaude-updater")
        .build()
        .map_err(|e| AclaudeError::Update {
            message: format!("http client error: {e}"),
        })?;

    let releases: Vec<GitHubRelease> = client
        .get("https://api.github.com/repos/ArcavenAE/aclaude/releases")
        .send()
        .map_err(|e| AclaudeError::Update {
            message: format!("failed to fetch releases: {e}"),
        })?
        .json()
        .map_err(|e| AclaudeError::Update {
            message: format!("failed to parse releases: {e}"),
        })?;

    let latest = releases
        .iter()
        .filter(|r| match channel {
            Channel::Stable => !r.prerelease,
            Channel::Alpha => r.prerelease,
        })
        .max_by_key(|r| &r.published_at);

    Ok(latest.map(|r| r.tag_name.clone()))
}

/// List installed versions.
pub fn list_versions() -> Result<Vec<String>> {
    let dir = versions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions: Vec<String> = fs::read_dir(&dir)
        .map_err(AclaudeError::Io)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    versions.sort();
    versions.reverse();
    Ok(versions)
}

/// Clean old versions, keeping the most recent `keep` entries.
pub fn clean_old_versions(keep: usize) -> Result<usize> {
    let versions = list_versions()?;
    let to_remove = versions.iter().skip(keep);
    let mut removed = 0;

    let dir = versions_dir();
    for version in to_remove {
        let path = dir.join(version);
        if fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }

    Ok(removed)
}
