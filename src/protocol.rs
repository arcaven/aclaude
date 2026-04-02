use serde::Deserialize;

/// Token usage from an assistant message.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

/// A content block within a message.
#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub id: Option<String>,
}

/// The message body within an assistant or user event.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageBody {
    pub role: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    pub usage: Option<TokenUsage>,
}

/// Result event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ResultPayload {
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub num_turns: u64,
    #[serde(default)]
    pub is_error: bool,
    pub session_id: Option<String>,
    pub result: Option<String>,
}

/// Parsed NDJSON event from the claude CLI.
#[derive(Debug)]
pub enum ClaudeEvent {
    System { session_id: String },
    Assistant { message: MessageBody },
    Result { payload: ResultPayload },
    Unknown { event_type: String },
}

/// Parse a single NDJSON line from claude CLI output.
pub fn parse_event(line: &str) -> Option<ClaudeEvent> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    match event_type {
        "system" => {
            let session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            Some(ClaudeEvent::System { session_id })
        }
        "assistant" => {
            let message: MessageBody = serde_json::from_value(
                v.get("message").cloned().unwrap_or(serde_json::Value::Null),
            )
            .ok()?;
            Some(ClaudeEvent::Assistant { message })
        }
        "result" => {
            let payload: ResultPayload = serde_json::from_value(v.clone()).ok()?;
            Some(ClaudeEvent::Result { payload })
        }
        other => Some(ClaudeEvent::Unknown {
            event_type: other.to_string(),
        }),
    }
}

/// Aggregated session usage across all turns.
#[derive(Debug, Default)]
pub struct SessionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub num_turns: u64,
    pub duration_ms: u64,
    pub tool_uses: Vec<String>,
}

impl SessionUsage {
    pub fn add_turn(&mut self, usage: &TokenUsage) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_read_tokens += usage.cache_read_input_tokens;
        self.cache_creation_tokens += usage.cache_creation_input_tokens;
    }

    pub fn set_result(&mut self, result: &ResultPayload) {
        self.cost_usd = result.cost_usd;
        self.num_turns = result.num_turns;
        self.duration_ms = result.duration_ms;
    }

    /// Estimate context window usage percentage.
    /// Uses input tokens against a 200k default context window.
    pub fn context_pct(&self, context_window: u64) -> f64 {
        if context_window == 0 {
            return 0.0;
        }
        (self.input_tokens as f64 / context_window as f64) * 100.0
    }

    pub fn print_summary(&self) {
        println!();
        println!("Session summary:");
        println!(
            "  Tokens: {} in / {} out (cache: {} read, {} created)",
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cache_creation_tokens
        );
        if self.cost_usd > 0.0 {
            println!("  Cost: ${:.4}", self.cost_usd);
        }
        println!("  Turns: {}", self.num_turns);
        if self.duration_ms > 0 {
            println!("  Duration: {:.1}s", self.duration_ms as f64 / 1000.0);
        }
        if !self.tool_uses.is_empty() {
            println!("  Tool uses: {}", self.tool_uses.len());
        }
    }
}
