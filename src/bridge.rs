//! Session bridge — async subprocess lifecycle for Claude Code.
//!
//! Spawns `claude` as a headless subprocess with bidirectional NDJSON streaming.
//! Parses events via `BridgeParser`, updates `SessionMetrics`, and sends events
//! to consumers via an mpsc channel. The bridge is TUI-agnostic — no ratatui types.
//!
//! Both human TUI and future marvel diagnostic view consume the same bridge.
//! tmux statusline updates happen here so any consumer gets status for free.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::AclaudeConfig;
use crate::error::{AclaudeError, Result};
use crate::persona;
use crate::protocol::ClaudeEvent;
use crate::protocol_ext::{BridgeEvent, BridgeParser, SessionMetrics};
use crate::session::find_claude;
use crate::statusline;

/// A running Claude Code subprocess with event streaming.
pub struct Session {
    child: Child,
    event_rx: mpsc::Receiver<BridgeEvent>,
    metrics: Arc<Mutex<SessionMetrics>>,
    stdin_tx: mpsc::Sender<String>,
}

impl Session {
    /// Spawn a Claude Code subprocess with NDJSON streaming.
    pub async fn spawn(config: &AclaudeConfig) -> Result<Self> {
        let claude_path = find_claude()?;

        let system_prompt = {
            let theme = persona::load_theme(&config.persona.theme)?;
            let agent = persona::get_agent(&theme, &config.persona.role)?;
            persona::build_system_prompt(&theme, agent, &config.persona.immersion)
        };

        let mut cmd = Command::new(&claude_path);
        cmd.args(["--output-format", "stream-json"])
            .args(["--input-format", "stream-json"])
            .args(["--verbose"])
            .args(["--include-partial-messages"])
            .args(["--include-hook-events"])
            .args(["--model", &config.session.model])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        if !system_prompt.is_empty() {
            cmd.args(["--append-system-prompt", &system_prompt]);
        }

        let mut child = cmd.spawn().map_err(|e| AclaudeError::Session {
            message: format!("failed to start claude: {e}"),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| AclaudeError::Session {
            message: "failed to capture claude stdout".to_string(),
        })?;

        let child_stdin = child.stdin.take().ok_or_else(|| AclaudeError::Session {
            message: "failed to capture claude stdin".to_string(),
        })?;

        let metrics = Arc::new(Mutex::new(SessionMetrics {
            model: config.session.model.clone(),
            ..SessionMetrics::default()
        }));

        let (event_tx, event_rx) = mpsc::channel::<BridgeEvent>(256);
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        // Reader task: stdout → BridgeParser → metrics update → event channel
        let reader_metrics = Arc::clone(&metrics);
        let statusline_config = config.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = BridgeParser::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }
                if let Some(event) = parser.parse(&line) {
                    update_metrics(&reader_metrics, &event);
                    push_statusline_from_metrics(&reader_metrics, &statusline_config);

                    if event_tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Writer task: stdin channel → child stdin
        tokio::spawn(async move {
            let mut stdin = child_stdin;
            while let Some(msg) = stdin_rx.recv().await {
                if stdin.write_all(msg.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        Ok(Session {
            child,
            event_rx,
            metrics,
            stdin_tx,
        })
    }

    /// Get the event receiver for consuming bridge events.
    pub fn event_rx(&mut self) -> &mut mpsc::Receiver<BridgeEvent> {
        &mut self.event_rx
    }

    /// Get a shared reference to session metrics.
    pub fn metrics(&self) -> Arc<Mutex<SessionMetrics>> {
        Arc::clone(&self.metrics)
    }

    /// Send a user message to the Claude Code subprocess.
    pub async fn send_user_message(&self, text: &str) -> Result<()> {
        let msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": text
            }
        });
        let line = format!(
            "{}\n",
            serde_json::to_string(&msg).map_err(|e| {
                AclaudeError::Session {
                    message: format!("failed to serialize message: {e}"),
                }
            })?
        );
        self.stdin_tx
            .send(line)
            .await
            .map_err(|_| AclaudeError::Session {
                message: "subprocess stdin closed".to_string(),
            })
    }

    /// Send a permission response (allow/deny) to the subprocess.
    ///
    /// The hook event protocol expects a JSON response on stdin.
    pub async fn send_permission_response(&self, allowed: bool) -> Result<()> {
        let behavior = if allowed { "allow" } else { "deny" };
        let msg = serde_json::json!({
            "type": "permission_response",
            "permission_response": {
                "behavior": behavior
            }
        });
        let line = format!(
            "{}\n",
            serde_json::to_string(&msg).map_err(|e| {
                AclaudeError::Session {
                    message: format!("failed to serialize permission response: {e}"),
                }
            })?
        );
        self.stdin_tx
            .send(line)
            .await
            .map_err(|_| AclaudeError::Session {
                message: "subprocess stdin closed".to_string(),
            })
    }

    /// Gracefully shut down the subprocess.
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

/// Update `SessionMetrics` from a `BridgeEvent`.
fn update_metrics(metrics: &Arc<Mutex<SessionMetrics>>, event: &BridgeEvent) {
    let mut m = match metrics.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    match event {
        BridgeEvent::SessionInit {
            session_id,
            permission_mode,
            available_slash_commands,
            context_window_size,
            model,
            ..
        } => {
            m.session_id = Some(session_id.clone());
            m.permission_mode = permission_mode.clone();
            m.available_slash_commands = available_slash_commands.clone();
            m.context_window_size = *context_window_size;
            if !model.is_empty() {
                m.model = model.clone();
            }
        }
        BridgeEvent::Core(ClaudeEvent::System { session_id }) => {
            m.session_id = Some(session_id.clone());
        }
        BridgeEvent::Core(ClaudeEvent::Assistant { message }) => {
            if let Some(usage) = &message.usage {
                m.input_tokens += usage.input_tokens;
                m.output_tokens += usage.output_tokens;
                m.cache_read_tokens += usage.cache_read_input_tokens;
                m.cache_creation_tokens += usage.cache_creation_input_tokens;
                m.update_context_pct();
            }
        }
        BridgeEvent::Core(ClaudeEvent::Result { payload }) => {
            m.cost_usd = payload.cost_usd;
            m.num_turns = payload.num_turns;
            m.active_tool = None;
        }
        BridgeEvent::ToolCallStart { name, .. } => {
            m.tool_use_count += 1;
            m.active_tool = Some(name.clone());
        }
        BridgeEvent::ToolCallStop | BridgeEvent::ToolResult { .. } => {
            m.active_tool = None;
        }
        BridgeEvent::ThinkingDelta { text } => {
            m.thinking_chars += text.len() as u64;
        }
        BridgeEvent::RateLimit { status, .. } => {
            m.rate_limit_status = Some(status.clone());
        }
        _ => {}
    }
}

/// Push tmux statusline from current metrics.
fn push_statusline_from_metrics(metrics: &Arc<Mutex<SessionMetrics>>, config: &AclaudeConfig) {
    if !config.statusline.enabled {
        return;
    }
    let m = match metrics.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    if m.input_tokens == 0 {
        return;
    }

    let character_name = config
        .persona
        .theme
        .split('/')
        .next_back()
        .unwrap_or("aclaude");
    let left = statusline::render_statusline(config, character_name, Some(m.context_pct));
    let right = statusline::build_progress_bar(m.context_pct, 10);
    statusline::write_tmux_cache(&left, &right);
}
