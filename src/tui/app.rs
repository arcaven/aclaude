//! TUI application state and rendering.
//!
//! `AppState` owns the conversation as a structured turn model, driven by
//! an `AppStatus` state machine. Render functions draw the conversation
//! viewport, input area, and status bar using a `RenderCtx` for purity.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::scroll::ScrollState;
use crate::protocol::ClaudeEvent;
use crate::protocol_ext::{BridgeEvent, SessionMetrics};

// ── Types ────────────────────────────────────────────────────────────────

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

    /// Cycle to the next size.
    pub fn next(self) -> Self {
        match self {
            Self::Small => Self::Medium,
            Self::Medium => Self::Large,
            Self::Large => Self::Original,
            Self::Original => Self::Small,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::Original => "original",
        }
    }
}

/// Portrait position in the conversation viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortraitPosition {
    /// Upper-right corner of conversation area.
    TopRight,
    /// Lower-right corner, just above the input area.
    BottomRight,
}

impl PortraitPosition {
    /// Toggle between top and bottom.
    pub fn toggle(self) -> Self {
        match self {
            Self::TopRight => Self::BottomRight,
            Self::BottomRight => Self::TopRight,
        }
    }
}

/// Transcript/view mode — cycled via Ctrl+O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranscriptMode {
    /// Normal TUI rendering in alternate screen.
    #[default]
    Normal,
    /// Dump conversation to native terminal scrollback.
    Transcript,
    /// Distraction-free: hide status bar and input borders.
    Focus,
}

impl TranscriptMode {
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Transcript,
            Self::Transcript => Self::Focus,
            Self::Focus => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Transcript => "transcript",
            Self::Focus => "focus",
        }
    }
}

/// A diagnostic entry (placeholder for future LSP integration).
#[derive(Debug)]
#[allow(dead_code)]
pub struct DiagnosticEntry {
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// Application status — drives visual feedback and input gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppStatus {
    /// Waiting for system/init from subprocess.
    Connecting,
    /// Idle, accepting user input.
    Ready,
    /// Assistant turn started, no content yet.
    Thinking,
    /// Text deltas arriving.
    Streaming,
    /// Tool call in progress.
    ToolRunning,
    /// Fatal error.
    Error,
}

impl AppStatus {
    /// Whether the input field should accept typed characters.
    /// Permissive — only blocks on fatal error. The user should always
    /// be able to type; the subprocess accepts input regardless of state.
    pub fn accepts_input(self) -> bool {
        self != Self::Error
    }

    /// Spinner character for active states.
    pub fn spinner(self, frame_count: u64) -> Option<&'static str> {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        match self {
            Self::Thinking | Self::Streaming | Self::ToolRunning => {
                Some(FRAMES[(frame_count as usize) % FRAMES.len()])
            }
            _ => None,
        }
    }
}

// ── Conversation Model ───────────────────────────────────────────────────

/// A single item in the conversation.
#[derive(Debug)]
pub enum ConversationItem {
    /// User-sent message.
    UserMessage { text: String },
    /// One assistant turn (may contain text, tool calls, thinking).
    AssistantTurn {
        blocks: Vec<TurnBlock>,
        is_active: bool,
    },
    /// Synthetic system notice (compact markers, errors).
    SystemNotice { text: String },
}

/// A block within an assistant turn.
#[derive(Debug)]
pub enum TurnBlock {
    /// Text content (streaming or committed).
    Text { content: String, is_streaming: bool },
    /// A tool call with lifecycle tracking.
    ToolCall(ToolCallItem),
    /// A thinking block (streaming or committed).
    Thinking { content: String, is_streaming: bool },
}

/// Tool call lifecycle state.
#[derive(Debug)]
pub struct ToolCallItem {
    pub id: String,
    pub name: String,
    /// Accumulated input_json_delta chunks.
    pub input_json: String,
    /// First 200 chars of tool result.
    pub result_preview: String,
    pub status: ToolStatus,
    pub started_at: Instant,
    pub is_expanded: bool,
    /// Diagnostics (placeholder — empty until LSP wired).
    pub diagnostics: Vec<DiagnosticEntry>,
}

/// Tool execution status.
#[derive(Debug)]
pub enum ToolStatus {
    /// Input JSON still arriving via deltas.
    InputStreaming,
    /// Tool executing (content_block_stop seen, awaiting result).
    Running,
    /// Tool completed successfully.
    Complete { elapsed_secs: f64 },
    /// Tool errored.
    Error { message: String },
}

/// Permission mode — cycled via Shift+Tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    Auto,
    Bypass,
}

impl PermissionMode {
    /// Cycle to the next permission mode.
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::AcceptEdits,
            Self::AcceptEdits => Self::Plan,
            Self::Plan => Self::Auto,
            Self::Auto => Self::Bypass,
            Self::Bypass => Self::Default,
        }
    }

    /// Display label for the status bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::Bypass => "bypass",
        }
    }

    /// Color for the status bar indicator.
    pub fn color(self) -> Color {
        match self {
            Self::Default => Color::Green,
            Self::AcceptEdits | Self::Plan => Color::Yellow,
            Self::Auto => Color::Cyan,
            Self::Bypass => Color::Red,
        }
    }

    /// Parse from system/init permission_mode value.
    pub fn parse_mode(s: &str) -> Self {
        match s {
            "acceptEdits" => Self::AcceptEdits,
            "plan" => Self::Plan,
            "auto" => Self::Auto,
            "dontAsk" | "bypass" | "bypassPermissions" => Self::Bypass,
            _ => Self::Default,
        }
    }
}

/// A pending permission prompt for tool approval.
#[derive(Debug)]
pub struct PermissionPrompt {
    pub tool: String,
    pub description: String,
}

// ── App State ────────────────────────────────────────────────────────────

/// Application state for the TUI.
pub struct AppState {
    /// Structured conversation history.
    pub items: Vec<ConversationItem>,
    /// User input state (buffer + cursor).
    pub input: super::input::InputState,
    /// Portrait size setting.
    pub portrait_size: PortraitSize,
    /// Whether the portrait is visible.
    pub portrait_visible: bool,
    /// Portrait position (top-right or bottom-right).
    pub portrait_position: PortraitPosition,
    /// Shared metrics from bridge.
    pub metrics: Arc<Mutex<SessionMetrics>>,
    /// Current application status.
    pub status: AppStatus,
    /// Scroll state for the conversation viewport.
    pub scroll: ScrollState,
    /// Status message (rate limit notices, errors).
    pub status_message: Option<String>,
    /// Available slash commands from system/init.
    pub available_slash_commands: Vec<String>,
    /// Whether to show thinking blocks.
    pub show_thinking: bool,
    /// Frame counter for spinner animation.
    pub frame_count: u64,
    /// Current permission mode.
    pub permission_mode: PermissionMode,
    /// Pending permission prompt (blocks normal input when Some).
    pub pending_permission: Option<PermissionPrompt>,
    /// When the status message was set (for auto-clear after timeout).
    pub status_message_at: Option<Instant>,
    /// Current transcript/view mode.
    pub transcript_mode: TranscriptMode,
}

impl AppState {
    pub fn new(metrics: Arc<Mutex<SessionMetrics>>) -> Self {
        Self {
            items: Vec::new(),
            input: super::input::InputState::default(),
            portrait_size: PortraitSize::Medium,
            portrait_visible: true,
            portrait_position: PortraitPosition::TopRight,
            metrics,
            status: AppStatus::Connecting,
            scroll: ScrollState::default(),
            status_message: None,
            available_slash_commands: Vec::new(),
            show_thinking: false,
            frame_count: 0,
            permission_mode: PermissionMode::Default,
            pending_permission: None,
            status_message_at: None,
            transcript_mode: TranscriptMode::default(),
        }
    }

    /// Apply a bridge event to update state.
    pub fn apply_event(&mut self, event: &BridgeEvent) {
        // Any event arriving means we're connected
        if self.status == AppStatus::Connecting {
            self.status = AppStatus::Ready;
        }

        match event {
            BridgeEvent::SessionInit {
                available_slash_commands,
                permission_mode,
                ..
            } => {
                self.status = AppStatus::Ready;
                self.available_slash_commands = available_slash_commands.clone();
                self.permission_mode = PermissionMode::parse_mode(permission_mode);
            }
            BridgeEvent::Core(ClaudeEvent::System { .. }) => {}
            BridgeEvent::MessageStart => {
                self.status = AppStatus::Thinking;
                self.ensure_active_turn();
            }
            BridgeEvent::TextDelta { text } => {
                self.status = AppStatus::Streaming;
                self.append_streaming_text(text);
            }
            BridgeEvent::ToolCallStart { id, name } => {
                self.status = AppStatus::ToolRunning;
                self.commit_streaming_text();
                self.ensure_active_turn();
                if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut()
                {
                    blocks.push(TurnBlock::ToolCall(ToolCallItem {
                        id: id.clone(),
                        name: name.clone(),
                        input_json: String::new(),
                        result_preview: String::new(),
                        status: ToolStatus::InputStreaming,
                        started_at: Instant::now(),
                        is_expanded: false,
                        diagnostics: Vec::new(),
                    }));
                }
            }
            BridgeEvent::ToolInputDelta { partial_json } => {
                if let Some(tool) = self.last_tool_mut() {
                    tool.input_json.push_str(partial_json);
                }
            }
            BridgeEvent::ToolCallStop => {
                if let Some(tool) = self.last_tool_mut() {
                    tool.status = ToolStatus::Running;
                }
            }
            BridgeEvent::ToolResult {
                tool_use_id,
                content,
            } => {
                // Find tool by id and mark complete
                if let Some(tool) = self.find_tool_mut(tool_use_id) {
                    let elapsed = tool.started_at.elapsed().as_secs_f64();
                    tool.status = ToolStatus::Complete {
                        elapsed_secs: elapsed,
                    };
                    tool.result_preview = content.chars().take(200).collect();
                }
                // After tool result, Claude may continue streaming text
                self.status = AppStatus::Streaming;
            }
            BridgeEvent::ThinkingStart => {
                self.ensure_active_turn();
                if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut()
                {
                    blocks.push(TurnBlock::Thinking {
                        content: String::new(),
                        is_streaming: true,
                    });
                }
            }
            BridgeEvent::ThinkingDelta { text } => {
                if let Some(thinking) = self.last_thinking_mut() {
                    thinking.push_str(text);
                }
            }
            BridgeEvent::ThinkingStop => {
                // Mark thinking as committed
                if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut()
                {
                    for block in blocks.iter_mut().rev() {
                        if let TurnBlock::Thinking { is_streaming, .. } = block {
                            *is_streaming = false;
                            break;
                        }
                    }
                }
            }
            BridgeEvent::MessageStop { .. } | BridgeEvent::Core(ClaudeEvent::Result { .. }) => {
                self.commit_streaming_text();
                self.finalize_turn();
                self.status = AppStatus::Ready;
            }
            BridgeEvent::Core(ClaudeEvent::Assistant { message }) => {
                // Handle complete assistant messages (non-streaming mode)
                if self.status == AppStatus::Connecting || self.status == AppStatus::Ready {
                    self.status = AppStatus::Streaming;
                }
                self.ensure_active_turn();
                for block in &message.content {
                    match block.block_type.as_str() {
                        "text" => {
                            if let Some(text) = &block.text {
                                self.append_streaming_text(text);
                            }
                        }
                        "tool_use" => {
                            if let Some(name) = &block.name {
                                let id = block.id.clone().unwrap_or_default();
                                if let Some(ConversationItem::AssistantTurn { blocks, .. }) =
                                    self.items.last_mut()
                                {
                                    blocks.push(TurnBlock::ToolCall(ToolCallItem {
                                        id,
                                        name: name.clone(),
                                        input_json: String::new(),
                                        result_preview: String::new(),
                                        status: ToolStatus::Running,
                                        started_at: Instant::now(),
                                        is_expanded: false,
                                        diagnostics: Vec::new(),
                                    }));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            BridgeEvent::RateLimit { status, .. } => {
                self.status_message = Some(format!("Rate limit: {status}"));
            }
            BridgeEvent::PermissionRequest { tool, description } => {
                self.pending_permission = Some(PermissionPrompt {
                    tool: tool.clone(),
                    description: description.clone(),
                });
            }
            _ => {}
        }
    }

    /// Record a sent user message and transition to Thinking.
    pub fn record_user_message(&mut self, text: String) {
        self.commit_streaming_text();
        self.finalize_turn();
        self.items.push(ConversationItem::UserMessage { text });
        self.status = AppStatus::Thinking;
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Ensure there's an active assistant turn at the end of items.
    fn ensure_active_turn(&mut self) -> &mut ConversationItem {
        if !matches!(
            self.items.last(),
            Some(ConversationItem::AssistantTurn {
                is_active: true,
                ..
            })
        ) {
            self.items.push(ConversationItem::AssistantTurn {
                blocks: Vec::new(),
                is_active: true,
            });
        }
        self.items.last_mut().expect("just pushed")
    }

    /// Append text to the current streaming text block, creating one if needed.
    fn append_streaming_text(&mut self, text: &str) {
        let turn = self.ensure_active_turn();
        if let ConversationItem::AssistantTurn { blocks, .. } = turn {
            // Find last streaming text block or create one
            let needs_new = blocks.last().is_none_or(|b| {
                !matches!(
                    b,
                    TurnBlock::Text {
                        is_streaming: true,
                        ..
                    }
                )
            });
            if needs_new {
                blocks.push(TurnBlock::Text {
                    content: String::new(),
                    is_streaming: true,
                });
            }
            if let Some(TurnBlock::Text { content, .. }) = blocks.last_mut() {
                content.push_str(text);
            }
        }
    }

    /// Commit any streaming text block (mark as not streaming).
    fn commit_streaming_text(&mut self) {
        if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut() {
            for block in blocks.iter_mut() {
                if let TurnBlock::Text {
                    is_streaming,
                    content,
                    ..
                } = block
                {
                    if *is_streaming && !content.is_empty() {
                        *is_streaming = false;
                    }
                }
            }
        }
    }

    /// Mark the current turn as finalized.
    fn finalize_turn(&mut self) {
        if let Some(ConversationItem::AssistantTurn {
            is_active, blocks, ..
        }) = self.items.last_mut()
        {
            // Remove empty text blocks
            blocks.retain(|b| !matches!(b, TurnBlock::Text { content, .. } if content.is_empty()));
            *is_active = false;
        }
    }

    /// Get mutable reference to the last tool call in the active turn.
    fn last_tool_mut(&mut self) -> Option<&mut ToolCallItem> {
        if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut() {
            blocks.iter_mut().rev().find_map(|b| {
                if let TurnBlock::ToolCall(tool) = b {
                    Some(tool)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Find a tool call by ID in the active turn.
    fn find_tool_mut(&mut self, tool_use_id: &str) -> Option<&mut ToolCallItem> {
        if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut() {
            blocks.iter_mut().find_map(|b| {
                if let TurnBlock::ToolCall(tool) = b {
                    if tool.id == tool_use_id {
                        return Some(tool);
                    }
                }
                None
            })
        } else {
            None
        }
    }

    /// Get mutable reference to the last thinking block's content.
    fn last_thinking_mut(&mut self) -> Option<&mut String> {
        if let Some(ConversationItem::AssistantTurn { blocks, .. }) = self.items.last_mut() {
            blocks.iter_mut().rev().find_map(|b| {
                if let TurnBlock::Thinking { content, .. } = b {
                    Some(content)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Set a status message with auto-clear timestamp.
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_message_at = Some(Instant::now());
    }

    /// Clear expired status messages (older than 5 seconds).
    pub fn tick_status_timeout(&mut self) {
        if let Some(at) = self.status_message_at {
            if at.elapsed() >= Duration::from_secs(5) {
                self.status_message = None;
                self.status_message_at = None;
            }
        }
    }

    /// Render conversation as plain text for transcript dump.
    pub fn conversation_as_text(&self) -> String {
        let mut output = String::new();
        for item in &self.items {
            match item {
                ConversationItem::UserMessage { text } => {
                    output.push_str("You: ");
                    output.push_str(text);
                    output.push_str("\n\n");
                }
                ConversationItem::AssistantTurn { blocks, .. } => {
                    for block in blocks {
                        match block {
                            TurnBlock::Text { content, .. } => {
                                output.push_str(content);
                                output.push('\n');
                            }
                            TurnBlock::ToolCall(tool) => {
                                output.push_str(&format!(
                                    "  [{} {}]\n",
                                    match &tool.status {
                                        ToolStatus::Complete { .. } => "✓",
                                        ToolStatus::Error { .. } => "✗",
                                        _ => "⟳",
                                    },
                                    tool.name
                                ));
                                if !tool.result_preview.is_empty() {
                                    output.push_str(&format!(
                                        "    {}\n",
                                        tool.result_preview.lines().next().unwrap_or("")
                                    ));
                                }
                            }
                            TurnBlock::Thinking { content, .. } => {
                                if !content.is_empty() {
                                    output.push_str(&format!(
                                        "  [Thinking: {} chars]\n",
                                        content.len()
                                    ));
                                }
                            }
                        }
                    }
                    output.push('\n');
                }
                ConversationItem::SystemNotice { text } => {
                    output.push_str("System: ");
                    output.push_str(text);
                    output.push_str("\n\n");
                }
            }
        }
        output
    }

    /// Toggle is_expanded on the most recent completed tool call.
    pub fn toggle_last_tool_expand(&mut self) {
        // Search backward through all turns for the last completed tool
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantTurn { blocks, .. } = item {
                for block in blocks.iter_mut().rev() {
                    if let TurnBlock::ToolCall(tool) = block {
                        if matches!(tool.status, ToolStatus::Complete { .. }) {
                            tool.is_expanded = !tool.is_expanded;
                            return;
                        }
                    }
                }
            }
        }
    }
}

// ── Rendering ────────────────────────────────────────────────────────────

/// Render the conversation viewport.
pub fn render_conversation(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for item in &state.items {
        match item {
            ConversationItem::UserMessage { text } => {
                let style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                lines.push(Line::from(Span::styled("You: ", style)));
                for text_line in text.lines() {
                    lines.push(Line::from(Span::styled(text_line.to_string(), style)));
                }
                lines.push(Line::from(""));
            }
            ConversationItem::AssistantTurn { blocks, is_active } => {
                for block in blocks {
                    match block {
                        TurnBlock::Text {
                            content,
                            is_streaming,
                        } => {
                            if *is_streaming {
                                // Streaming text — render with markdown for
                                // partial formatting (code blocks, headers)
                                lines.extend(super::markdown::render_markdown_safe(content));
                                lines.push(Line::from(Span::styled(
                                    "▌",
                                    Style::default().fg(Color::Green),
                                )));
                            } else {
                                // Committed text — full markdown rendering
                                lines.extend(super::markdown::render_markdown_safe(content));
                            }
                        }
                        TurnBlock::ToolCall(tool) => {
                            render_tool_call_line(&mut lines, tool, state.frame_count);
                        }
                        TurnBlock::Thinking {
                            content,
                            is_streaming,
                        } => {
                            if state.show_thinking {
                                let style = Style::default()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::ITALIC);
                                lines.push(Line::from(Span::styled(
                                    "┌─ Thinking ─",
                                    Style::default().fg(Color::DarkGray),
                                )));
                                for text_line in content.lines().take(20) {
                                    lines.push(Line::from(Span::styled(
                                        format!("│ {text_line}"),
                                        style,
                                    )));
                                }
                                if content.lines().count() > 20 {
                                    lines.push(Line::from(Span::styled(
                                        format!(
                                            "│ [... {} more lines]",
                                            content.lines().count() - 20
                                        ),
                                        style,
                                    )));
                                }
                                if *is_streaming {
                                    lines.push(Line::from(Span::styled("│ ▌", style)));
                                }
                                lines.push(Line::from(Span::styled(
                                    "└─────────────",
                                    Style::default().fg(Color::DarkGray),
                                )));
                            } else if !content.is_empty() {
                                let char_count = if content.len() >= 1000 {
                                    format!("{:.1}k", content.len() as f64 / 1000.0)
                                } else {
                                    format!("{}", content.len())
                                };
                                if *is_streaming {
                                    let spinner =
                                        state.status.spinner(state.frame_count).unwrap_or("⠋");
                                    lines.push(Line::from(Span::styled(
                                        format!("  {spinner} Thinking ({char_count} chars)..."),
                                        Style::default().fg(Color::DarkGray),
                                    )));
                                } else {
                                    lines.push(Line::from(Span::styled(
                                        format!("  ▸ Thinking ({char_count} chars)"),
                                        Style::default().fg(Color::DarkGray),
                                    )));
                                }
                            } else if *is_streaming {
                                // Thinking just started, no content yet
                                let spinner =
                                    state.status.spinner(state.frame_count).unwrap_or("⠋");
                                lines.push(Line::from(Span::styled(
                                    format!("  {spinner} Thinking..."),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                }
                // Thinking indicator when turn is active but no content yet
                if *is_active && blocks.is_empty() && state.status == AppStatus::Thinking {
                    let spinner = state.status.spinner(state.frame_count).unwrap_or("⠋");
                    lines.push(Line::from(Span::styled(
                        format!("{spinner} Thinking..."),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
                if !is_active {
                    lines.push(Line::from(""));
                }
            }
            ConversationItem::SystemNotice { text } => {
                lines.push(Line::from(Span::styled(
                    text.clone(),
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(""));
            }
        }
    }

    let text = Text::from(lines);
    let content_height = text.height() as u16;
    state.scroll.set_viewport_height(area.height);
    state.scroll.set_content_height(content_height);

    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll.offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render a tool call block in the conversation.
fn render_tool_call_line(lines: &mut Vec<Line>, tool: &ToolCallItem, frame_count: u64) {
    let elapsed = tool.started_at.elapsed().as_secs_f64();

    match &tool.status {
        ToolStatus::InputStreaming => {
            let spinner = AppStatus::ToolRunning.spinner(frame_count).unwrap_or("⟳");
            let preview: String = tool.input_json.chars().take(60).collect();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {spinner} {} ", tool.name),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("[{elapsed:.1}s] "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{preview}▌"), Style::default().fg(Color::DarkGray)),
            ]));
        }
        ToolStatus::Running => {
            let spinner = AppStatus::ToolRunning.spinner(frame_count).unwrap_or("⟳");
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {spinner} {} ", tool.name),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("[{elapsed:.1}s]"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        ToolStatus::Complete { elapsed_secs } => {
            // Header line with status
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  ✓ {} ", tool.name),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!("[{elapsed_secs:.1}s]"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            // Rich tool call rendering (diff, file content, command output)
            let detail_lines = super::diff::render_tool_call(tool);
            lines.extend(detail_lines);
        }
        ToolStatus::Error { message } => {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  ✗ {} ", tool.name),
                    Style::default().fg(Color::Red),
                ),
                Span::styled(
                    format!("[{elapsed:.1}s] "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(message.clone(), Style::default().fg(Color::Red)),
            ]));
        }
    }
}

/// Render the input area.
pub fn render_input(frame: &mut Frame, state: &AppState, area: Rect) {
    let placeholder = match state.status {
        AppStatus::Connecting | AppStatus::Ready => "Type a message...",
        AppStatus::Thinking | AppStatus::Streaming | AppStatus::ToolRunning => "Waiting...",
        AppStatus::Error => "Error — press Ctrl+C to exit",
    };

    let display_text = if state.input.buffer.is_empty() {
        Span::styled(placeholder, Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(
            state.input.buffer.as_str(),
            Style::default().fg(Color::White),
        )
    };

    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Green)),
        display_text,
    ]))
    .block(Block::default().borders(Borders::TOP | Borders::BOTTOM))
    .wrap(Wrap { trim: false });

    frame.render_widget(input, area);

    // Position cursor accounting for text wrapping.
    // No left/right borders — inner width equals area width.
    // +2 for "> " prefix.
    let inner_width = area.width;
    let capped_cursor = state.input.cursor.min(u16::MAX as usize - 2) as u16;
    let char_pos = capped_cursor + 2; // +2 for "> "
    let (cursor_x, cursor_y) = if inner_width == 0 {
        (area.x, area.y + 1)
    } else {
        let line_num = char_pos / inner_width;
        let col = char_pos % inner_width;
        (area.x + col, area.y + 1 + line_num) // +1 for top border
    };
    frame.set_cursor_position((
        cursor_x.min(area.x + area.width - 1),
        cursor_y.min(area.y + area.height - 1),
    ));
}

/// Render the status bar.
pub fn render_status(frame: &mut Frame, state: &AppState, area: Rect) {
    let m = state
        .metrics
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut parts: Vec<Span> = Vec::new();

    // Status indicator
    if let Some(spinner) = state.status.spinner(state.frame_count) {
        parts.push(Span::styled(
            format!("{spinner} "),
            Style::default().fg(Color::Yellow),
        ));
    }

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

    // Permission mode
    parts.push(Span::raw(" │ "));
    parts.push(Span::styled(
        format!("[{}]", state.permission_mode.label()),
        Style::default().fg(state.permission_mode.color()),
    ));

    // Active tool
    if let Some(tool) = &m.active_tool {
        parts.push(Span::raw(" │ "));
        parts.push(Span::styled(
            format!("⟳ {tool}"),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Thinking chars
    if m.thinking_chars > 0 {
        parts.push(Span::raw(" │ "));
        let label = if m.thinking_chars >= 1000 {
            format!("think:{:.1}k", m.thinking_chars as f64 / 1000.0)
        } else {
            format!("think:{}", m.thinking_chars)
        };
        parts.push(Span::styled(label, Style::default().fg(Color::DarkGray)));
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

/// Render the permission prompt overlay above the input area.
pub fn render_permission_prompt(frame: &mut Frame, prompt: &PermissionPrompt, area: Rect) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  Tool: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                prompt.tool.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!("  {}", prompt.description),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [a] Allow  ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[d] Deny",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Permission Required ")
        .title_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
