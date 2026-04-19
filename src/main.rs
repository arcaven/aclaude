#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use forestage::config;
use forestage::download;
use forestage::persona;
use forestage::portrait;
use forestage::resolve;
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

    /// Theme override (the roster)
    #[arg(short = 't', long)]
    theme: Option<String>,

    /// Persona: character slug from the theme roster (e.g. "naomi-nagata")
    #[arg(long)]
    persona: Option<String>,

    /// Role override: job assignment(s), comma-separated (e.g. "reviewer,troubleshooter")
    #[arg(short = 'r', long)]
    role: Option<String>,

    /// Identity: professional lens (e.g. "homicide detective", "systems architect")
    #[arg(long)]
    identity: Option<String>,

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

    // -- Marvel integration flags --
    // Set by marvel's forestage adapter when launching agents.
    /// Agent session name (e.g. "squad-worker-g1-0")
    #[arg(long)]
    name: Option<String>,

    /// Marvel workspace name
    #[arg(long)]
    workspace: Option<String>,

    /// Marvel team name
    #[arg(long)]
    team: Option<String>,

    /// Marvel daemon socket path
    #[arg(long)]
    socket: Option<String>,

    /// Claude Code permission mode (passed through to claude subprocess)
    #[arg(long)]
    permission_mode: Option<String>,

    /// Bypass ALL tool-permission prompts — maps to Claude Code's
    /// --dangerously-skip-permissions. Intended for autonomous agents
    /// (marvel teams, multiclaude fleets) where no interactive approver
    /// exists. Do NOT enable for interactive sessions you don't fully
    /// trust.
    #[arg(long, alias = "yolo")]
    dangerously_skip_permissions: bool,

    /// Lua script path (future: native lua support)
    #[arg(long)]
    script: Option<String>,

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

    /// Manage portrait images
    Portraits {
        #[command(subcommand)]
        action: PortraitAction,
    },
}

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
    /// List available themes from remote manifest
    ListRemote,
    /// Remove cached portraits for a theme
    Clean {
        /// Theme to clean (omit for all)
        theme: Option<String>,
    },
}

#[derive(Subcommand)]
enum SessionAction {
    /// Start an forestage tmux session (adds a pane if session exists)
    Start {
        /// Session name (default: config tmux.default_name)
        #[arg(short = 't', long = "session-name")]
        name: Option<String>,
        /// tmux socket name override
        #[arg(long)]
        socket: Option<String>,
        /// Attach terminal to the session after starting
        #[arg(long)]
        attach: bool,
        /// Force create a new session (petname) instead of joining existing
        #[arg(long)]
        new: bool,
        /// Persona (theme/character) override for this pane
        #[arg(long)]
        persona: Option<String>,
        /// Role override for this pane
        #[arg(long)]
        role: Option<String>,
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
    /// List themes, or characters within a theme
    List {
        /// Theme slug (or fuzzy fragment) — list this theme's characters.
        /// Omit to list all themes.
        theme: Option<String>,
    },

    /// Show theme details (or a single character card with --agent)
    Show {
        /// Theme slug
        name: String,

        /// Character slug — show card for this character. Omit to list
        /// the full roster.
        #[arg(long)]
        agent: Option<String>,

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

/// Trim a string to a single line and cap its length for table-style output.
fn truncate_one_line(s: &str, max: usize) -> String {
    let one_line = s.lines().next().unwrap_or("").trim();
    if one_line.chars().count() <= max {
        return one_line.to_string();
    }
    let mut out: String = one_line.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Fuzzy-resolve --theme and --persona before they reach the config
    // merge. Two-phase: theme narrows persona; persona back-propagates
    // theme when theme can't be resolved. Warnings on stderr.
    let (resolved_theme, resolved_persona) =
        resolve::resolve_theme_and_persona(cli.theme.as_deref(), cli.persona.as_deref())?;

    // Build CLI overrides table
    let mut overrides = toml::Table::new();
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
        if let Some(theme) = &resolved_theme {
            persona_overrides.insert("theme".to_string(), toml::Value::String(theme.clone()));
        }
        if let Some(persona) = &resolved_persona {
            persona_overrides.insert(
                "character".to_string(),
                toml::Value::String(persona.clone()),
            );
        }
        if let Some(role) = &cli.role {
            persona_overrides.insert("role".to_string(), toml::Value::String(role.clone()));
        }
        if let Some(identity) = &cli.identity {
            persona_overrides.insert(
                "identity".to_string(),
                toml::Value::String(identity.clone()),
            );
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

    // Marvel integration overrides
    {
        let mut marvel_overrides = toml::Table::new();
        if let Some(name) = &cli.name {
            marvel_overrides.insert("name".to_string(), toml::Value::String(name.clone()));
        }
        if let Some(workspace) = &cli.workspace {
            marvel_overrides.insert(
                "workspace".to_string(),
                toml::Value::String(workspace.clone()),
            );
        }
        if let Some(team) = &cli.team {
            marvel_overrides.insert("team".to_string(), toml::Value::String(team.clone()));
        }
        if let Some(socket) = &cli.socket {
            marvel_overrides.insert("socket".to_string(), toml::Value::String(socket.clone()));
        }
        if let Some(perm) = &cli.permission_mode {
            marvel_overrides.insert(
                "permission_mode".to_string(),
                toml::Value::String(perm.clone()),
            );
        }
        if cli.dangerously_skip_permissions {
            marvel_overrides.insert(
                "dangerously_skip_permissions".to_string(),
                toml::Value::Boolean(true),
            );
        }
        if let Some(script) = &cli.script {
            marvel_overrides.insert("script".to_string(), toml::Value::String(script.clone()));
        }
        if !marvel_overrides.is_empty() {
            overrides.insert("marvel".to_string(), toml::Value::Table(marvel_overrides));
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
                // Auto-download portraits before interactive session
                if let Err(e) = download::ensure_portraits(&cfg.persona.theme, &cfg.portrait) {
                    eprintln!("portrait download: {e}");
                }

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
            PersonaAction::List { theme: theme_arg } => {
                if let Some(q) = theme_arg {
                    // List characters in a specific theme (fuzzy-resolve the theme slug).
                    let theme_slug = match resolve::match_theme(&q).picked() {
                        Some(s) => s,
                        None => {
                            eprintln!("forestage: theme '{q}' not found");
                            std::process::exit(2);
                        }
                    };
                    let theme = persona::load_theme(&theme_slug)?;
                    let mut chars: Vec<_> = theme.characters.iter().collect();
                    chars.sort_by_key(|(k, _)| k.as_str());
                    println!(
                        "{} ({}) — {} characters:",
                        theme.theme.name,
                        theme_slug,
                        chars.len()
                    );
                    for (slug, c) in chars {
                        println!(
                            "  {:<40} {} — {}",
                            slug,
                            c.character,
                            truncate_one_line(&c.style, 60)
                        );
                    }
                } else {
                    // List all themes.
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
                // Fuzzy-resolve the theme slug so 'persona show disc' works.
                let name = resolve::match_theme(&name).picked().ok_or_else(|| {
                    anyhow::anyhow!("theme '{name}' not found — try 'forestage persona list'")
                })?;
                let theme = persona::load_theme(&name)?;

                let Some(agent_slug) = agent else {
                    // No character selected — print roster summary.
                    println!("Theme: {} ({})", theme.theme.name, theme.category);
                    println!("Description: {}", theme.theme.description);
                    if !theme.theme.source.is_empty() {
                        println!("Source: {}", theme.theme.source);
                    }
                    if let Some(title) = &theme.theme.user_title {
                        println!("User title: {title}");
                    }
                    let mut chars: Vec<_> = theme.characters.iter().collect();
                    chars.sort_by_key(|(k, _)| k.as_str());
                    println!();
                    println!("Characters ({}):", chars.len());
                    for (slug, c) in chars {
                        println!(
                            "  {:<40} {} — {}",
                            slug,
                            c.character,
                            truncate_one_line(&c.style, 60)
                        );
                    }
                    println!();
                    println!("Use 'forestage persona show {name} --agent <slug>' for details.");
                    return Ok(());
                };

                // Auto-download portraits if showing and not cached
                if show_portrait {
                    let _ = download::ensure_portraits(&name, &cfg.portrait);
                }

                // Fuzzy-resolve the character slug within the theme roster.
                let agent_slug = resolve::match_character_in_theme(&agent_slug, &theme)
                    .picked()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "character '{agent_slug}' not found in theme '{name}' — try 'forestage persona list {name}'"
                        )
                    })?;
                let character_data = persona::get_character(&theme, &agent_slug)?;
                let portraits = portrait::resolve_portrait(&name, character_data);

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
                println!();
                println!("Character: {} ({agent_slug})", character_data.character);
                println!("  Style: {}", character_data.style);
                println!("  Expertise: {}", character_data.expertise);
                println!("  Trait: {}", character_data.r#trait);
                if !character_data.quirks.is_empty() {
                    println!("  Quirks: {}", character_data.quirks.join("; "));
                }
                if !character_data.catchphrases.is_empty() {
                    println!("  Catchphrases:");
                    for phrase in &character_data.catchphrases {
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
                    attach,
                    new,
                    persona,
                    role,
                } => {
                    session_cmd::run_session_start(
                        &cfg,
                        socket.as_deref(),
                        name.as_deref(),
                        attach,
                        new,
                        persona.as_deref(),
                        role.as_deref(),
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

        Some(Commands::Portraits { action }) => {
            let cfg = config::load_config(cli_overrides)?;
            match action {
                PortraitAction::Download { theme, all } => {
                    if all {
                        let (downloaded, skipped) = download::download_all(&cfg.portrait)?;
                        println!("Downloaded {downloaded} theme(s), {skipped} already cached");
                    } else {
                        let theme_slug = theme.unwrap_or_else(|| cfg.persona.theme.clone());
                        match download::ensure_portraits(&theme_slug, &cfg.portrait)? {
                            true => println!("Portraits ready for {theme_slug}"),
                            false => println!("No portraits available for {theme_slug}"),
                        }
                    }
                }
                PortraitAction::Status => {
                    let cache_dir = portrait::portrait_cache_dir();
                    let (themes, images) = portrait::cache_status();
                    println!("Portrait cache: {}", cache_dir.display());
                    println!("Themes with portraits: {themes}");
                    println!("Total images: {images}");
                    println!(
                        "Auto-download: {}",
                        if cfg.portrait.auto_download {
                            "on"
                        } else {
                            "off"
                        }
                    );
                    println!("Display mode: {}", cfg.portrait.display);
                }
                PortraitAction::ListRemote => {
                    let themes = download::list_remote()?;
                    if themes.is_empty() {
                        println!("Could not fetch remote manifest");
                    } else {
                        println!("{} themes available:", themes.len());
                        for (name, count) in &themes {
                            let cached = portrait::portrait_cache_dir()
                                .join(name)
                                .join(".complete")
                                .exists();
                            let status = if cached { "cached" } else { "remote" };
                            println!("  {name:<30} {count:>2} personas  [{status}]");
                        }
                    }
                }
                PortraitAction::Clean { theme } => {
                    if let Some(slug) = theme {
                        if download::clean_theme(&slug)? {
                            println!("Cleaned portrait cache for {slug}");
                        } else {
                            println!("No cached portraits for {slug}");
                        }
                    } else {
                        let cache_dir = portrait::portrait_cache_dir();
                        if cache_dir.exists() {
                            let (themes, _) = portrait::cache_status();
                            std::fs::remove_dir_all(&cache_dir).map_err(|e| {
                                forestage::error::ForestageError::Session {
                                    message: format!("failed to clean portrait cache: {e}"),
                                }
                            })?;
                            println!("Cleaned all portrait cache ({themes} themes)");
                        } else {
                            println!("Portrait cache is empty");
                        }
                    }
                }
            }
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
