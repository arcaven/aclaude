use std::process::Command;

use anyhow::{Context, Result};
use tmux_cmc::{Client, ConnectOptions, NewSessionOptions};

use crate::config::AclaudeConfig;
use crate::petname;

/// Prefix for control sessions — filtered from user-facing listings.
const CTRL_PREFIX: &str = "_ctrl-";

/// Build ConnectOptions for a given session name and socket.
fn connect_opts(cfg: &AclaudeConfig, socket: Option<&str>, session_name: &str) -> ConnectOptions {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| cfg.tmux.socket.clone());

    ConnectOptions {
        socket_name: Some(socket_name),
        control_session_name: Some(format!("{CTRL_PREFIX}{session_name}")),
        control_session_command: Some("cat".into()),
        ..ConnectOptions::default()
    }
}

/// Resolve the session name: use the provided name, or generate a petname.
fn resolve_session_name(name: Option<&str>) -> String {
    name.map(str::to_owned)
        .unwrap_or_else(|| format!("aclaude-{}", petname::generate()))
}

/// Start (or attach to) an aclaude tmux session.
///
/// Creates the session if it doesn't exist, configures the statusline via
/// control mode, launches the aclaude binary in the main pane, and optionally
/// attaches the current terminal.
pub fn run_session_start(
    config: &AclaudeConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
    attach: bool,
) -> Result<()> {
    let session_name = resolve_session_name(session_name);
    let opts = connect_opts(config, socket, &session_name);
    let socket_name = opts.socket_name.clone().unwrap_or_default();

    let client =
        Client::connect(&opts).context("failed to connect to tmux — is tmux installed?")?;

    let session = if client
        .has_session(&session_name)
        .context("has-session failed")?
    {
        println!("Session '{session_name}' already exists.");
        let resp = client
            .run_command(&format!(
                "display-message -p -t {session_name} '#{{session_id}}'"
            ))
            .context("failed to query session id")?;
        let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
        tmux_cmc::SessionId::new(&id_str)
            .unwrap_or_else(|_| tmux_cmc::SessionId::new("$0").expect("$0 is valid"))
    } else {
        println!("Creating session '{session_name}'...");
        client
            .new_session(&NewSessionOptions {
                name: Some(session_name.clone()),
                detached: true,
                ..Default::default()
            })
            .context("new-session failed")?
    };

    // Configure statusline via control mode
    client
        .set_status_enabled(&session, true)
        .context("set-option status failed")?;
    client
        .set_status_interval(&session, config.tmux.status_interval)
        .context("set-option status-interval failed")?;
    client
        .set_status_left(&session, &format!(" aclaude | {session_name} "))
        .context("set-option status-left failed")?;
    client
        .set_status_right(&session, "")
        .context("set-option status-right failed")?;

    // Launch aclaude in the main pane
    let aclaude_bin =
        std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("aclaude"));
    let aclaude_path = aclaude_bin.to_string_lossy();

    let pane_resp = client
        .run_command(&format!(
            "display-message -p -t {session_name} '#{{pane_id}}'"
        ))
        .context("failed to query pane id")?;
    let pane_id_str = pane_resp.first_line().unwrap_or("%0").trim().to_owned();
    let pane = tmux_cmc::PaneId::new(&pane_id_str)
        .unwrap_or_else(|_| tmux_cmc::PaneId::new("%0").expect("%0 is valid"));

    client
        .send_keys(&pane, &aclaude_path, false)
        .context("send-keys failed")?;

    println!("Session ready. Attach with: aclaude session attach -t {session_name}");

    // Drop the control mode connection before attaching
    drop(client);

    if attach {
        let status = Command::new("tmux")
            .args(["-L", &socket_name, "attach-session", "-t", &session_name])
            .status()
            .context("failed to exec tmux attach")?;

        if !status.success() {
            anyhow::bail!("tmux attach-session exited with {status}");
        }
    }

    Ok(())
}

/// Attach to an existing aclaude session.
pub fn run_session_attach(
    config: &AclaudeConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    // If no session name given, find the first non-control session
    let target = match session_name {
        Some(name) => name.to_owned(),
        None => {
            let sessions = list_user_sessions(&socket_name)?;
            match sessions.len() {
                0 => anyhow::bail!("no aclaude sessions found on socket '{socket_name}'"),
                1 => sessions[0].clone(),
                _ => {
                    println!("Multiple sessions found:");
                    for s in &sessions {
                        println!("  {s}");
                    }
                    anyhow::bail!(
                        "specify a session with -t, e.g.: aclaude session attach -t {}",
                        sessions[0]
                    );
                }
            }
        }
    };

    let status = Command::new("tmux")
        .args(["-L", &socket_name, "attach-session", "-t", &target])
        .status()
        .context("failed to exec tmux attach")?;

    if !status.success() {
        anyhow::bail!("no session named '{target}' on socket '{socket_name}'");
    }

    Ok(())
}

/// Stop (kill) an aclaude session, or all sessions.
pub fn run_session_stop(
    config: &AclaudeConfig,
    socket: Option<&str>,
    session_name: Option<&str>,
    all: bool,
) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    if all {
        let status = Command::new("tmux")
            .args(["-L", &socket_name, "kill-server"])
            .status()
            .context("failed to kill tmux server")?;

        if status.success() {
            println!("All sessions on socket '{socket_name}' stopped.");
        } else {
            println!("No tmux server running on socket '{socket_name}'.");
        }
        return Ok(());
    }

    // If no session name given, find the only session or error
    let target = match session_name {
        Some(name) => name.to_owned(),
        None => {
            let sessions = list_user_sessions(&socket_name)?;
            match sessions.len() {
                0 => {
                    println!("No aclaude sessions found.");
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

    // Kill the user session and its control session
    let ctrl_name = format!("{CTRL_PREFIX}{target}");

    let opts = connect_opts(config, socket, &target);
    if let Ok(client) = Client::connect(&opts) {
        // Query session ID and kill it
        if let Ok(resp) =
            client.run_command(&format!("display-message -p -t {target} '#{{session_id}}'"))
        {
            let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
            if let Ok(session) = tmux_cmc::SessionId::new(&id_str) {
                let _ = client.kill_session(&session);
            }
        }
        // Kill the control session too
        if let Ok(resp) = client.run_command(&format!(
            "display-message -p -t {ctrl_name} '#{{session_id}}'"
        )) {
            let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
            if let Ok(session) = tmux_cmc::SessionId::new(&id_str) {
                let _ = client.kill_session(&session);
            }
        }
    } else {
        // Fallback: use tmux CLI directly
        let _ = Command::new("tmux")
            .args(["-L", &socket_name, "kill-session", "-t", &target])
            .status();
        let _ = Command::new("tmux")
            .args(["-L", &socket_name, "kill-session", "-t", &ctrl_name])
            .status();
    }

    println!("Session '{target}' stopped.");
    Ok(())
}

/// List user sessions (excluding control sessions).
pub fn run_session_list(
    config: &AclaudeConfig,
    socket: Option<&str>,
    names_only: bool,
) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    let output = Command::new("tmux")
        .args(["-L", &socket_name, "list-sessions"])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        println!("No sessions on socket '{socket_name}'.");
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Filter out control sessions
        let session_name = line.split(':').next().unwrap_or("");
        if session_name.starts_with(CTRL_PREFIX) {
            continue;
        }
        if names_only {
            println!("{session_name}");
        } else {
            println!("{line}");
        }
    }

    Ok(())
}

/// Show status of sessions.
pub fn run_session_status(config: &AclaudeConfig, socket: Option<&str>) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    let output = Command::new("tmux")
        .args([
            "-L",
            &socket_name,
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_windows}\t#{session_created_string}\t#{?session_attached,attached,detached}",
        ])
        .output()
        .context("failed to run tmux list-sessions")?;

    if !output.status.success() {
        println!("No sessions on socket '{socket_name}'.");
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut user_sessions = Vec::new();
    let mut ctrl_count = 0;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        let name = parts[0];
        if name.starts_with(CTRL_PREFIX) {
            ctrl_count += 1;
            continue;
        }
        user_sessions.push(parts);
    }

    if user_sessions.is_empty() {
        println!("No aclaude sessions.");
        return Ok(());
    }

    println!(
        "{:<30} {:>7} {:<24} {:<10}",
        "SESSION", "WINDOWS", "CREATED", "STATE"
    );
    for parts in &user_sessions {
        println!(
            "{:<30} {:>7} {:<24} {:<10}",
            parts.first().unwrap_or(&""),
            parts.get(1).unwrap_or(&""),
            parts.get(2).unwrap_or(&""),
            parts.get(3).unwrap_or(&""),
        );
    }

    if ctrl_count > 0 {
        println!("\n({ctrl_count} control session(s) hidden)");
    }

    Ok(())
}

/// List user session names (excluding control sessions) via tmux CLI.
fn list_user_sessions(socket_name: &str) -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["-L", socket_name, "list-sessions", "-F", "#{session_name}"])
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
