use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AclaudeError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
}

fn default_model() -> String {
    "claude-sonnet-4-6".to_string()
}
fn default_max_tokens() -> u64 {
    16384
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_immersion")]
    pub immersion: String,
}

fn default_theme() -> String {
    "the-expanse".to_string()
}
fn default_role() -> String {
    "dev".to_string()
}
fn default_immersion() -> String {
    "high".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatuslineConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub git_info: bool,
    #[serde(default = "default_true")]
    pub context_bar: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub otel_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxConfig {
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_socket")]
    pub socket: String,
}

fn default_layout() -> String {
    "bottom".to_string()
}
fn default_socket() -> String {
    "ac".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AclaudeConfig {
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub persona: PersonaConfig,
    #[serde(default)]
    pub statusline: StatuslineConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub tmux: TmuxConfig,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for PersonaConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            role: default_role(),
            immersion: default_immersion(),
        }
    }
}

impl Default for StatuslineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            git_info: true,
            context_bar: true,
        }
    }
}

impl Default for TmuxConfig {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            socket: default_socket(),
        }
    }
}

/// Paths for each config layer.
pub struct ConfigPaths {
    pub defaults: PathBuf,
    pub global: PathBuf,
    pub local: PathBuf,
}

fn xdg_config_home() -> PathBuf {
    env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            crate::paths::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        })
}

pub fn config_paths() -> ConfigPaths {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ConfigPaths {
        defaults: PathBuf::from("config/defaults.toml"),
        global: xdg_config_home().join("aclaude/config.toml"),
        local: cwd.join(".aclaude/config.toml"),
    }
}

/// Load a TOML file into a generic table. Returns empty table if missing.
/// Warns on stderr if the file exists but fails to parse.
fn load_toml_table(path: &Path) -> toml::Table {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return toml::Table::new(),
    };
    match content.parse::<toml::Table>() {
        Ok(table) => table,
        Err(e) => {
            eprintln!("warning: failed to parse {}: {e}", path.display());
            toml::Table::new()
        }
    }
}

/// Deep-merge two TOML tables. `source` values override `target`.
fn deep_merge(target: &mut toml::Table, source: &toml::Table) {
    for (key, src_val) in source {
        match (target.get_mut(key), src_val) {
            (Some(toml::Value::Table(t)), toml::Value::Table(s)) => {
                deep_merge(t, s);
            }
            _ => {
                target.insert(key.clone(), src_val.clone());
            }
        }
    }
}

/// Apply ACLAUDE_* environment variable overrides.
///
/// Format: `ACLAUDE_SECTION__FIELD=value`
/// Double underscore separates section from field.
fn apply_env_overrides(table: &mut toml::Table) {
    let prefix = "ACLAUDE_";
    for (key, value) in env::vars() {
        if !key.starts_with(prefix) {
            continue;
        }
        let rest = &key[prefix.len()..];
        let parts: Vec<&str> = rest.splitn(2, "__").collect();
        if parts.len() != 2 {
            continue;
        }
        let section = parts[0].to_lowercase();
        let field = parts[1].to_lowercase();

        let section_table = table
            .entry(section)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(t) = section_table {
            let parsed: toml::Value = match value.as_str() {
                "true" => toml::Value::Boolean(true),
                "false" => toml::Value::Boolean(false),
                v if v.parse::<i64>().is_ok() => {
                    toml::Value::Integer(v.parse::<i64>().expect("already checked"))
                }
                _ => toml::Value::String(value),
            };
            t.insert(field, parsed);
        }
    }
}

/// Load the full config with 5-layer merge:
/// defaults -> global -> local -> env -> CLI overrides
pub fn load_config(overrides: Option<&toml::Table>) -> Result<AclaudeConfig> {
    let paths = config_paths();

    // Layer 1: built-in defaults (from Default impl) + file defaults
    let mut table = toml::to_string(&AclaudeConfig::default())
        .expect("default config serializes")
        .parse::<toml::Table>()
        .expect("default config parses");

    let file_defaults = load_toml_table(&paths.defaults);
    if !file_defaults.is_empty() {
        deep_merge(&mut table, &file_defaults);
    }

    // Layer 2: global user config
    let global = load_toml_table(&paths.global);
    if !global.is_empty() {
        deep_merge(&mut table, &global);
    }

    // Layer 3: local project config
    let local = load_toml_table(&paths.local);
    if !local.is_empty() {
        deep_merge(&mut table, &local);
    }

    // Layer 4: environment overrides
    apply_env_overrides(&mut table);

    // Layer 5: CLI overrides
    if let Some(cli) = overrides {
        deep_merge(&mut table, cli);
    }

    let config: AclaudeConfig =
        toml::Value::Table(table)
            .try_into()
            .map_err(|e| AclaudeError::Config {
                message: format!("config merge failed: {e}"),
            })?;

    Ok(config)
}
