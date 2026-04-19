//! TUI module — ratatui-based terminal interface for forestage.
//!
//! Wraps Claude Code as a headless subprocess and renders a custom TUI
//! around the NDJSON event stream. This is one consumer of the session
//! bridge — the bridge is shared infrastructure at `src/` level.

pub mod app;
pub mod diff;
pub mod input;
pub mod layout;
pub mod markdown;
pub mod portrait_widget;
pub mod scroll;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
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
use crate::config::ForestageConfig;
use crate::error::Result;
use crate::persona;
use crate::portrait;
use crate::protocol_ext::BridgeEvent;

/// Copy text to system clipboard via OSC 52 escape sequence.
///
/// OSC 52 is widely supported (iTerm2, WezTerm, Kitty, Ghostty, tmux 3.2+).
/// Works over SSH and inside tmux — no platform-specific clipboard binary needed.
fn copy_to_clipboard(text: &str) {
    let encoded = BASE64.encode(text.as_bytes());
    // OSC 52: \x1b]52;c;<base64>\x07
    let _ = execute!(
        io::stdout(),
        crossterm::style::Print(format!("\x1b]52;c;{encoded}\x07"))
    );
}

/// Text batcher — buffers streaming text deltas for smooth rendering.
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

    fn push(&mut self, text: &str) -> Option<String> {
        self.buffer.push_str(text);
        if self.buffer.len() >= 2048 || self.last_flush.elapsed() >= Duration::from_millis(45) {
            self.flush()
        } else {
            None
        }
    }

    fn flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        self.last_flush = Instant::now();
        Some(std::mem::take(&mut self.buffer))
    }
}

/// Run the TUI.
pub async fn run_tui(config: &ForestageConfig) -> Result<()> {
    // Install panic hook to restore terminal state on panic.
    // Without this, a panic leaves the terminal in raw mode with
    // alternate screen and mouse capture still active.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableFocusChange,
            LeaveAlternateScreen
        );
        original_hook(info);
    }));

    // Resolve portrait
    let theme = persona::load_theme(&config.persona.theme)?;
    let character = persona::resolve_character(&theme, &config.persona)?;
    let portrait_paths = portrait::resolve_portrait(&config.persona.theme, character);

    // Terminal setup: raw mode FIRST, then picker query, then terminal
    enable_raw_mode().map_err(|e| crate::error::ForestageError::Session {
        message: format!("failed to enable raw mode: {e}"),
    })?;

    let mut portrait_widget = PortraitWidget::new();
    if let Some(pw) = &mut portrait_widget {
        pw.set_size(PortraitSize::Large, &portrait_paths);
    }

    // Mouse capture OFF by default — native text selection is more important
    // than mouse wheel scroll. F2 toggles mouse capture on for scrolling.
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableFocusChange
    )
    .map_err(|e| {
        let _ = disable_raw_mode();
        crate::error::ForestageError::Session {
            message: format!("failed to enter alternate screen: {e}"),
        }
    })?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        crate::error::ForestageError::Session {
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

    // Mouse capture off by default — text selection works natively.
    // F2 toggles mouse capture on for scroll wheel support.
    let mut mouse_captured = false;

    // Terminal event reader channel.
    // Uses poll() with a timeout so the thread can check the shutdown flag
    // and exit cleanly — without this, event::read() blocks indefinitely
    // and the tokio runtime hangs on shutdown until the user presses a key.
    let term_shutdown = Arc::new(AtomicBool::new(false));
    let term_shutdown_flag = Arc::clone(&term_shutdown);
    let (term_tx, mut term_rx) = tokio::sync::mpsc::channel::<Event>(64);
    tokio::task::spawn_blocking(move || {
        loop {
            if term_shutdown_flag.load(Ordering::Relaxed) {
                break;
            }
            // Poll with 100ms timeout to periodically check shutdown flag
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                match event::read() {
                    Ok(ev) => {
                        if term_tx.blocking_send(ev).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    // 20fps tick
    let mut tick = interval(std::time::Duration::from_millis(50));

    // Event loop
    let result = loop {
        tokio::select! {
            // Bridge events
            bridge_event = session.event_rx().recv() => {
                match bridge_event {
                    Some(BridgeEvent::TextDelta { text }) => {
                        if let Some(flushed) = text_batcher.push(&text) {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                    }
                    Some(event) => {
                        if let Some(flushed) = text_batcher.flush() {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                        state.apply_event(&event);
                    }
                    None => {
                        if let Some(flushed) = text_batcher.flush() {
                            state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                        }
                        break Ok(());
                    }
                }
            }

            // Terminal events
            term_event = term_rx.recv() => {
                match term_event {
                    Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        let has_perm = state.pending_permission.is_some();
                        let action = handle_key(
                            key,
                            &mut state.input,
                            &mut history,
                            has_perm,
                            &state.available_slash_commands,
                        );
                        match action {
                            InputAction::Quit => break Ok(()),

                            InputAction::Interrupt => {
                                if state.status == app::AppStatus::Thinking
                                    || state.status == app::AppStatus::Streaming
                                    || state.status == app::AppStatus::ToolRunning
                                {
                                    session.interrupt();
                                    state.set_status("Interrupted".to_string());
                                }
                            }

                            InputAction::CopySelection(text) => {
                                copy_to_clipboard(&text);
                                state.set_status("Copied to clipboard".to_string());
                            }
                            InputAction::CutSelection(text) => {
                                copy_to_clipboard(&text);
                                state.set_status("Cut to clipboard".to_string());
                            }

                            InputAction::SendMessage(text) if state.status.accepts_input() => {
                                state.record_user_message(text.clone());
                                if let Err(e) = session.send_user_message(&text).await {
                                    state.set_status(format!("Send error: {e}"));
                                }
                            }
                            InputAction::SendMessage(_) => {}

                            InputAction::SlashCommand(SlashCmd::Exit) => break Ok(()),
                            InputAction::SlashCommand(SlashCmd::Login) => {
                                state.set_status(
                                    "Auth is managed by Claude Code. Run `claude login` in a separate terminal.".to_string()
                                );
                            }
                            InputAction::SlashCommand(SlashCmd::Clear) => {
                                state.items.clear();
                                state.set_status("Conversation cleared".to_string());
                            }
                            InputAction::SlashCommand(SlashCmd::Help) => {
                                let help_text = [
                                    "Commands: /exit /clear /cost /help /login /compact /model /persona",
                                    "Portrait: /persona portrait [on|off|top|bottom|size <s>]",
                                    "",
                                    "Keys:",
                                    "  Esc interrupt       Ctrl+C quit/copy      Ctrl+X cut/expand",
                                    "  Ctrl+A/E home/end   Ctrl+W del word       Ctrl+U clear line",
                                    "  Ctrl+O transcript   Ctrl+P portrait pos   Alt+P portrait on/off",
                                    "  Alt+S portrait size Alt+T thinking        Shift+Tab perm mode",
                                    "  Shift+Arrow select  F2 toggle scroll      Up/Down history",
                                    "  Tab complete        Mouse: drag to select text",
                                ].join("\n");
                                state.items.push(app::ConversationItem::SystemNotice { text: help_text });
                            }
                            InputAction::SlashCommand(SlashCmd::Cost) => {
                                let msg = {
                                    let m = state.metrics.lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                                    format!(
                                        "Cost: ${:.4} | Turns: {} | Tokens: {}↓ {}↑",
                                        m.cost_usd, m.num_turns, m.input_tokens, m.output_tokens
                                    )
                                };
                                state.set_status(msg);
                            }
                            InputAction::SlashCommand(SlashCmd::ForwardToAgent(cmd)) => {
                                // Forward Claude Code slash commands as user messages
                                state.record_user_message(cmd.clone());
                                if let Err(e) = session.send_user_message(&cmd).await {
                                    state.set_status(format!("Send error: {e}"));
                                }
                            }
                            InputAction::SlashCommand(SlashCmd::PortraitSize(size_str)) => {
                                if let Some(size) = PortraitSize::parse(&size_str) {
                                    state.portrait_size = size;
                                    if let Some(pw) = &mut portrait_widget {
                                        pw.set_size(size, &portrait_paths);
                                    }
                                }
                            }
                            InputAction::SlashCommand(SlashCmd::PortraitToggle(on)) => {
                                state.portrait_visible = on;
                                state.set_status(format!(
                                    "Portrait {}",
                                    if on { "on" } else { "off" }
                                ));
                            }
                            InputAction::SlashCommand(SlashCmd::PortraitMove(pos)) => {
                                state.portrait_position = match pos.as_str() {
                                    "top" => app::PortraitPosition::TopRight,
                                    "bottom" => app::PortraitPosition::BottomRight,
                                    _ => state.portrait_position,
                                };
                                state.set_status(format!("Portrait: {pos}"));
                            }
                            InputAction::SlashCommand(SlashCmd::Unknown(cmd)) => {
                                // Forward unknown / commands to Claude Code — they
                                // may be valid MCP/skill commands from system/init
                                state.record_user_message(cmd.clone());
                                if let Err(e) = session.send_user_message(&cmd).await {
                                    state.set_status(format!("Send error: {e}"));
                                }
                            }

                            InputAction::CyclePermissionMode => {
                                state.permission_mode = state.permission_mode.next();
                                state.set_status(format!(
                                    "Permission mode: {}",
                                    state.permission_mode.label()
                                ));
                            }

                            // Dynamic permission responses over stream-json
                            // stdin are architecturally impossible — see
                            // finding-021 (aae-orc _kos). send_permission_response
                            // always returns an error explaining that. We
                            // still clear the pending prompt so the UI doesn't
                            // wedge; the user's recourse is to relaunch with
                            // --dangerously-skip-permissions / --allowedTools.
                            InputAction::PermissionAllow | InputAction::PermissionDeny => {
                                state.pending_permission = None;
                                match session.send_permission_response(false).await {
                                    Ok(()) => unreachable!(
                                        "send_permission_response must return Err until HTTP hooks land"
                                    ),
                                    Err(e) => state.set_status(format!("{e}")),
                                }
                            }

                            InputAction::ToggleMouseCapture => {
                                if mouse_captured {
                                    let _ = execute!(io::stdout(), DisableMouseCapture);
                                    mouse_captured = false;
                                    state.set_status("Mouse scroll off — drag to select text".to_string());
                                } else {
                                    let _ = execute!(io::stdout(), EnableMouseCapture);
                                    mouse_captured = true;
                                    state.set_status("Mouse scroll on — F2 to toggle back".to_string());
                                }
                            }
                            InputAction::PortraitTogglePosition => {
                                state.portrait_position = state.portrait_position.toggle();
                                state.set_status(format!(
                                    "Portrait: {}",
                                    match state.portrait_position {
                                        app::PortraitPosition::TopRight => "top",
                                        app::PortraitPosition::BottomRight => "bottom",
                                    }
                                ));
                            }
                            InputAction::PortraitToggleVisible => {
                                state.portrait_visible = !state.portrait_visible;
                                state.set_status(format!(
                                    "Portrait {}",
                                    if state.portrait_visible { "on" } else { "off" }
                                ));
                            }
                            InputAction::PortraitCycleSize => {
                                state.portrait_size = state.portrait_size.next();
                                if let Some(pw) = &mut portrait_widget {
                                    pw.set_size(state.portrait_size, &portrait_paths);
                                }
                                state.set_status(format!(
                                    "Portrait size: {}",
                                    state.portrait_size.label()
                                ));
                            }
                            InputAction::CycleTranscript => {
                                let next = state.transcript_mode.next();
                                if next == app::TranscriptMode::Transcript {
                                    // Dump conversation to native terminal scrollback
                                    let _ = execute!(
                                        io::stdout(),
                                        DisableBracketedPaste,
                                        DisableMouseCapture,
                                        LeaveAlternateScreen
                                    );
                                    let _ = disable_raw_mode();
                                    print!("{}", state.conversation_as_text());
                                    println!("--- Press any key to return ---");
                                    let _ = enable_raw_mode();
                                    let _ = crossterm::event::read();
                                    let _ = execute!(
                                        io::stdout(),
                                        EnterAlternateScreen,
                                        EnableMouseCapture,
                                        EnableBracketedPaste
                                    );
                                    // Stay in Normal after viewing transcript
                                    state.transcript_mode = app::TranscriptMode::Normal;
                                } else {
                                    state.transcript_mode = next;
                                    state.set_status(format!(
                                        "View: {}",
                                        next.label()
                                    ));
                                }
                            }
                            InputAction::ToggleExpand => state.toggle_last_tool_expand(),
                            InputAction::ToggleThinking => {
                                state.show_thinking = !state.show_thinking;
                                state.set_status(format!(
                                    "Thinking blocks: {}",
                                    if state.show_thinking { "shown" } else { "hidden" }
                                ));
                            }
                            InputAction::OpenEditor => {
                                // Stub — full implementation in a later pass
                                state.set_status(
                                    "External editor not yet implemented. Use Ctrl+U to clear input.".to_string()
                                );
                            }
                            InputAction::ScrollUp => state.scroll.scroll_up(),
                            InputAction::ScrollDown => state.scroll.scroll_down(),
                            InputAction::PageUp => state.scroll.page_up(),
                            InputAction::PageDown => state.scroll.page_down(),
                            InputAction::ScrollEnd => state.scroll.scroll_to_bottom(),
                            InputAction::None => {}
                        }
                    }
                    Some(Event::Paste(text)) => {
                        // Replace selection if one exists
                        if state.input.selection_anchor.is_some() {
                            state.input.delete_selection();
                        }
                        // Bracketed paste — insert pasted text at cursor.
                        // Newlines are preserved as spaces for single-line input.
                        // Drag-drop: terminals emit absolute file paths on drop.
                        let cleaned = text.replace('\r', "");
                        let is_file_drop = cleaned.lines().count() == 1
                            && cleaned.trim().starts_with('/')
                            && std::path::Path::new(cleaned.trim()).exists();

                        if is_file_drop {
                            // Insert as @-mention for Claude Code file reference
                            let path = cleaned.trim();
                            for c in format!("@{path}").chars() {
                                state.input.insert(c);
                            }
                        } else {
                            // Preserve newlines as actual newlines in the buffer
                            for c in cleaned.chars() {
                                state.input.insert(c);
                            }
                        }
                    }
                    Some(Event::Mouse(mouse)) => {
                        if mouse.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) {
                            // Shift+mouse — release capture for native selection
                            if mouse_captured {
                                let _ = execute!(io::stdout(), DisableMouseCapture);
                                mouse_captured = false;
                            }
                        } else {
                            match mouse.kind {
                                MouseEventKind::ScrollUp => state.scroll.scroll_up(),
                                MouseEventKind::ScrollDown => state.scroll.scroll_down(),
                                _ => {}
                            }
                        }
                    }
                    Some(Event::FocusGained) | Some(Event::Resize(_, _)) => {
                        // Focus gained (window switch) or resize (new window added)
                        // clears the terminal graphics layer. Clear ratatui's diff
                        // cache and force portrait re-send.
                        let _ = terminal.clear();
                        if let Some(pw) = &mut portrait_widget {
                            pw.force_redraw();
                        }
                    }
                    Some(Event::FocusLost) => {}
                    Some(_) => {}
                    None => break Ok(()),
                }
            }

            // Tick — render frame
            _ = tick.tick() => {
                if let Some(flushed) = text_batcher.flush() {
                    state.apply_event(&BridgeEvent::TextDelta { text: flushed });
                }

                state.frame_count += 1;
                state.tick_status_timeout();


                let has_perm_prompt = state.pending_permission.is_some();

                terminal.draw(|frame| {
                    let area = frame.area();
                    // Compute portrait cell size from actual image dimensions
                    let portrait_cell_size = if state.portrait_visible {
                        portrait_widget.as_ref().and_then(|pw| {
                            let max_w = portrait_max_width(state.portrait_size, area.width);
                            let max_h = area.height / 2;
                            pw.cell_size(max_w, max_h)
                        })
                    } else {
                        None
                    };

                    let is_focus = state.transcript_mode == app::TranscriptMode::Focus;
                    let tui_layout = compute_layout(
                        area,
                        state.portrait_position,
                        portrait_cell_size,
                        has_perm_prompt,
                        is_focus,
                        state.input.buffer.len(),
                    );

                    app::render_conversation(frame, &mut state, tui_layout.conversation);
                    if let Some(pw) = &mut portrait_widget {
                        pw.render(frame, tui_layout.portrait);
                    }

                    if let Some(prompt) = &state.pending_permission {
                        app::render_permission_prompt(frame, prompt, tui_layout.permission_prompt);
                    }

                    app::render_input(frame, &state, tui_layout.input);
                    if tui_layout.status.height > 0 {
                        app::render_status(frame, &state, tui_layout.status);
                    }
                }).ok();
            }
        }
    };

    // Cleanup — signal terminal reader thread to exit before waiting
    term_shutdown.store(true, Ordering::Relaxed);
    session.shutdown().await;
    if mouse_captured {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        DisableFocusChange,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();

    result
}

/// Max width in cells for a portrait size setting.
fn portrait_max_width(size: PortraitSize, terminal_width: u16) -> u16 {
    match size {
        PortraitSize::Small => 20,
        PortraitSize::Medium => 32,
        PortraitSize::Large => 48,
        PortraitSize::Original => (terminal_width / 3).min(64),
    }
}
