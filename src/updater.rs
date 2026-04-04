use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::error::{AclaudeError, Result};

/// GitHub release metadata (subset).
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    published_at: String,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

/// GitHub release asset metadata.
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
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

/// How aclaude was installed — determines update strategy.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallMethod {
    /// Installed via Homebrew (macOS). User should run `brew upgrade`.
    Homebrew,
    /// Installed via a Linux system package manager (apt/dpkg, yum/rpm, apk).
    LinuxPackageManager { manager: String },
    /// Direct binary install — self-update is possible.
    DirectBinary,
}

/// Version directory: ~/.local/share/aclaude/versions/{version}/
fn versions_dir() -> PathBuf {
    crate::paths::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("aclaude/versions")
}

/// Build the HTTP client used for GitHub API and asset downloads.
fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("aclaude-updater")
        .build()
        .map_err(|e| AclaudeError::Update {
            message: format!("http client error: {e}"),
        })
}

/// Detect how aclaude was installed by examining the binary path.
pub fn detect_install_method() -> Result<InstallMethod> {
    let exe = env::current_exe().map_err(|e| AclaudeError::Update {
        message: format!("cannot determine binary path: {e}"),
    })?;

    let path_str = exe.to_string_lossy();

    // macOS Homebrew: path contains /Cellar/ or /homebrew/
    if path_str.contains("/Cellar/") || path_str.contains("/homebrew/") {
        return Ok(InstallMethod::Homebrew);
    }

    // Linux package managers: check if the binary is managed by dpkg, rpm, or apk
    if cfg!(target_os = "linux") {
        if let Some(manager) = detect_linux_package_manager(&path_str) {
            return Ok(InstallMethod::LinuxPackageManager { manager });
        }
    }

    Ok(InstallMethod::DirectBinary)
}

/// Check if a Linux package manager owns the binary path.
fn detect_linux_package_manager(binary_path: &str) -> Option<String> {
    // dpkg -S <path> — exits 0 if the file is owned by a package
    if Command::new("dpkg")
        .args(["-S", binary_path])
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some("apt".to_string());
    }

    // rpm -qf <path> — exits 0 if the file is owned by a package
    if Command::new("rpm")
        .args(["-qf", binary_path])
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some("yum/dnf".to_string());
    }

    // apk info --who-owns <path> — Alpine
    if Command::new("apk")
        .args(["info", "--who-owns", binary_path])
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some("apk".to_string());
    }

    None
}

/// Determine the expected asset name for the current platform.
fn asset_name() -> Result<String> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(AclaudeError::Update {
            message: format!("unsupported OS: {}", env::consts::OS),
        });
    };

    let arch = match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => {
            return Err(AclaudeError::Update {
                message: format!("unsupported architecture: {other}"),
            });
        }
    };

    // Asset naming convention: aclaude-a-{os}-{arch}
    Ok(format!("aclaude-a-{os}-{arch}"))
}

/// Check for available updates on GitHub.
pub fn check_for_update(channel: Channel) -> Result<Option<String>> {
    let client = http_client()?;

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

/// Download and install the latest release, replacing the current binary.
///
/// Returns the new version tag on success.
pub fn download_and_install(channel: Channel) -> Result<String> {
    let client = http_client()?;

    // Fetch releases
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

    let release = releases
        .iter()
        .filter(|r| match channel {
            Channel::Stable => !r.prerelease,
            Channel::Alpha => r.prerelease,
        })
        .max_by_key(|r| &r.published_at)
        .ok_or_else(|| AclaudeError::Update {
            message: format!(
                "no {} releases found",
                if channel == Channel::Stable {
                    "stable"
                } else {
                    "alpha"
                }
            ),
        })?;

    // Find the right asset for this platform
    let expected_asset = asset_name()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected_asset)
        .ok_or_else(|| AclaudeError::Update {
            message: format!(
                "no asset named '{expected_asset}' in release {}. Available: {}",
                release.tag_name,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })?;

    // Determine current binary path
    let current_exe = env::current_exe().map_err(|e| AclaudeError::Update {
        message: format!("cannot determine binary path: {e}"),
    })?;

    // Resolve symlinks to get the actual binary location
    let current_exe = current_exe
        .canonicalize()
        .map_err(|e| AclaudeError::Update {
            message: format!("cannot resolve binary path: {e}"),
        })?;

    let parent_dir = current_exe.parent().ok_or_else(|| AclaudeError::Update {
        message: "binary has no parent directory".to_string(),
    })?;

    // Check we can write to the directory
    let test_path = parent_dir.join(".aclaude-update-test");
    fs::write(&test_path, b"test").map_err(|e| AclaudeError::Update {
        message: format!(
            "cannot write to {}: {e}\n\nTry running with sudo, or move the binary to a user-writable location.",
            parent_dir.display()
        ),
    })?;
    let _ = fs::remove_file(&test_path);

    let new_path = parent_dir.join(format!(
        "{}.new",
        current_exe
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| "aclaude".into())
    ));
    let old_path = parent_dir.join(format!(
        "{}.old",
        current_exe
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| "aclaude".into())
    ));

    // Download the asset to a temp file
    println!("Downloading {}...", asset.name);
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|e| AclaudeError::Update {
            message: format!("download failed: {e}"),
        })?;

    if !response.status().is_success() {
        return Err(AclaudeError::Update {
            message: format!(
                "download failed with HTTP {}: {}",
                response.status(),
                asset.browser_download_url
            ),
        });
    }

    let bytes = response.bytes().map_err(|e| AclaudeError::Update {
        message: format!("failed to read download body: {e}"),
    })?;

    if bytes.is_empty() {
        return Err(AclaudeError::Update {
            message: "downloaded file is empty".to_string(),
        });
    }

    // Write to .new file
    {
        let mut file = fs::File::create(&new_path).map_err(|e| AclaudeError::Update {
            message: format!("cannot create {}: {e}", new_path.display()),
        })?;
        file.write_all(&bytes).map_err(|e| {
            // Clean up on write failure
            let _ = fs::remove_file(&new_path);
            AclaudeError::Update {
                message: format!("failed to write {}: {e}", new_path.display()),
            }
        })?;
        file.flush().map_err(|e| {
            let _ = fs::remove_file(&new_path);
            AclaudeError::Update {
                message: format!("failed to flush {}: {e}", new_path.display()),
            }
        })?;
    }

    // Set executable permissions (rwxr-xr-x = 0o755)
    fs::set_permissions(&new_path, fs::Permissions::from_mode(0o755)).map_err(|e| {
        let _ = fs::remove_file(&new_path);
        AclaudeError::Update {
            message: format!("cannot set permissions on {}: {e}", new_path.display()),
        }
    })?;

    // Atomic swap sequence:
    // 1. Rename current binary to .old (backup)
    // 2. Rename .new to current binary path
    // 3. Remove .old on success

    // Step 1: current -> .old
    fs::rename(&current_exe, &old_path).map_err(|e| {
        let _ = fs::remove_file(&new_path);
        AclaudeError::Update {
            message: format!(
                "cannot backup current binary to {}: {e}",
                old_path.display()
            ),
        }
    })?;

    // Step 2: .new -> current
    if let Err(e) = fs::rename(&new_path, &current_exe) {
        // Rollback: restore the old binary
        let _ = fs::rename(&old_path, &current_exe);
        let _ = fs::remove_file(&new_path);
        return Err(AclaudeError::Update {
            message: format!(
                "cannot install new binary to {}: {e}",
                current_exe.display()
            ),
        });
    }

    // Step 3: clean up .old
    let _ = fs::remove_file(&old_path);

    Ok(release.tag_name.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_parse_stable() {
        assert_eq!(Channel::parse("stable"), Channel::Stable);
    }

    #[test]
    fn channel_parse_alpha() {
        assert_eq!(Channel::parse("alpha"), Channel::Alpha);
    }

    #[test]
    fn channel_parse_unknown_defaults_to_alpha() {
        assert_eq!(Channel::parse("nightly"), Channel::Alpha);
    }

    #[test]
    fn channel_binary_name() {
        assert_eq!(Channel::Stable.binary_name(), "aclaude");
        assert_eq!(Channel::Alpha.binary_name(), "aclaude-a");
    }

    #[test]
    fn asset_name_is_valid_format() {
        let name = asset_name().expect("should detect platform");
        assert!(
            name.starts_with("aclaude-a-"),
            "unexpected asset name: {name}"
        );
        // Should be aclaude-a-{os}-{arch}
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 4, "expected 4 dash-separated parts: {name}");
        assert!(
            parts[2] == "darwin" || parts[2] == "linux",
            "unexpected os: {}",
            parts[2]
        );
        assert!(
            parts[3] == "arm64" || parts[3] == "amd64",
            "unexpected arch: {}",
            parts[3]
        );
    }

    #[test]
    fn detect_install_method_for_current_binary() {
        // Running from a cargo build dir, should be DirectBinary
        let method = detect_install_method().expect("should detect install method");
        assert_eq!(method, InstallMethod::DirectBinary);
    }

    #[test]
    fn github_release_deserializes_with_assets() {
        let json = r#"{
            "tag_name": "v0.1.0",
            "prerelease": false,
            "published_at": "2026-04-01T00:00:00Z",
            "assets": [
                {
                    "name": "aclaude-a-darwin-arm64",
                    "browser_download_url": "https://example.com/aclaude-a-darwin-arm64"
                }
            ]
        }"#;
        let release: GitHubRelease =
            serde_json::from_str(json).expect("should deserialize release");
        assert_eq!(release.tag_name, "v0.1.0");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "aclaude-a-darwin-arm64");
    }

    #[test]
    fn github_release_deserializes_without_assets() {
        let json = r#"{
            "tag_name": "v0.1.0",
            "prerelease": true,
            "published_at": "2026-04-01T00:00:00Z"
        }"#;
        let release: GitHubRelease =
            serde_json::from_str(json).expect("should deserialize release without assets");
        assert!(release.assets.is_empty());
    }
}
