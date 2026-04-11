use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use crate::config::ForestageConfig;
use crate::error::{ForestageError, Result};
use crate::persona;
use crate::protocol::{self, ClaudeEvent, SessionUsage};
use crate::statusline;

/// Check that the `claude` CLI is available.
pub fn find_claude() -> Result<String> {
    let output = Command::new("claude")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(o) if o.status.success() => Ok("claude".to_string()),
        _ => Err(ForestageError::ClaudeNotFound),
    }
}

/// Start an interactive session with Claude Code.
///
/// Spawns `claude` with inherited stdio so the user gets the full Claude Code
/// TUI experience. The persona system prompt is injected via --append-system-prompt.
/// Any extra `claude_args` are passed through directly to the claude CLI.
pub fn start_session(config: &ForestageConfig, claude_args: &[String]) -> Result<()> {
    let claude_path = find_claude()?;

    let system_prompt = {
        let theme = persona::load_theme(&config.persona.theme)?;
        let agent = persona::get_agent(&theme, &config.persona.role)?;
        persona::build_system_prompt(&theme, agent, &config.persona.immersion)
    };

    let mut cmd = Command::new(&claude_path);
    cmd.args(["--model", &config.session.model])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if !system_prompt.is_empty() {
        cmd.args(["--append-system-prompt", &system_prompt]);
    }

    // Pass through any additional claude CLI arguments
    if !claude_args.is_empty() {
        cmd.args(claude_args);
    }

    let status = cmd.status().map_err(|e| ForestageError::Session {
        message: format!("failed to start claude: {e}"),
    })?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code != 0 {
            return Err(ForestageError::Session {
                message: format!("claude exited with code {code}"),
            });
        }
    }

    Ok(())
}

/// Start a programmatic session using the NDJSON streaming protocol.
///
/// Spawns `claude` with structured JSON I/O for programmatic access to
/// token usage, tool invocations, and session metadata. Used for agent
/// mode and non-interactive automation.
pub fn start_streaming_session(
    config: &ForestageConfig,
    claude_args: &[String],
) -> Result<SessionUsage> {
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
        .args(["--model", &config.session.model])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if !system_prompt.is_empty() {
        cmd.args(["--append-system-prompt", &system_prompt]);
    }

    if !claude_args.is_empty() {
        cmd.args(claude_args);
    }

    let mut child = cmd.spawn().map_err(|e| ForestageError::Session {
        message: format!("failed to start claude: {e}"),
    })?;

    let stdout = child.stdout.take().expect("stdout piped");
    // stdin kept alive for future interactive input support
    let _stdin = child.stdin.take().expect("stdin piped");

    let reader = BufReader::new(stdout);
    let mut usage = SessionUsage::default();
    let mut _session_id = String::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.is_empty() {
            continue;
        }

        let event = match protocol::parse_event(&line) {
            Some(e) => e,
            None => continue,
        };

        match event {
            ClaudeEvent::System { session_id: sid } => {
                _session_id = sid;
            }
            ClaudeEvent::Assistant { message } => {
                for block in &message.content {
                    if block.block_type == "text" {
                        if let Some(text) = &block.text {
                            print!("{text}");
                            std::io::stdout().flush().ok();
                        }
                    } else if block.block_type == "tool_use" {
                        if let Some(name) = &block.name {
                            usage.tool_uses.push(name.clone());
                        }
                    }
                }

                if let Some(u) = &message.usage {
                    usage.add_turn(u);

                    if config.statusline.enabled {
                        let theme = persona::load_theme(&config.persona.theme).ok();
                        let agent = theme
                            .as_ref()
                            .and_then(|t| persona::get_agent(t, &config.persona.role).ok());
                        let character_name = agent
                            .map(|a| a.character.clone())
                            .unwrap_or_else(|| "forestage".to_string());
                        let context_pct = usage.context_pct(200_000);
                        let left = statusline::render_statusline(
                            config,
                            &character_name,
                            Some(context_pct),
                        );
                        let right = statusline::build_progress_bar(context_pct, 10);
                        statusline::write_tmux_cache(&left, &right);
                    }
                }
            }
            ClaudeEvent::Result { payload } => {
                usage.set_result(&payload);
                break;
            }
            ClaudeEvent::Unknown { .. } => {}
        }
    }

    let _ = child.wait();

    Ok(usage)
}

/// Run a one-shot prompt (non-interactive).
pub fn run_prompt(
    config: &ForestageConfig,
    prompt: &str,
    output_format: &str,
    claude_args: &[String],
) -> Result<String> {
    let claude_path = find_claude()?;

    let system_prompt = {
        let theme = persona::load_theme(&config.persona.theme)?;
        let agent = persona::get_agent(&theme, &config.persona.role)?;
        persona::build_system_prompt(&theme, agent, &config.persona.immersion)
    };

    // Always request JSON from claude so we can extract the result field.
    // Pass through stream-json directly if requested.
    let claude_format = if output_format == "stream-json" {
        "stream-json"
    } else {
        "json"
    };

    let mut cmd = Command::new(&claude_path);
    cmd.args(["-p", prompt])
        .args(["--model", &config.session.model])
        .args(["--output-format", claude_format]);

    if !system_prompt.is_empty() {
        cmd.args(["--append-system-prompt", &system_prompt]);
    }

    if !claude_args.is_empty() {
        cmd.args(claude_args);
    }

    let output = cmd.output().map_err(|e| ForestageError::Session {
        message: format!("failed to run claude: {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ForestageError::Session {
            message: format!("claude error: {stderr}"),
        });
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    match output_format {
        "json" | "stream-json" => Ok(raw),
        _ => {
            // Extract the "result" field from the JSON response for human output
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(result) = parsed.get("result").and_then(|v| v.as_str()) {
                    return Ok(format!("{result}\n"));
                }
            }
            // Fallback: return raw if parsing fails
            Ok(raw)
        }
    }
}
