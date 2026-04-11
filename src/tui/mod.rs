//! TUI module — ratatui-based terminal interface for aclaude.
//!
//! Wraps Claude Code as a headless subprocess and renders a custom TUI
//! around the NDJSON event stream. This is one consumer of the session
//! bridge — the bridge is shared infrastructure at `src/` level.

pub mod app;
pub mod input;
pub mod layout;
pub mod portrait_widget;
pub mod scroll;

use std::io;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::time::{Duration, interval};

use self::app::{AppState, PortraitSize};
use self::input::{InputAction, InputHistory, SlashCmd, handle_key};
use self::layout::compute_layout;
use self::portrait_widget::PortraitWidget;
use crate::bridge;
use crate::config::AclaudeConfig;
use crate::error::Result;
use crate::persona;
use crate::portrait;

/// Run the TUI prototype.
///
/// Sets up the terminal, spawns the Claude Code bridge subprocess,
/// and runs the event loop until quit.
pub async fn run_tui(config: &AclaudeConfig) -> Result<()> {
    // Resolve portrait
    let theme = persona::load_theme(&config.persona.theme)?;
    let agent = persona::get_agent(&theme, &config.persona.role)?;
    let portrait_paths =
        portrait::resolve_portrait(&config.persona.theme, agent, Some(&config.persona.role));

    // Terminal setup: raw mode FIRST, then picker query, then terminal
    enable_raw_mode().map_err(|e| crate::error::AclaudeError::Session {
        message: format!("failed to enable raw mode: {e}"),
    })?;

    // Portrait widget (queries terminal capabilities via stdio)
    let mut portrait_widget = PortraitWidget::new();
    if let Some(pw) = &mut portrait_widget {
        pw.set_size(PortraitSize::Medium, &portrait_paths);
    }

    execute!(io::stdout(), EnterAlternateScreen).map_err(|e| {
        let _ = disable_raw_mode();
        crate::error::AclaudeError::Session {
            message: format!("failed to enter alternate screen: {e}"),
        }
    })?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        crate::error::AclaudeError::Session {
            message: format!("failed to create terminal: {e}"),
        }
    })?;

    // Spawn bridge subprocess
    let mut session = bridge::Session::spawn(config).await?;
    let metrics = session.metrics();

    // App state and input history
    let mut state = AppState::new(metrics);
    let mut history = InputHistory::new();

    // Terminal event reader channel
    let (term_tx, mut term_rx) = tokio::sync::mpsc::channel::<Event>(64);
    tokio::task::spawn_blocking(move || {
        while let Ok(ev) = event::read() {
            if term_tx.blocking_send(ev).is_err() {
                break;
            }
        }
    });

    // 20fps tick
    let mut tick = interval(Duration::from_millis(50));

    // Event loop
    let result = loop {
        tokio::select! {
            // Bridge events (NDJSON from subprocess)
            bridge_event = session.event_rx().recv() => {
                match bridge_event {
                    Some(event) => {
                        state.apply_event(&event);
                    }
                    None => {
                        // Subprocess exited
                        break Ok(());
                    }
                }
            }

            // Terminal events (keyboard input)
            term_event = term_rx.recv() => {
                match term_event {
                    Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        match handle_key(key, &mut state.input_buffer, &mut history) {
                            InputAction::Quit => break Ok(()),
                            InputAction::SendMessage(text) => {
                                state.record_user_message(text.clone());
                                if let Err(e) = session.send_user_message(&text).await {
                                    state.status_message = Some(format!("Send error: {e}"));
                                }
                            }
                            InputAction::SlashCommand(SlashCmd::Exit) => break Ok(()),
                            InputAction::SlashCommand(SlashCmd::Login) => {
                                state.status_message = Some(
                                    "Auth is managed by Claude Code. Run `claude login` in a separate terminal to re-authenticate.".to_string()
                                );
                            }
                            InputAction::SlashCommand(SlashCmd::PortraitSize(size_str)) => {
                                if let Some(size) = PortraitSize::parse(&size_str) {
                                    state.portrait_size = size;
                                    if let Some(pw) = &mut portrait_widget {
                                        pw.set_size(size, &portrait_paths);
                                    }
                                }
                            }
                            InputAction::SlashCommand(SlashCmd::Unknown(cmd)) => {
                                state.status_message = Some(format!("Unknown command: {cmd}"));
                            }
                            InputAction::PageUp => state.scroll.page_up(),
                            InputAction::PageDown => state.scroll.page_down(),
                            InputAction::ScrollEnd => state.scroll.scroll_to_bottom(),
                            InputAction::None => {}
                        }
                    }
                    Some(Event::Resize(_, _)) => {
                        // Terminal resized — next draw will pick up new size
                    }
                    Some(_) => {}
                    None => break Ok(()),
                }
            }

            // Tick — render frame
            _ = tick.tick() => {
                let has_portrait = portrait_widget.as_ref().is_some_and(portrait_widget::PortraitWidget::has_image);
                terminal.draw(|frame| {
                    let tui_layout = compute_layout(
                        frame.area(),
                        state.portrait_size,
                        has_portrait,
                    );

                    // Conversation first (full width), then portrait overlays on top
                    app::render_conversation(frame, &mut state, tui_layout.conversation);
                    if let Some(pw) = &mut portrait_widget {
                        pw.render(frame, tui_layout.portrait);
                    }

                    app::render_input(frame, &state, tui_layout.input);
                    app::render_status(frame, &state, tui_layout.status);
                }).ok();
            }
        }
    };

    // Cleanup
    session.shutdown().await;
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    let _ = disable_raw_mode();

    result
}
