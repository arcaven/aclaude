use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use tmux_cmc::{Client, ConnectOptions, NewSessionOptions, NewWindowOptions};

use crate::config::ForestageConfig;
use crate::petname;

/// Name of the shared control session (one per socket).
const CTRL_SESSION: &str = "_ctrl";

/// Prefix used to identify control sessions in listings.
const CTRL_PREFIX: &str = "_ctrl";

/// Return the binary name the user invoked (e.g. "forestage-a" or "forestage").
fn binary_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "forestage".to_string())
}

/// Resolve the socket name from CLI arg or config.
fn socket_name(config: &ForestageConfig, socket: Option<&str>) -> String {
    socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone())
}

/// Path to the tmux socket file for a given socket name.
/// tmux uses /tmp/tmux-<uid>/<socket> on macOS/Linux.
fn socket_path(socket: &str) -> PathBuf {
    // TMUX_TMPDIR overrides the default location
    let base = std::env::var("TMUX_TMPDIR").unwrap_or_else(|_| {
        let uid = std::process::id(); // PID, not UID — need nix or Command
        // Shell out for UID since we forbid unsafe
        let uid_str = Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .unwrap_or_else(|| uid.to_string());
        format!("/tmp/tmux-{uid_str}")
    });
    PathBuf::from(base).join(socket)
}

/// Clean up a stale tmux socket if the server is confirmed dead.
///
/// Only removes the socket file when: the file exists AND the server
/// reports "server exited unexpectedly" (tmux's specific error for a
/// dead server behind a live socket file). Does NOT remove on other
/// failures (no sessions, server busy, etc.) to avoid killing live servers.
fn cleanup_stale_socket(socket: &str) {
    let path = socket_path(socket);
    if !path.exists() {
        return;
    }

    // Ping the server — any successful command means it's alive.
    let output = Command::new("tmux")
        .args(["-L", socket, "list-sessions"])
        .output();

    match output {
        Ok(o) if o.status.success() => {} // server alive, nothing to do
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Only remove if tmux specifically says the server is dead.
            // "server exited unexpectedly" = dead server behind stale socket.
            // "no server running" = socket exists but server gone.
            // Any other error = leave it alone.
            if stderr.contains("server exited unexpectedly") || stderr.contains("no server running")
            {
                eprintln!("Removing stale tmux socket: {}", path.display());
                let _ = std::fs::remove_file(&path);
            }
        }
        Err(_) => {} // couldn't even run tmux — don't touch the socket
    }
}

/// Resolve the session name: use the provided name, or the config default.
fn resolve_session_name(name: Option<&str>, config: &ForestageConfig) -> String {
    name.map(str::to_owned)
        .unwrap_or_else(|| config.tmux.default_name.clone())
}

/// Generate a fresh petname session name (for --new).
fn fresh_session_name() -> String {
    format!("forestage-{}", petname::generate())
}

/// Check how many user sessions exist on the socket.
fn user_session_count(socket: &str) -> usize {
    list_user_sessions(socket).map(|s| s.len()).unwrap_or(0)
}

/// Check if the shared control session exists on the socket.
fn ctrl_session_exists(socket: &str) -> bool {
    Command::new("tmux")
        .args(["-L", socket, "has-session", "-t", CTRL_SESSION])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Connect to tmux via control mode.
///
/// tmux-cmc uses `new-session -A -D` which detaches other clients from the
/// target session. This is safe for initial setup (Pattern 1: the session was
/// just created, no one else is attached) but destructive when modifying an
/// existing session that a user is viewing.
///
/// - **Pattern 1** (first session, no one attached): connect directly to the
///   user's session. Safe because we just created it.
/// - **Pattern 2** (anything else): use the shared `_ctrl` session running
///   `cat`. Never disrupts attached clients.
fn smart_connect(socket: &str, session_name: &str, is_new_session: bool) -> Result<Client> {
    let existing_count = user_session_count(socket);

    // Pattern 1 is only safe when we just created the session and no one
    // else could be attached. That means: this IS the new session being
    // created AND there are no other sessions on this socket.
    let safe_for_pattern1 = is_new_session && existing_count == 0;

    // ConnectOptions is non_exhaustive in tmux-cmc, so functional update
    // syntax is forbidden from outside the crate. Build from default and
    // mutate the fields we care about.
    let mut opts = ConnectOptions::default();
    opts.socket_name = Some(socket.to_owned());
    if safe_for_pattern1 {
        // Pattern 1: attach directly to the user's session (just created)
        opts.control_session_name = Some(session_name.to_owned());
        opts.control_session_command = None;
    } else {
        // Pattern 2: shared control session — never disrupts attached clients
        opts.control_session_name = Some(CTRL_SESSION.to_owned());
        opts.control_session_command = Some("cat".into());
    }

    Client::connect(&opts).context("failed to connect to tmux — is tmux installed?")
}

/// Start (or add a window to) a forestage tmux session.
///
/// - If the target session doesn't exist: create it with the first window.
/// - If the target session exists: add a new window to it (full-screen tab,
///   visible in the tmux status bar, switchable with Ctrl-b <number>).
/// - `--new` forces creation of a fresh session with a petname.
/// - `--persona` and `--role` are passed through to the forestage binary
///   launched in the new window.
pub fn run_session_start(
    config: &ForestageConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
    attach: bool,
    force_new: bool,
    persona: Option<&str>,
    role: Option<&str>,
) -> Result<()> {
    let session_name = if force_new {
        fresh_session_name()
    } else {
        resolve_session_name(session_name, config)
    };
    let socket = socket_name(config, socket);

    // Clean up stale socket if the server crashed previously
    cleanup_stale_socket(&socket);

    // Check if this session already exists
    let already_exists = Command::new("tmux")
        .args(["-L", &socket, "has-session", "-t", &session_name])
        .output()
        .is_ok_and(|o| o.status.success());

    if already_exists {
        // Session exists — add a new window (full-screen tab)
        println!("Adding window to session '{session_name}'...");

        let client = smart_connect(&socket, &session_name, false)?;

        // Query session ID ($n) for the existing session
        let session_id = query_session_id(&client, &session_name)?;

        // Create a new window in this session, launching forestage directly as
        // the pane's initial process (no shell → no shell-echo of the command).
        // Image env vars aren't needed here: new windows inherit the session's
        // globals set by configure_session when the session was first created.
        let window_name = window_label(persona, role);
        let window_cmd = forestage_command(persona, role, None);
        // NewWindowOptions is non_exhaustive — mutate after default.
        let mut window_opts = NewWindowOptions::default();
        window_opts.session = session_id;
        window_opts.name = Some(window_name);
        window_opts.detached = true;
        window_opts.start_command = Some(window_cmd);
        let _win_id = client
            .new_window(&window_opts)
            .context("new-window failed")?;

        // Don't select-window here — it would disrupt anyone already
        // attached to this session (clears Kitty graphics, forces redraw).
        // Instead, the attach call below targets the new window directly.
        drop(client);

        println!("Window added to session '{session_name}'.");

        // Attach targets the new window specifically, not the session default
        if attach {
            return exec_attach(&socket, &format!("{session_name}:$"));
        }
        return Ok(());
    } else {
        // Session doesn't exist — create it
        let existing_count = user_session_count(&socket);

        // Detect the terminal's image protocol BEFORE creating the session
        // so the initial pane can be launched with forestage as its first
        // process (skipping a shell, and its echo of the command line).
        let image_env = detect_image_protocol();
        let start_command = forestage_command(persona, role, image_env.as_ref());

        if existing_count == 0 {
            // First session: create it, then attach control mode to it (Pattern 1)
            println!("Creating session '{session_name}'...");

            let status = Command::new("tmux")
                .args([
                    "-L",
                    &socket,
                    "new-session",
                    "-d",
                    "-s",
                    &session_name,
                    "-x",
                    "200",
                    "-y",
                    "50",
                    &start_command,
                ])
                .status()
                .context("failed to create tmux session")?;
            if !status.success() {
                anyhow::bail!("tmux new-session failed");
            }

            let client = smart_connect(&socket, &session_name, false)?;
            configure_session(&client, config, &session_name, &socket, image_env.as_ref())?;
            drop(client);
        } else {
            // Additional session: use shared _ctrl (Pattern 2)
            println!("Creating session '{session_name}'...");

            let client = smart_connect(&socket, &session_name, true)?;
            // NewSessionOptions is non_exhaustive — mutate after default
            let mut new_session_opts = NewSessionOptions::default();
            new_session_opts.name = Some(session_name.clone());
            new_session_opts.detached = true;
            new_session_opts.start_command = Some(start_command);
            client
                .new_session(&new_session_opts)
                .context("new-session failed")?;
            configure_session(&client, config, &session_name, &socket, image_env.as_ref())?;
            drop(client);
        }

        println!(
            "Session ready. Attach with: {} session attach -t {session_name}",
            binary_name()
        );
    }

    if attach {
        exec_attach(&socket, &session_name)?;
    }

    Ok(())
}

/// Configure statusline and supporting tmux options for a session.
///
/// `image_env`, if provided, is persisted in tmux's global environment so
/// future panes in this session (new windows, splits) inherit the protocol.
/// Detection itself happens before session creation — see
/// [`detect_image_protocol`].
fn configure_session(
    client: &Client,
    config: &ForestageConfig,
    session_name: &str,
    socket: &str,
    image_env: Option<&ImageEnv>,
) -> Result<()> {
    let session = query_session_id(client, session_name)?;

    client
        .set_status_enabled(&session, true)
        .context("set-option status failed")?;
    client
        .set_status_interval(&session, config.tmux.status_interval)
        .context("set-option status-interval failed")?;
    client
        .set_status_left(&session, &format!(" forestage | {session_name} "))
        .context("set-option status-left failed")?;
    client
        .set_status_right(&session, "")
        .context("set-option status-right failed")?;

    // Enable Kitty graphics passthrough for portrait display (tmux 3.3+).
    // Global scope on the dedicated forestage socket — doesn't affect other tmux sessions.
    // Ignore errors: older tmux versions don't have this option.
    let _ = client.set_global_option("allow-passthrough", "on");

    // Forward terminal-level focus events to panes.
    let _ = client.set_global_option("focus-events", "on");

    // tmux window switches don't generate FocusGained events even with
    // focus-events on. Use after-select-window hook to send the FocusIn
    // escape sequence (\x1b[I) as hex bytes — unambiguous, no quoting issues.
    let _ = Command::new("tmux")
        .args([
            "-L",
            socket,
            "set-hook",
            "-g",
            "after-select-window",
            "send-keys -H 1b 5b 49",
        ])
        .output();

    if let Some(env) = image_env {
        persist_image_protocol_in_tmux(socket, env);
    }

    Ok(())
}

/// Detected image protocol and font size for env injection.
struct ImageEnv {
    protocol: String,
    font_size: String,
}

/// Query the current terminal for its image protocol and font size.
///
/// Must run OUTSIDE any tmux pane — the picker queries stdin directly and
/// races with tmux's own passthrough if the current TTY is a pane. Returns
/// `None` if the terminal doesn't support any image protocol (picker will
/// have fallen back to halfblocks, which is included as a successful
/// result).
fn detect_image_protocol() -> Option<ImageEnv> {
    use ratatui_image::picker::Picker;

    let raw_ok = crossterm::terminal::enable_raw_mode().is_ok();

    let result = Picker::from_query_stdio().ok().map(|picker| {
        let protocol = match picker.protocol_type() {
            ratatui_image::picker::ProtocolType::Kitty => "kitty",
            ratatui_image::picker::ProtocolType::Iterm2 => "iterm2",
            ratatui_image::picker::ProtocolType::Sixel => "sixel",
            ratatui_image::picker::ProtocolType::Halfblocks => "halfblocks",
        }
        .to_owned();
        let font = picker.font_size();
        ImageEnv {
            protocol,
            font_size: format!("{}x{}", font.0, font.1),
        }
    });

    if raw_ok {
        let _ = crossterm::terminal::disable_raw_mode();
    }

    result
}

/// Store the detected image protocol in tmux's global environment so that
/// panes created after session setup inherit it automatically. The session's
/// initial pane is handled separately (env prefix on its start_command)
/// because it spawns before this call.
fn persist_image_protocol_in_tmux(socket: &str, env: &ImageEnv) {
    let _ = Command::new("tmux")
        .args([
            "-L",
            socket,
            "set-environment",
            "-g",
            "FORESTAGE_IMAGE_PROTOCOL",
            &env.protocol,
        ])
        .output();
    let _ = Command::new("tmux")
        .args([
            "-L",
            socket,
            "set-environment",
            "-g",
            "FORESTAGE_IMAGE_FONT_SIZE",
            &env.font_size,
        ])
        .output();
}

/// Build the forestage command string with optional persona/role overrides.
///
/// When `image_env` is provided, prepends environment variable assignments
/// so the shell sets them before exec. This is needed for the first pane
/// of a new session — its shell was spawned before `set-environment -g`
/// stored the protocol in tmux's global env.
fn forestage_command(
    persona: Option<&str>,
    role: Option<&str>,
    image_env: Option<&ImageEnv>,
) -> String {
    let forestage_bin =
        std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("forestage"));
    let mut cmd = String::new();
    if let Some(env) = image_env {
        cmd.push_str(&format!(
            "FORESTAGE_IMAGE_PROTOCOL={} FORESTAGE_IMAGE_FONT_SIZE={} ",
            env.protocol, env.font_size
        ));
    }
    cmd.push_str(&format!("exec {}", forestage_bin.to_string_lossy()));
    if let Some(p) = persona {
        cmd.push_str(&format!(" --theme {p}"));
    }
    if let Some(r) = role {
        cmd.push_str(&format!(" --role {r}"));
    }
    cmd
}

/// Generate a window name label from persona/role, or a default.
fn window_label(persona: Option<&str>, role: Option<&str>) -> String {
    match (persona, role) {
        (Some(p), Some(r)) => format!("{p}:{r}"),
        (Some(p), None) => p.to_owned(),
        (None, Some(r)) => r.to_owned(),
        (None, None) => "forestage".to_owned(),
    }
}

/// Query the session ID ($n) for a named session.
fn query_session_id(client: &Client, session_name: &str) -> Result<tmux_cmc::SessionId> {
    let resp = client
        .run_command(&format!(
            "display-message -p -t '{session_name}' '#{{session_id}}'"
        ))
        .context("failed to query session id")?;
    let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
    tmux_cmc::SessionId::new(&id_str)
        .map_err(|_| anyhow::anyhow!("invalid session id from tmux: {id_str}"))
}

/// Exec tmux attach (replaces current process on success).
fn exec_attach(socket: &str, session_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["-L", socket, "attach-session", "-t", session_name])
        .status()
        .context("failed to exec tmux attach")?;

    if !status.success() {
        anyhow::bail!("tmux attach-session exited with {status}");
    }
    Ok(())
}

/// Attach to an existing forestage session.
pub fn run_session_attach(
    config: &ForestageConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
) -> Result<()> {
    let socket = socket_name(config, socket);

    let target = match session_name {
        Some(name) => name.to_owned(),
        None => {
            let sessions = list_user_sessions(&socket)?;
            match sessions.len() {
                0 => anyhow::bail!("no forestage sessions found on socket '{socket}'"),
                1 => sessions[0].clone(),
                _ => {
                    println!("Multiple sessions found:");
                    for s in &sessions {
                        println!("  {s}");
                    }
                    anyhow::bail!(
                        "specify a session with -t, e.g.: {} session attach -t {}",
                        binary_name(),
                        sessions[0]
                    );
                }
            }
        }
    };

    exec_attach(&socket, &target)
}

/// Stop (kill) an forestage session, or all sessions.
pub fn run_session_stop(
    config: &ForestageConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
    all: bool,
) -> Result<()> {
    let socket = socket_name(config, socket);

    if all {
        let status = Command::new("tmux")
            .args(["-L", &socket, "kill-server"])
            .status()
            .context("failed to kill tmux server")?;

        if status.success() {
            println!("All sessions on socket '{socket}' stopped.");
        } else {
            println!("No tmux server running on socket '{socket}'.");
        }
        return Ok(());
    }

    let target = match session_name {
        Some(name) => name.to_owned(),
        None => {
            let sessions = list_user_sessions(&socket)?;
            match sessions.len() {
                0 => {
                    println!("No forestage sessions found.");
                    return Ok(());
                }
                1 => sessions[0].clone(),
                _ => {
                    println!("Multiple sessions found:");
                    for s in &sessions {
                        println!("  {s}");
                    }
                    anyhow::bail!("specify a session with -t, or use --all to stop everything");
                }
            }
        }
    };

    // Kill the user session via tmux CLI
    let _ = Command::new("tmux")
        .args(["-L", &socket, "kill-session", "-t", &target])
        .status();

    // If this was the last user session, clean up _ctrl too
    let remaining = user_session_count(&socket);
    if remaining == 0 && ctrl_session_exists(&socket) {
        let _ = Command::new("tmux")
            .args(["-L", &socket, "kill-session", "-t", CTRL_SESSION])
            .status();
    }

    println!("Session '{target}' stopped.");
    Ok(())
}

/// List sessions. Excludes control sessions unless `show_all` is true.
pub fn run_session_list(
    config: &ForestageConfig,
    socket: Option<&str>,
    names_only: bool,
    show_all: bool,
) -> Result<()> {
    let socket = socket_name(config, socket);

    let output = Command::new("tmux")
        .args(["-L", &socket, "list-sessions"])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        println!("No sessions on socket '{socket}'.");
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let name = line.split(':').next().unwrap_or("");
        if !show_all && name.starts_with(CTRL_PREFIX) {
            continue;
        }
        if names_only {
            println!("{name}");
        } else {
            println!("{line}");
        }
    }

    Ok(())
}

/// Show status of sessions. Excludes control sessions unless `show_all` is true.
pub fn run_session_status(
    config: &ForestageConfig,
    socket: Option<&str>,
    show_all: bool,
) -> Result<()> {
    let socket = socket_name(config, socket);

    let output = Command::new("tmux")
        .args([
            "-L",
            &socket,
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_windows}\t#{session_created}\t#{?session_attached,attached,detached}",
        ])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        println!("No sessions on socket '{socket}'.");
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut visible_sessions = Vec::new();
    let mut ctrl_count = 0;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        let name = parts[0];
        if name.starts_with(CTRL_PREFIX) {
            ctrl_count += 1;
            if !show_all {
                continue;
            }
        }
        visible_sessions.push(parts);
    }

    if visible_sessions.is_empty() {
        println!("No forestage sessions.");
        return Ok(());
    }

    println!(
        "{:<30} {:>7} {:<24} {:<10}",
        "SESSION", "WINDOWS", "CREATED", "STATE"
    );
    for parts in &visible_sessions {
        let created = parts
            .get(2)
            .and_then(|s| s.parse::<i64>().ok())
            .map(|epoch| {
                chrono::DateTime::from_timestamp(epoch, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        println!(
            "{:<30} {:>7} {:<24} {:<10}",
            parts.first().unwrap_or(&""),
            parts.get(1).unwrap_or(&""),
            created,
            parts.get(3).unwrap_or(&""),
        );
    }

    if !show_all && ctrl_count > 0 {
        println!("\n({ctrl_count} control session(s) hidden, use --all to show)");
    }

    Ok(())
}

/// List user session names (excluding control sessions) via tmux CLI.
fn list_user_sessions(socket: &str) -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["-L", socket, "list-sessions", "-F", "#{session_name}"])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|name| !name.starts_with(CTRL_PREFIX))
        .map(str::to_owned)
        .collect())
}
