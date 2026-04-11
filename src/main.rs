#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use forestage::config;
use forestage::persona;
use forestage::portrait;
use forestage::session;
use forestage::session_cmd;
use forestage::tui;
use forestage::updater;

/// Build-time version info injected by build.rs.
const VERSION: &str = env!("FORESTAGE_VERSION");
const COMMIT: &str = env!("FORESTAGE_COMMIT");
const BUILD_TIME: &str = env!("FORESTAGE_BUILD_TIME");
const CHANNEL: &str = env!("FORESTAGE_CHANNEL");
#[derive(Parser)]
#[command(
    name = "forestage",
    version = env!("FORESTAGE_LONG_VERSION"),
    about = "Opinionated Claude Code distribution with persona theming"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Model override
    #[arg(short = 'm', long)]
    model: Option<String>,

    /// Theme override
    #[arg(short = 't', long)]
    theme: Option<String>,

    /// Role override
    #[arg(short = 'r', long)]
    role: Option<String>,

    /// Immersion level override
    #[arg(short = 'i', long)]
    immersion: Option<String>,

    /// One-shot prompt (non-interactive, like claude -p)
    #[arg(short = 'p', long)]
    prompt: Option<String>,

    /// Output format for one-shot prompts: text (default), json, stream-json
    #[arg(long, default_value = "text")]
    output_format: String,

    /// Use NDJSON streaming protocol (agent/programmatic mode)
    #[arg(long)]
    streaming: bool,

    /// Interactive mode: "forestage" (custom TUI) or "claude" (native Claude Code TUI)
    #[arg(long)]
    mode: Option<String>,

    /// Arguments passed through to the claude CLI (after --)
    #[arg(last = true)]
    claude_args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show resolved configuration
    Config,

    /// Persona management
    Persona {
        #[command(subcommand)]
        action: PersonaAction,
    },

    /// Manage the forestage tmux session
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Update to the latest release, or to a specific version
    Update {
        /// Target version tag (e.g., alpha-20260405-075244-abc1234). Omit for latest.
        version: Option<String>,
    },

    /// Show version details
    Version,

    /// List installed versions
    Versions {
        /// Clean old versions, keeping N most recent
        #[arg(long)]
        clean: Option<usize>,
    },

    /// Launch interactive TUI (prototype)
    Tui,
}

#[derive(Subcommand)]
enum SessionAction {
    /// Start an forestage tmux session
    Start {
        /// Session name (default: forestage-{petname})
        #[arg(short = 't', long = "session-name")]
        name: Option<String>,
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
        /// Don't attach terminal to the session after starting
        #[arg(long)]
        no_attach: bool,
    },
    /// Attach to an existing forestage session
    Attach {
        /// Session name (auto-selects if only one exists)
        #[arg(short = 't', long = "session-name")]
        name: Option<String>,
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
    },
    /// Stop (kill) an forestage session
    Stop {
        /// Session name (auto-selects if only one exists)
        #[arg(short = 't', long = "session-name")]
        name: Option<String>,
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
        /// Stop all sessions on the socket
        #[arg(long)]
        all: bool,
    },
    /// List forestage sessions
    List {
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
        /// Show only session names (no details)
        #[arg(long)]
        names: bool,
        /// Include control sessions
        #[arg(long)]
        all: bool,
    },
    /// Show session status
    Status {
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
        /// Include control sessions
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum PersonaAction {
    /// List available themes
    List,

    /// Show theme details
    Show {
        /// Theme slug
        name: String,

        /// Show specific agent role
        #[arg(long, default_value = "dev")]
        agent: String,

        /// Display portrait inline (Kitty/Ghostty terminals)
        #[arg(short = 'p', long)]
        portrait: bool,

        /// Portrait position relative to info card
        #[arg(long, default_value = "top")]
        portrait_position: String,

        /// Portrait alignment
        #[arg(long, default_value = "left")]
        portrait_align: String,

        /// Portrait size
        #[arg(long, default_value = "original")]
        portrait_size: String,
    },

    /// Show portrait cache status
    Portraits,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Build CLI overrides table
    let mut overrides = toml::Table::new();
    if cli.model.is_some()
        || cli.theme.is_some()
        || cli.role.is_some()
        || cli.immersion.is_some()
        || cli.mode.is_some()
    {
        let mut session_overrides = toml::Table::new();
        if let Some(model) = &cli.model {
            session_overrides.insert("model".to_string(), toml::Value::String(model.clone()));
        }
        if let Some(mode) = &cli.mode {
            session_overrides.insert("mode".to_string(), toml::Value::String(mode.clone()));
        }
        if !session_overrides.is_empty() {
            overrides.insert("session".to_string(), toml::Value::Table(session_overrides));
        }
        let mut persona_overrides = toml::Table::new();
        if let Some(theme) = &cli.theme {
            persona_overrides.insert("theme".to_string(), toml::Value::String(theme.clone()));
        }
        if let Some(role) = &cli.role {
            persona_overrides.insert("role".to_string(), toml::Value::String(role.clone()));
        }
        if let Some(immersion) = &cli.immersion {
            persona_overrides.insert(
                "immersion".to_string(),
                toml::Value::String(immersion.clone()),
            );
        }
        if !persona_overrides.is_empty() {
            overrides.insert("persona".to_string(), toml::Value::Table(persona_overrides));
        }
    }

    let cli_overrides = if overrides.is_empty() {
        None
    } else {
        Some(&overrides)
    };

    match cli.command {
        None => {
            let cfg = config::load_config(cli_overrides)?;
            if let Some(prompt) = &cli.prompt {
                // One-shot prompt mode
                let result =
                    session::run_prompt(&cfg, prompt, &cli.output_format, &cli.claude_args)?;
                print!("{result}");
            } else if cli.streaming {
                // NDJSON streaming protocol (agent/programmatic mode)
                let usage = session::start_streaming_session(&cfg, &cli.claude_args)?;
                usage.print_summary();
            } else {
                // Default: interactive session — mode selects TUI
                match cfg.session.mode.as_str() {
                    "claude" => {
                        // Native Claude Code TUI (inherited stdio)
                        session::start_session(&cfg, &cli.claude_args)?;
                    }
                    _ => {
                        // forestage TUI (custom ratatui over NDJSON)
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map_err(|e| forestage::error::ForestageError::Session {
                                message: format!("failed to create async runtime: {e}"),
                            })?;
                        rt.block_on(tui::run_tui(&cfg))?;
                    }
                }
            }
        }

        Some(Commands::Config) => {
            let cfg = config::load_config(cli_overrides)?;
            let paths = config::config_paths();
            println!("Config paths:");
            println!("  defaults: {}", paths.defaults.display());
            println!("  global:   {}", paths.global.display());
            println!("  local:    {}", paths.local.display());
            println!();
            println!("{}", toml::to_string_pretty(&cfg)?);
        }

        Some(Commands::Persona { action }) => match action {
            PersonaAction::List => {
                let themes = persona::list_themes();
                println!("{} themes available:", themes.len());
                for slug in &themes {
                    if let Ok(theme) = persona::load_theme(slug) {
                        println!("  {:<30} {}", slug, theme.theme.description);
                    } else {
                        println!("  {slug}");
                    }
                }
            }
            PersonaAction::Show {
                name,
                agent,
                portrait: show_portrait,
                portrait_position,
                portrait_align,
                portrait_size,
            } => {
                let cfg = config::load_config(cli_overrides)?;
                let theme = persona::load_theme(&name)?;
                let agent_data = persona::get_agent(&theme, &agent)?;
                let portraits = portrait::resolve_portrait(&name, agent_data, Some(&agent));

                // Portrait before card (position: top)
                if show_portrait && portrait_position == "top" {
                    if let Some(path) = portraits.best_for_size(&portrait_size) {
                        if !portrait::display_portrait(path, &portrait_align, &cfg.portrait) {
                            println!("(terminal does not support inline images)");
                        }
                        println!();
                    }
                }

                // Info card
                println!("Theme: {} ({})", theme.theme.name, theme.category);
                println!("Description: {}", theme.theme.description);
                if let Some(title) = &theme.theme.user_title {
                    println!("User title: {title}");
                }
                println!("Roles: {}", {
                    let mut roles: Vec<_> = theme.agents.keys().collect();
                    roles.sort();
                    roles
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                println!();
                println!("Agent: {} (role: {agent})", agent_data.character);
                println!("  Style: {}", agent_data.style);
                println!("  Expertise: {}", agent_data.expertise);
                println!("  Trait: {}", agent_data.r#trait);
                if !agent_data.quirks.is_empty() {
                    println!("  Quirks: {}", agent_data.quirks.join("; "));
                }
                if !agent_data.catchphrases.is_empty() {
                    println!("  Catchphrases:");
                    for phrase in &agent_data.catchphrases {
                        println!("    - \"{phrase}\"");
                    }
                }
                if portraits.has_any() {
                    println!("  Portraits: [{}]", portraits.available_sizes().join(", "));
                }

                // Portrait after card (position: bottom)
                if show_portrait && portrait_position == "bottom" {
                    println!();
                    if let Some(path) = portraits.best_for_size(&portrait_size) {
                        if !portrait::display_portrait(path, &portrait_align, &cfg.portrait) {
                            println!("(terminal does not support inline images)");
                        }
                    }
                }
            }
            PersonaAction::Portraits => {
                let cache_dir = portrait::portrait_cache_dir();
                let (themes, images) = portrait::cache_status();
                println!("Portrait cache: {}", cache_dir.display());
                println!("Themes with portraits: {themes}");
                println!("Total images: {images}");
            }
        },

        Some(Commands::Session { action }) => {
            let cfg = config::load_config(cli_overrides)?;
            match action {
                SessionAction::Start {
                    name,
                    socket,
                    no_attach,
                } => {
                    session_cmd::run_session_start(
                        &cfg,
                        socket.as_deref(),
                        name.as_deref(),
                        !no_attach,
                    )?;
                }
                SessionAction::Attach { name, socket } => {
                    session_cmd::run_session_attach(&cfg, socket.as_deref(), name.as_deref())?;
                }
                SessionAction::Stop { name, socket, all } => {
                    session_cmd::run_session_stop(&cfg, socket.as_deref(), name.as_deref(), all)?;
                }
                SessionAction::List { socket, names, all } => {
                    session_cmd::run_session_list(&cfg, socket.as_deref(), names, all)?;
                }
                SessionAction::Status { socket, all } => {
                    session_cmd::run_session_status(&cfg, socket.as_deref(), all)?;
                }
            }
        }

        Some(Commands::Update { version }) => {
            let channel = updater::Channel::parse(CHANNEL);
            let channel_label = if channel == updater::Channel::Stable {
                "stable"
            } else {
                "alpha"
            };

            // Check how forestage was installed
            let bin = updater::binary_name();
            match updater::detect_install_method()? {
                updater::InstallMethod::Homebrew => {
                    let formula = if bin.starts_with("forestage-a") {
                        "ArcavenAE/tap/forestage-a"
                    } else {
                        "ArcavenAE/tap/forestage"
                    };
                    println!("forestage was installed via Homebrew.");
                    println!("Run: brew upgrade {formula}");
                    return Ok(());
                }
                updater::InstallMethod::LinuxPackageManager { manager } => {
                    println!("forestage was installed via {manager}.");
                    println!("Update using your package manager.");
                    return Ok(());
                }
                updater::InstallMethod::DirectBinary => {}
            }

            let current_tag = env!("FORESTAGE_TAG");
            let target = version.as_deref();

            println!("Checking for updates ({channel_label})...");
            match updater::check_for_update(channel, target)? {
                Some(tag) if tag == current_tag => {
                    println!("Already up to date: {current_tag}");
                }
                Some(tag) => {
                    if target.is_some() {
                        println!("Current: {current_tag}");
                        println!("Target:  {tag}");
                    } else {
                        println!("Latest: {tag} (current: {current_tag})");
                    }
                    println!("Downloading and installing {tag}...");
                    let new_tag = updater::download_and_install(channel, target)?;
                    println!("Updated to {new_tag}. Restart to use the new version.");
                }
                None => {
                    if let Some(v) = target {
                        println!("No release matching '{v}' found.");
                    } else {
                        println!("No {channel_label} releases found.");
                    }
                }
            }
        }

        Some(Commands::Version) => {
            println!("forestage {VERSION}");
            println!("  commit:  {COMMIT}");
            println!("  built:   {BUILD_TIME}");
            println!("  channel: {CHANNEL}");
        }

        Some(Commands::Tui) => {
            let cfg = config::load_config(cli_overrides)?;
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| forestage::error::ForestageError::Session {
                    message: format!("failed to create async runtime: {e}"),
                })?;
            rt.block_on(tui::run_tui(&cfg))?;
        }

        Some(Commands::Versions { clean }) => {
            if let Some(keep) = clean {
                let removed = updater::clean_old_versions(keep)?;
                println!("Removed {removed} old version(s).");
            }
            let versions = updater::list_versions()?;
            if versions.is_empty() {
                println!("No installed versions found.");
            } else {
                println!("Installed versions:");
                for v in &versions {
                    println!("  {v}");
                }
            }
        }
    }

    Ok(())
}
