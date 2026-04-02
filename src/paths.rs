use std::env;
use std::path::PathBuf;

/// Returns the user's home directory via `$HOME`.
pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

/// Returns the XDG data directory (`$XDG_DATA_HOME` or `~/.local/share`).
pub fn data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
}
