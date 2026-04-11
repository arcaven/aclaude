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
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::time::interval;

use self::app::{AppState, PortraitSize};
use self::input::{InputAction, InputHistory, SlashCmd, handle_key};
use self::layout::compute_layout;
use self::portrait_widget::PortraitWidget;
use crate::bridge;
use crate::config::AclaudeConfig;
use crate::error::Result;
use crate::persona;
use crate::portrait;
use crate::protocol_ext::BridgeEvent;

/// Text batcher — buffers streaming text deltas for smooth rendering.
///
/// Coalesces consecutive TextDelta events and flushes on a 45ms timer
/// or 2KB size threshold. Prevents per-token re-renders at 20fps.
/// Pattern from pi_agent_rust's UiStreamDeltaBatcher.
struct TextBatcher {
    buffer: String,
    last_flush: Instant,
}

impl TextBatcher {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            last_flush: Instant::now(),
        }
    }

    /// Push a text delta. Returns the flushed text if threshold met.
    fn push(&mut self, text: &str) -> Option<String> {
        self.buffer.push_str(text);
        if self.buffer.len() >= 2048 || self.last_flush.elapsed() >= Duration::from_millis(45) {
            self.flush()
        } else {
            None
        }
    }

    /// Force flush any remaining buffer.
    fn flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        self.last_flush = Instant::now();
        Some(std::mem::take(&mut self.buffer))
    }
}

/// Run the TUI.
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

    // App state, input history, text batcher
    let mut state = AppState::new(metrics);
    let mut history = InputHistory::new();
    let mut text_batcher = TextBatcher::new();

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
    let mut tick = interval(std::time::Duration::from_millis(50));

    // Event loop
    let result = loop {
        tokio::select! {
            // Bridge events (NDJSON from subprocess)
            bridge_event = session.event_rx().recv() => {
                match bridge_event {
                    Some(BridgeEvent::TextDelta { text }) => {
                        // Buffer text deltas — flush on threshold
                        if let Some(flushed) = text_batcher.push(&text) {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                    }
                    Some(event) => {
                        // Non-text events flush the batcher immediately
                        if let Some(flushed) = text_batcher.flush() {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                        state.apply_event(&event);
                    }
                    None => {
                        // Subprocess exited — flush remaining
                        if let Some(flushed) = text_batcher.flush() {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                        break Ok(());
                    }
                }
            }

            // Terminal events (keyboard input)
            term_event = term_rx.recv() => {
                match term_event {
                    Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        // Gate input by status — only Ready accepts typed input
                        // (Ctrl+C and slash commands always work)
                        let action = handle_key(key, &mut state.input_buffer, &mut history);
                        match action {
                            InputAction::Quit => break Ok(()),
                            InputAction::SendMessage(text) if state.status.accepts_input() => {
                                state.record_user_message(text.clone());
                                if let Err(e) = session.send_user_message(&text).await {
                                    state.status_message = Some(format!("Send error: {e}"));
                                }
                            }
                            InputAction::SendMessage(_) => {
                                // Input not accepted in current state
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
                        // Terminal resized — next draw picks up new size
                    }
                    Some(_) => {}
                    None => break Ok(()),
                }
            }

            // Tick — render frame + flush batcher on timer
            _ = tick.tick() => {
                // Flush any buffered text on tick (45ms timer threshold)
                if let Some(flushed) = text_batcher.flush() {
                    state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                }

                state.frame_count += 1;

                let has_portrait = portrait_widget.as_ref().is_some_and(portrait_widget::PortraitWidget::has_image);
                terminal.draw(|frame| {
                    let tui_layout = compute_layout(
                        frame.area(),
                        state.portrait_size,
                        has_portrait,
                    );

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
