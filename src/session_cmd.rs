use std::process::Command;

use anyhow::{Context, Result};
use tmux_cmc::{Client, ConnectOptions, NewSessionOptions};

use crate::config::AclaudeConfig;

const SESSION_NAME: &str = "aclaude";

/// Start (or attach to) the aclaude tmux session.
///
/// Creates the session if it doesn't exist, configures the statusline via
/// control mode, launches the aclaude binary in the main pane, and optionally
/// attaches the current terminal.
pub fn run_session_start(config: &AclaudeConfig, socket: Option<&str>, attach: bool) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    let client = Client::connect(&ConnectOptions {
        socket_name: Some(socket_name.clone()),
        ..ConnectOptions::default()
    })
    .context("failed to connect to tmux — is tmux installed?")?;

    let session = if client
        .has_session(SESSION_NAME)
        .context("has-session failed")?
    {
        println!("Session '{SESSION_NAME}' already exists.");
        // Re-attach path: configure statusline on existing session.
        // We need the session id — query it.
        let resp = client
            .run_command(&format!(
                "display-message -p -t {SESSION_NAME} '#{{session_id}}'"
            ))
            .context("failed to query session id")?;
        let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
        tmux_cmc::SessionId::new(&id_str)
            .unwrap_or_else(|_| tmux_cmc::SessionId::new("$0").expect("$0 is valid"))
    } else {
        println!("Creating session '{SESSION_NAME}'...");
        client
            .new_session(&NewSessionOptions {
                name: Some(SESSION_NAME.into()),
                detached: true,
                ..Default::default()
            })
            .context("new-session failed")?
    };

    // Configure statusline via control mode (no shell polling needed)
    client
        .set_status_enabled(&session, true)
        .context("set-option status failed")?;
    client
        .set_status_interval(&session, config.tmux.status_interval)
        .context("set-option status-interval failed")?;
    client
        .set_status_left(&session, &format!(" aclaude | {SESSION_NAME} "))
        .context("set-option status-left failed")?;
    client
        .set_status_right(&session, "")
        .context("set-option status-right failed")?;

    // Launch aclaude in the main pane
    let aclaude_bin =
        std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("aclaude"));
    let aclaude_path = aclaude_bin.to_string_lossy();

    // Get the first (and only) pane in the new session
    let pane_resp = client
        .run_command(&format!(
            "display-message -p -t {SESSION_NAME} '#{{pane_id}}'"
        ))
        .context("failed to query pane id")?;
    let pane_id_str = pane_resp.first_line().unwrap_or("%0").trim().to_owned();
    let pane = tmux_cmc::PaneId::new(&pane_id_str)
        .unwrap_or_else(|_| tmux_cmc::PaneId::new("%0").expect("%0 is valid"));

    client
        .send_keys(&pane, &aclaude_path, false)
        .context("send-keys failed")?;

    println!("Session ready. Attach with: tmux -L {socket_name} attach-session -t {SESSION_NAME}");

    // Drop the control mode connection before attaching (we're done with it)
    drop(client);

    if attach {
        let status = Command::new("tmux")
            .args(["-L", &socket_name, "attach-session", "-t", SESSION_NAME])
            .status()
            .context("failed to exec tmux attach")?;

        if !status.success() {
            anyhow::bail!("tmux attach-session exited with {status}");
        }
    }

    Ok(())
}

/// Attach to an existing aclaude session.
pub fn run_session_attach(config: &AclaudeConfig, socket: Option<&str>) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    let status = Command::new("tmux")
        .args(["-L", &socket_name, "attach-session", "-t", SESSION_NAME])
        .status()
        .context("failed to exec tmux attach")?;

    if !status.success() {
        anyhow::bail!("no session named '{SESSION_NAME}' on socket '{socket_name}'");
    }

    Ok(())
}

/// Stop the aclaude session.
pub fn run_session_stop(config: &AclaudeConfig, socket: Option<&str>) -> Result<()> {
    let socket_name = socket
        .map(str::to_owned)
        .unwrap_or_else(|| config.tmux.socket.clone());

    let client = Client::connect(&ConnectOptions {
        socket_name: Some(socket_name),
        ..ConnectOptions::default()
    })
    .context("failed to connect to tmux")?;

    if !client.has_session(SESSION_NAME)? {
        println!("No session '{SESSION_NAME}' found.");
        return Ok(());
    }

    let resp = client
        .run_command(&format!(
            "display-message -p -t {SESSION_NAME} '#{{session_id}}'"
        ))
        .context("failed to query session id")?;
    let id_str = resp.first_line().unwrap_or("$0").trim().to_owned();
    let session = tmux_cmc::SessionId::new(&id_str)
        .unwrap_or_else(|_| tmux_cmc::SessionId::new("$0").expect("$0 is valid"));

    client
        .kill_session(&session)
        .context("kill-session failed")?;

    println!("Session '{SESSION_NAME}' stopped.");
    Ok(())
}
