use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::ForestageConfig;

/// ANSI color codes for terminal output.
mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const DIM: &str = "\x1b[2m";
    pub const BOLD: &str = "\x1b[1m";
    pub const FG_CYAN: &str = "\x1b[36m";
    pub const FG_GREEN: &str = "\x1b[32m";
    pub const FG_YELLOW: &str = "\x1b[33m";
    pub const FG_RED: &str = "\x1b[31m";
    pub const FG_GRAY: &str = "\x1b[90m";
}

/// Git branch and dirty state.
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
}

/// Get git info for the current directory.
pub fn get_git_info() -> Option<GitInfo> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .is_some_and(|o| !o.stdout.is_empty());

    Some(GitInfo { branch, dirty })
}

/// Build a progress bar string.
pub fn build_progress_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let color = if pct > 95.0 {
        ansi::FG_RED
    } else if pct > 70.0 {
        ansi::FG_YELLOW
    } else {
        ansi::FG_GREEN
    };

    format!(
        "{color}{filled_bar}{fg_gray}{empty_bar}{reset} {pct:.0}%",
        filled_bar = "█".repeat(filled),
        empty_bar = "░".repeat(empty),
        fg_gray = ansi::FG_GRAY,
        reset = ansi::RESET,
        color = color,
    )
}

/// Render the statusline string.
pub fn render_statusline(
    config: &ForestageConfig,
    character_name: &str,
    context_pct: Option<f64>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Character name
    parts.push(format!(
        "{}{}{character_name}{}",
        ansi::BOLD,
        ansi::FG_CYAN,
        ansi::RESET,
    ));

    // Working directory
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(dirname) = cwd.file_name() {
            parts.push(format!(
                "{}{}{}",
                ansi::DIM,
                dirname.to_string_lossy(),
                ansi::RESET,
            ));
        }
    }

    // Git info
    if config.statusline.git_info {
        if let Some(git) = get_git_info() {
            let dirty_marker = if git.dirty { "*" } else { "" };
            parts.push(format!(
                "{}{}{dirty_marker}{}",
                ansi::FG_GREEN,
                git.branch,
                ansi::RESET,
            ));
        }
    }

    // Model
    parts.push(format!(
        "{}{}{}",
        ansi::DIM,
        config.session.model,
        ansi::RESET,
    ));

    // Context bar
    if config.statusline.context_bar {
        if let Some(pct) = context_pct {
            parts.push(build_progress_bar(pct, 10));
        }
    }

    parts.join(&format!(" {}|{} ", ansi::DIM, ansi::RESET))
}

/// Push statusline content to tmux via control mode if a client is available,
/// otherwise fall back to writing cache files.
pub fn push_statusline(
    left: &str,
    right: &str,
    client: Option<&tmux_cmc::Client>,
    session: Option<&tmux_cmc::SessionId>,
) {
    if let (Some(client), Some(session)) = (client, session) {
        // Real-time push via control mode — no polling delay
        let _ = client.set_status_left(session, left);
        let _ = client.set_status_right(session, right);
    } else {
        // Fallback: write cache files for tmux to poll
        write_tmux_cache(left, right);
    }
}

/// Write tmux status cache files for polling (legacy fallback).
pub fn write_tmux_cache(left: &str, right: &str) {
    let cache_dir = Path::new(".forestage");
    let _ = fs::create_dir_all(cache_dir);
    let _ = fs::write(cache_dir.join("tmux-status-left"), left);
    let _ = fs::write(cache_dir.join("tmux-status-right"), right);
}
