#![forbid(unsafe_code)]

use aclaude::config;
use aclaude::persona;
use aclaude::portrait;
use aclaude::session;
use aclaude::updater;
use clap::{Parser, Subcommand};

/// Build-time version info injected by build.rs.
const VERSION: &str = env!("ACLAUDE_VERSION");
const COMMIT: &str = env!("ACLAUDE_COMMIT");
const BUILD_TIME: &str = env!("ACLAUDE_BUILD_TIME");
const CHANNEL: &str = env!("ACLAUDE_CHANNEL");

#[derive(Parser)]
#[command(
    name = "aclaude",
    version = VERSION,
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

    /// Use NDJSON streaming protocol (agent/programmatic mode)
    #[arg(long)]
    streaming: bool,

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

    /// Check for and install updates
    Update,

    /// Show version details
    Version,

    /// List installed versions
    Versions {
        /// Clean old versions, keeping N most recent
        #[arg(long)]
        clean: Option<usize>,
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
    if cli.model.is_some() || cli.theme.is_some() || cli.role.is_some() || cli.immersion.is_some() {
        if let Some(model) = &cli.model {
            let mut session = toml::Table::new();
            session.insert("model".to_string(), toml::Value::String(model.clone()));
            overrides.insert("session".to_string(), toml::Value::Table(session));
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
                let result = session::run_prompt(&cfg, prompt, &cli.claude_args)?;
                print!("{result}");
            } else if cli.streaming {
                // NDJSON streaming protocol (agent/programmatic mode)
                let usage = session::start_streaming_session(&cfg, &cli.claude_args)?;
                usage.print_summary();
            } else {
                // Default: interactive TUI session
                session::start_session(&cfg, &cli.claude_args)?;
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
                let theme = persona::load_theme(&name)?;
                let agent_data = persona::get_agent(&theme, &agent)?;
                let portraits = portrait::resolve_portrait(&name, agent_data, Some(&agent));

                // Portrait before card (position: top)
                if show_portrait && portrait_position == "top" {
                    if let Some(path) = portraits.best_for_size(&portrait_size) {
                        if !portrait::display_portrait(path, &portrait_align) {
                            println!(
                                "(terminal does not support inline images — try Kitty or Ghostty)"
                            );
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
                        if !portrait::display_portrait(path, &portrait_align) {
                            println!(
                                "(terminal does not support inline images — try Kitty or Ghostty)"
                            );
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

        Some(Commands::Update) => {
            let channel = updater::Channel::parse(CHANNEL);
            println!(
                "Checking for updates ({})...",
                if channel == updater::Channel::Stable {
                    "stable"
                } else {
                    "alpha"
                }
            );
            match updater::check_for_update(channel)? {
                Some(tag) => println!("Latest: {tag} (current: {VERSION}-{COMMIT})"),
                None => println!("No updates available."),
            }
        }

        Some(Commands::Version) => {
            println!("aclaude {VERSION}");
            println!("  commit:  {COMMIT}");
            println!("  built:   {BUILD_TIME}");
            println!("  channel: {CHANNEL}");
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
