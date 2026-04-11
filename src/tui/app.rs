//! TUI application state and rendering.
//!
//! `AppState` owns the conversation history, streaming state, and input buffer.
//! Render functions draw the conversation viewport, input area, and status bar.
//! Metrics are read from the shared bridge `Arc<Mutex<SessionMetrics>>`.

use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::scroll::ScrollState;
use crate::protocol::ClaudeEvent;
use crate::protocol_ext::{BridgeEvent, SessionMetrics};

/// Portrait size options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortraitSize {
    Small,
    Medium,
    Large,
    Original,
}

impl PortraitSize {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "original" => Some(Self::Original),
            _ => None,
        }
    }
}

/// A message in the conversation history.
#[derive(Debug)]
pub struct Message {
    pub role: MessageRole,
    pub text: String,
}

#[derive(Debug)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// A tool call entry for display.
#[derive(Debug)]
pub struct ToolCallEntry {
    pub name: String,
    pub status: ToolStatus,
}

#[derive(Debug)]
pub enum ToolStatus {
    Running,
    Complete,
}

/// Application state for the TUI.
pub struct AppState {
    /// Conversation history.
    pub messages: Vec<Message>,
    /// Current streaming text (partial response being assembled).
    pub streaming_text: String,
    /// Tool calls in the current turn.
    pub tool_calls: Vec<ToolCallEntry>,
    /// User input buffer.
    pub input_buffer: String,
    /// Portrait size setting.
    pub portrait_size: PortraitSize,
    /// Shared metrics from bridge.
    pub metrics: Arc<Mutex<SessionMetrics>>,
    /// Whether we're waiting for the first response after sending.
    pub is_waiting: bool,
    /// Scroll state for the conversation viewport.
    pub scroll: ScrollState,
    /// Status message (rate limit notices, errors).
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new(metrics: Arc<Mutex<SessionMetrics>>) -> Self {
        Self {
            messages: Vec::new(),
            streaming_text: String::new(),
            tool_calls: Vec::new(),
            input_buffer: String::new(),
            portrait_size: PortraitSize::Medium,
            metrics,
            is_waiting: false,
            scroll: ScrollState::default(),
            status_message: None,
        }
    }

    /// Apply a bridge event to update state.
    pub fn apply_event(&mut self, event: &BridgeEvent) {
        match event {
            BridgeEvent::TextDelta { text } => {
                self.is_waiting = false;
                self.streaming_text.push_str(text);
            }
            BridgeEvent::Core(ClaudeEvent::Assistant { message }) => {
                self.is_waiting = false;
                // If we have accumulated streaming text and this is a complete
                // message, commit it
                for block in &message.content {
                    match block.block_type.as_str() {
                        "text" => {
                            if let Some(text) = &block.text {
                                // If streaming text matches, it's already accumulated.
                                // If not (non-streaming mode), use the block text.
                                if self.streaming_text.is_empty() {
                                    self.streaming_text.push_str(text);
                                }
                            }
                        }
                        "tool_use" => {
                            if let Some(name) = &block.name {
                                self.tool_calls.push(ToolCallEntry {
                                    name: name.clone(),
                                    status: ToolStatus::Running,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            BridgeEvent::ToolResult { .. } => {
                // Mark the last running tool as complete
                if let Some(last) = self.tool_calls.last_mut() {
                    if matches!(last.status, ToolStatus::Running) {
                        last.status = ToolStatus::Complete;
                    }
                }
            }
            BridgeEvent::Core(ClaudeEvent::Result { .. }) => {
                // Session ended — commit any remaining streaming text
                self.commit_streaming_text();
                self.is_waiting = false;
            }
            BridgeEvent::RateLimit { status, .. } => {
                self.status_message = Some(format!("Rate limit: {status}"));
            }
            BridgeEvent::Core(ClaudeEvent::System { .. }) => {}
            _ => {}
        }
    }

    /// Commit accumulated streaming text as an assistant message.
    pub fn commit_streaming_text(&mut self) {
        if !self.streaming_text.is_empty() {
            let text = std::mem::take(&mut self.streaming_text);
            self.messages.push(Message {
                role: MessageRole::Assistant,
                text,
            });
            self.tool_calls.clear();
        }
    }

    /// Record a sent user message.
    pub fn record_user_message(&mut self, text: String) {
        // Commit any previous assistant response
        self.commit_streaming_text();
        self.messages.push(Message {
            role: MessageRole::User,
            text,
        });
        self.is_waiting = true;
    }
}

/// Render the conversation viewport.
pub fn render_conversation(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    // Render committed messages
    for msg in &state.messages {
        let (prefix, style) = match msg.role {
            MessageRole::User => (
                "You: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            MessageRole::Assistant => ("", Style::default().fg(Color::White)),
            MessageRole::System => ("System: ", Style::default().fg(Color::Yellow)),
        };

        lines.push(Line::from(vec![Span::styled(prefix, style)]));
        for text_line in msg.text.lines() {
            lines.push(Line::from(Span::styled(text_line.to_string(), style)));
        }
        lines.push(Line::from(""));
    }

    // Render active tool calls
    for tool in &state.tool_calls {
        let indicator = match tool.status {
            ToolStatus::Running => "⟳",
            ToolStatus::Complete => "✓",
        };
        lines.push(Line::from(Span::styled(
            format!("  {indicator} {}", tool.name),
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Render streaming text
    if !state.streaming_text.is_empty() {
        for text_line in state.streaming_text.lines() {
            lines.push(Line::from(Span::styled(
                text_line.to_string(),
                Style::default().fg(Color::White),
            )));
        }
        // Streaming cursor
        lines.push(Line::from(Span::styled(
            "▌",
            Style::default().fg(Color::Green),
        )));
    }

    // Waiting indicator
    if state.is_waiting {
        lines.push(Line::from(Span::styled(
            "Thinking...",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let text = Text::from(lines.clone());
    let content_height = text.height() as u16;
    state.scroll.set_viewport_height(area.height);
    state.scroll.set_content_height(content_height);

    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll.offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render the input area.
pub fn render_input(frame: &mut Frame, state: &AppState, area: Rect) {
    let display_text = if state.input_buffer.is_empty() {
        Span::styled("Type a message...", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(
            state.input_buffer.as_str(),
            Style::default().fg(Color::White),
        )
    };

    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Green)),
        display_text,
    ]))
    .block(Block::default().borders(Borders::TOP | Borders::BOTTOM));

    frame.render_widget(input, area);

    // Position cursor after the input text (inside the bordered area)
    // +1 for top border, +2 for "> " prefix
    let cursor_x = area.x + 2 + state.input_buffer.len() as u16;
    let cursor_y = area.y + 1; // +1 for top border
    frame.set_cursor_position((cursor_x.min(area.x + area.width - 1), cursor_y));
}

/// Render the status bar.
pub fn render_status(frame: &mut Frame, state: &AppState, area: Rect) {
    let m = state
        .metrics
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut parts: Vec<Span> = Vec::new();

    // Model
    if !m.model.is_empty() {
        parts.push(Span::styled(
            m.model.clone(),
            Style::default().fg(Color::DarkGray),
        ));
        parts.push(Span::raw(" │ "));
    }

    // Tokens
    parts.push(Span::styled(
        format!("{}↓ {}↑", m.input_tokens, m.output_tokens),
        Style::default().fg(Color::DarkGray),
    ));

    // Cost
    if m.cost_usd > 0.0 {
        parts.push(Span::raw(" │ "));
        parts.push(Span::styled(
            format!("${:.4}", m.cost_usd),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Context %
    if m.context_pct > 0.0 {
        parts.push(Span::raw(" │ "));
        let color = if m.context_pct > 90.0 {
            Color::Red
        } else if m.context_pct > 70.0 {
            Color::Yellow
        } else {
            Color::Green
        };
        parts.push(Span::styled(
            format!("ctx {:.0}%", m.context_pct),
            Style::default().fg(color),
        ));
    }

    // Active tool
    if let Some(tool) = &m.active_tool {
        parts.push(Span::raw(" │ "));
        parts.push(Span::styled(
            format!("⟳ {tool}"),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Rate limit
    if let Some(rl) = &m.rate_limit_status {
        parts.push(Span::raw(" │ "));
        parts.push(Span::styled(rl.clone(), Style::default().fg(Color::Red)));
    }

    // Status message
    if let Some(msg) = &state.status_message {
        parts.push(Span::raw(" │ "));
        parts.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }

    let status = Paragraph::new(Line::from(parts));
    frame.render_widget(status, area);
}
