//! Extended NDJSON event parsing for the session bridge.
//!
//! Provides `BridgeParser`, a stateful parser that handles the full Claude Code
//! streaming protocol: content blocks, tool calls, thinking blocks, system init,
//! rate limits, and permission hook events.
//!
//! This module is TUI-agnostic — no ratatui types. Both human TUI and future
//! marvel diagnostic view consume these events.

use crate::protocol::{self, ClaudeEvent};

/// Extended event type for the session bridge.
#[derive(Debug)]
#[non_exhaustive]
pub enum BridgeEvent {
    /// Core event from the existing protocol parser.
    Core(ClaudeEvent),

    /// Session initialization with full metadata from system/init.
    SessionInit {
        session_id: String,
        permission_mode: String,
        available_slash_commands: Vec<String>,
        context_window_size: u64,
        model: String,
        version: String,
    },

    /// Assistant message started.
    MessageStart,

    /// Assistant message completed (stop_reason, final usage).
    MessageStop { stop_reason: String },

    /// Streaming text chunk from `content_block_delta(text_delta)`.
    TextDelta { text: String },

    /// Tool call started — content_block_start(tool_use).
    ToolCallStart { id: String, name: String },

    /// Streaming tool input JSON fragment.
    ToolInputDelta { partial_json: String },

    /// Tool call block completed — content_block_stop for a tool.
    ToolCallStop,

    /// Tool result from a `user` type event containing tool_result blocks.
    ToolResult {
        tool_use_id: String,
        content: String,
    },

    /// Thinking block started.
    ThinkingStart,

    /// Streaming thinking text chunk.
    ThinkingDelta { text: String },

    /// Thinking block completed.
    ThinkingStop,

    /// Rate limit status change.
    RateLimit {
        status: String,
        resets_at: Option<String>,
    },

    /// Permission request from Claude Code hook event.
    PermissionRequest { tool: String, description: String },
}

/// Aggregated session metrics, readable by any consumer.
#[derive(Debug, Default, Clone)]
pub struct SessionMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub context_pct: f64,
    pub num_turns: u64,
    pub tool_use_count: u64,
    pub thinking_chars: u64,
    pub active_tool: Option<String>,
    pub rate_limit_status: Option<String>,
    pub model: String,
    pub session_id: Option<String>,
    pub context_window_size: u64,
    pub permission_mode: String,
    pub available_slash_commands: Vec<String>,
}

impl SessionMetrics {
    /// Update context window usage percentage.
    pub fn update_context_pct(&mut self) {
        let window = if self.context_window_size > 0 {
            self.context_window_size
        } else {
            200_000 // fallback
        };
        self.context_pct = (self.input_tokens as f64 / window as f64) * 100.0;
    }
}

/// Stateful NDJSON parser for the Claude Code streaming protocol.
///
/// Tracks open content blocks across events because `content_block_stop`
/// does not carry the block type or tool_use_id — the parser must correlate
/// by remembering what's currently open.
#[derive(Debug, Default)]
pub struct BridgeParser {
    /// Currently open tool call: (tool_use_id, tool_name).
    pending_tool: Option<(String, String)>,
    /// Whether a thinking block is currently open.
    pending_thinking: bool,
}

impl BridgeParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a single NDJSON line into a `BridgeEvent`.
    pub fn parse(&mut self, line: &str) -> Option<BridgeEvent> {
        if line.is_empty() {
            return None;
        }

        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let event_type = v.get("type")?.as_str()?;

        match event_type {
            "system" => self.parse_system(&v),
            "message_start" => Some(BridgeEvent::MessageStart),
            "message_stop" => Some(BridgeEvent::MessageStop {
                stop_reason: String::new(),
            }),
            "message_delta" => {
                let stop_reason = v
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BridgeEvent::MessageStop { stop_reason })
            }
            "content_block_start" => self.parse_content_block_start(&v),
            "content_block_delta" => self.parse_content_block_delta(&v),
            "content_block_stop" => self.parse_content_block_stop(),
            "rate_limit_event" => {
                let status = v
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let resets_at = v
                    .get("resets_at")
                    .and_then(|s| s.as_str())
                    .map(String::from);
                Some(BridgeEvent::RateLimit { status, resets_at })
            }
            "hook_event" => self.parse_hook_event(&v),
            "user" => Self::parse_user_event(&v),
            "assistant" | "result" => protocol::parse_event(line).map(BridgeEvent::Core),
            "ping" => None,
            _ => protocol::parse_event(line).map(BridgeEvent::Core),
        }
    }

    fn parse_system(&self, v: &serde_json::Value) -> Option<BridgeEvent> {
        let subtype = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");

        if subtype == "init" {
            let session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let permission_mode = v
                .get("permission_mode")
                .and_then(|s| s.as_str())
                .unwrap_or("default")
                .to_string();
            let model = v
                .get("model")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let version = v
                .get("version")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let context_window_size = v
                .get("context_window_size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(200_000);
            let available_slash_commands = v
                .get("available_slash_commands")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            return Some(BridgeEvent::SessionInit {
                session_id,
                permission_mode,
                available_slash_commands,
                context_window_size,
                model,
                version,
            });
        }

        // Fall through to core parser for other system subtypes (api_retry, etc.)
        let session_id = v
            .get("session_id")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        Some(BridgeEvent::Core(ClaudeEvent::System { session_id }))
    }

    fn parse_content_block_start(&mut self, v: &serde_json::Value) -> Option<BridgeEvent> {
        let block = v.get("content_block")?;
        let block_type = block.get("type")?.as_str()?;

        match block_type {
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                self.pending_tool = Some((id.clone(), name.clone()));
                Some(BridgeEvent::ToolCallStart { id, name })
            }
            "thinking" => {
                self.pending_thinking = true;
                Some(BridgeEvent::ThinkingStart)
            }
            "text" => None, // text block start carries no useful data
            _ => None,
        }
    }

    fn parse_content_block_delta(&self, v: &serde_json::Value) -> Option<BridgeEvent> {
        let delta = v.get("delta")?;
        let delta_type = delta.get("type")?.as_str()?;

        match delta_type {
            "text_delta" => {
                let text = delta.get("text")?.as_str()?.to_string();
                Some(BridgeEvent::TextDelta { text })
            }
            "input_json_delta" => {
                let partial = delta
                    .get("partial_json")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BridgeEvent::ToolInputDelta {
                    partial_json: partial,
                })
            }
            "thinking_delta" => {
                let text = delta
                    .get("thinking")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BridgeEvent::ThinkingDelta { text })
            }
            _ => None,
        }
    }

    fn parse_content_block_stop(&mut self) -> Option<BridgeEvent> {
        if self.pending_tool.take().is_some() {
            return Some(BridgeEvent::ToolCallStop);
        }
        if self.pending_thinking {
            self.pending_thinking = false;
            return Some(BridgeEvent::ThinkingStop);
        }
        // Text block stop — no event needed
        None
    }

    fn parse_hook_event(&self, v: &serde_json::Value) -> Option<BridgeEvent> {
        let subtype = v.get("subtype").and_then(|s| s.as_str())?;
        if subtype != "PermissionRequest" {
            return None;
        }
        let tool = v
            .get("tool_name")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let description = v
            .get("tool_input")
            .map(|input| {
                // Try to get a readable description from tool input
                if let Some(cmd) = input.get("command").and_then(|s| s.as_str()) {
                    cmd.to_string()
                } else if let Some(path) = input.get("file_path").and_then(|s| s.as_str()) {
                    path.to_string()
                } else {
                    serde_json::to_string(input).unwrap_or_default()
                }
            })
            .unwrap_or_default();
        Some(BridgeEvent::PermissionRequest { tool, description })
    }

    fn parse_user_event(v: &serde_json::Value) -> Option<BridgeEvent> {
        let message = v.get("message")?;
        let content = message.get("content")?.as_array()?;
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                let tool_use_id = block
                    .get("tool_use_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let content_text = if let Some(s) = block.get("content").and_then(|c| c.as_str()) {
                    s.to_string()
                } else if let Some(arr) = block.get("content").and_then(|c| c.as_array()) {
                    arr.iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                b.get("text").and_then(|t| t.as_str()).map(String::from)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };
                return Some(BridgeEvent::ToolResult {
                    tool_use_id,
                    content: content_text,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> BridgeParser {
        BridgeParser::new()
    }

    #[test]
    fn parse_text_delta() {
        let mut p = parser();
        let line = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        match p.parse(line) {
            Some(BridgeEvent::TextDelta { text }) => assert_eq!(text, "Hello"),
            other => panic!("expected TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_call_lifecycle() {
        let mut p = parser();

        // Start
        let line = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_123","name":"Edit","input":{}}}"#;
        match p.parse(line) {
            Some(BridgeEvent::ToolCallStart { id, name }) => {
                assert_eq!(id, "tool_123");
                assert_eq!(name, "Edit");
            }
            other => panic!("expected ToolCallStart, got {other:?}"),
        }
        assert!(p.pending_tool.is_some());

        // Input delta
        let line = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"src/"}}"#;
        match p.parse(line) {
            Some(BridgeEvent::ToolInputDelta { partial_json }) => {
                assert_eq!(partial_json, r#"{"path":"src/"#);
            }
            other => panic!("expected ToolInputDelta, got {other:?}"),
        }

        // Stop
        let line = r#"{"type":"content_block_stop","index":1}"#;
        match p.parse(line) {
            Some(BridgeEvent::ToolCallStop) => {}
            other => panic!("expected ToolCallStop, got {other:?}"),
        }
        assert!(p.pending_tool.is_none());
    }

    #[test]
    fn parse_thinking_lifecycle() {
        let mut p = parser();

        let line = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;
        assert!(matches!(p.parse(line), Some(BridgeEvent::ThinkingStart)));
        assert!(p.pending_thinking);

        let line = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#;
        match p.parse(line) {
            Some(BridgeEvent::ThinkingDelta { text }) => {
                assert_eq!(text, "Let me think...");
            }
            other => panic!("expected ThinkingDelta, got {other:?}"),
        }

        let line = r#"{"type":"content_block_stop","index":0}"#;
        assert!(matches!(p.parse(line), Some(BridgeEvent::ThinkingStop)));
        assert!(!p.pending_thinking);
    }

    #[test]
    fn parse_session_init() {
        let mut p = parser();
        let line = r#"{"type":"system","subtype":"init","session_id":"sess-1","permission_mode":"default","model":"claude-sonnet-4-6","version":"1.0","context_window_size":200000,"available_slash_commands":["/help","/clear"]}"#;
        match p.parse(line) {
            Some(BridgeEvent::SessionInit {
                session_id,
                permission_mode,
                model,
                context_window_size,
                available_slash_commands,
                ..
            }) => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(permission_mode, "default");
                assert_eq!(model, "claude-sonnet-4-6");
                assert_eq!(context_window_size, 200_000);
                assert_eq!(available_slash_commands, vec!["/help", "/clear"]);
            }
            other => panic!("expected SessionInit, got {other:?}"),
        }
    }

    #[test]
    fn parse_rate_limit() {
        let mut p = parser();
        let line = r#"{"type":"rate_limit_event","status":"rate_limited","resets_at":"2026-04-10T12:00:00Z"}"#;
        match p.parse(line) {
            Some(BridgeEvent::RateLimit { status, resets_at }) => {
                assert_eq!(status, "rate_limited");
                assert_eq!(resets_at.as_deref(), Some("2026-04-10T12:00:00Z"));
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_result() {
        let mut p = parser();
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"abc","content":"result text"}]}}"#;
        match p.parse(line) {
            Some(BridgeEvent::ToolResult {
                tool_use_id,
                content,
            }) => {
                assert_eq!(tool_use_id, "abc");
                assert_eq!(content, "result text");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_start_stop() {
        let mut p = parser();
        assert!(matches!(
            p.parse(r#"{"type":"message_start","message":{"id":"msg_1","role":"assistant"}}"#),
            Some(BridgeEvent::MessageStart)
        ));
        assert!(matches!(
            p.parse(r#"{"type":"message_stop"}"#),
            Some(BridgeEvent::MessageStop { .. })
        ));
    }

    #[test]
    fn parse_message_delta_with_stop_reason() {
        let mut p = parser();
        let line = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#;
        match p.parse(line) {
            Some(BridgeEvent::MessageStop { stop_reason }) => {
                assert_eq!(stop_reason, "end_turn");
            }
            other => panic!("expected MessageStop, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_line_returns_none() {
        assert!(parser().parse("").is_none());
    }

    #[test]
    fn parse_invalid_json_returns_none() {
        assert!(parser().parse("not json").is_none());
    }

    #[test]
    fn parse_ping_returns_none() {
        assert!(parser().parse(r#"{"type":"ping"}"#).is_none());
    }

    #[test]
    fn content_block_stop_without_pending_returns_none() {
        let mut p = parser();
        // No pending tool or thinking — text block stop
        assert!(
            p.parse(r#"{"type":"content_block_stop","index":0}"#)
                .is_none()
        );
    }

    #[test]
    fn session_metrics_context_pct_uses_actual_window() {
        let mut m = SessionMetrics {
            input_tokens: 50_000,
            context_window_size: 100_000,
            ..SessionMetrics::default()
        };
        m.update_context_pct();
        assert!((m.context_pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn session_metrics_context_pct_fallback() {
        let mut m = SessionMetrics {
            input_tokens: 100_000,
            context_window_size: 0, // unset
            ..SessionMetrics::default()
        };
        m.update_context_pct();
        assert!((m.context_pct - 50.0).abs() < 0.01); // falls back to 200k
    }
}
