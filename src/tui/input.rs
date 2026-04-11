//! Key handling, slash command parsing, tab completion, and input history.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Static slash commands (always available, handled locally by aclaude).
const LOCAL_COMMANDS: &[&str] = &[
    "/clear",
    "/cost",
    "/exit",
    "/help",
    "/login",
    "/persona portrait size small",
    "/persona portrait size medium",
    "/persona portrait size large",
    "/persona portrait size original",
];

/// Action resulting from a key press.
#[derive(Debug)]
pub enum InputAction {
    /// Send the input buffer as a user message.
    SendMessage(String),
    /// Process a local slash command.
    SlashCommand(SlashCmd),
    /// Quit the TUI.
    Quit,
    /// Scroll conversation up one page.
    PageUp,
    /// Scroll conversation down one page.
    PageDown,
    /// Scroll to bottom of conversation.
    ScrollEnd,
    /// Toggle expanded output for most recent completed tool call.
    ToggleExpand,
    /// Cycle permission mode (Shift+Tab).
    CyclePermissionMode,
    /// Allow pending permission request.
    PermissionAllow,
    /// Deny pending permission request.
    PermissionDeny,
    /// No action (key consumed but no effect).
    None,
}

/// Parsed slash commands.
#[derive(Debug, PartialEq)]
pub enum SlashCmd {
    /// Exit the TUI.
    Exit,
    /// Show auth/login info.
    Login,
    /// Clear conversation display.
    Clear,
    /// Show available commands and keybindings.
    Help,
    /// Show session cost from metrics.
    Cost,
    /// Set portrait size.
    PortraitSize(String),
    /// Forward to Claude Code as a user message (handles /compact, /model, etc.).
    ForwardToAgent(String),
    /// Unknown slash command.
    Unknown(String),
}

/// Input history with up/down arrow cycling.
#[derive(Default)]
pub struct InputHistory {
    entries: Vec<String>,
    position: Option<usize>,
    draft: String,
}

impl InputHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: String) {
        if entry.is_empty() || self.entries.last().is_some_and(|last| last == &entry) {
            self.position = None;
            return;
        }
        self.entries.push(entry);
        self.position = None;
    }

    pub fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.position {
            None => {
                self.draft = current_input.to_string();
                let idx = self.entries.len() - 1;
                self.position = Some(idx);
                Some(&self.entries[idx])
            }
            Some(0) => Some(&self.entries[0]),
            Some(idx) => {
                let new_idx = idx - 1;
                self.position = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    pub fn newer(&mut self) -> NextResult<'_> {
        match self.position {
            None => NextResult::NotBrowsing,
            Some(idx) => {
                if idx + 1 >= self.entries.len() {
                    self.position = None;
                    NextResult::Draft(&self.draft)
                } else {
                    let new_idx = idx + 1;
                    self.position = Some(new_idx);
                    NextResult::Entry(&self.entries[new_idx])
                }
            }
        }
    }
}

pub enum NextResult<'a> {
    Entry(&'a str),
    Draft(&'a str),
    NotBrowsing,
}

/// Tab-complete slash commands or @ file paths.
///
/// For slash commands: merges static local commands with dynamic commands
/// from system/init. For @ file paths: completes from the current directory.
pub fn tab_complete(input_buffer: &mut String, dynamic_commands: &[String]) -> bool {
    // @ file path completion
    if let Some(at_pos) = input_buffer.rfind('@') {
        let partial = &input_buffer[at_pos + 1..];
        if let Some(completed) = complete_file_path(partial) {
            let prefix = input_buffer[..at_pos + 1].to_string();
            *input_buffer = format!("{prefix}{completed}");
            return true;
        }
        return false;
    }

    // Slash command completion
    if !input_buffer.starts_with('/') {
        return false;
    }

    let prefix = input_buffer.as_str();

    // Build combined command list: local + dynamic
    let mut all_commands: Vec<&str> = LOCAL_COMMANDS.to_vec();
    for cmd in dynamic_commands {
        if !all_commands.contains(&cmd.as_str()) {
            all_commands.push(cmd);
        }
    }

    let matches: Vec<&str> = all_commands
        .iter()
        .filter(|cmd| cmd.starts_with(prefix) && **cmd != prefix)
        .copied()
        .collect();

    match matches.len() {
        0 => false,
        1 => {
            *input_buffer = matches[0].to_string();
            true
        }
        _ => {
            let lcp = longest_common_prefix(&matches);
            if lcp.len() > input_buffer.len() {
                *input_buffer = lcp;
                true
            } else {
                false
            }
        }
    }
}

/// Complete a file path from the current directory.
fn complete_file_path(partial: &str) -> Option<String> {
    if partial.is_empty() {
        return None;
    }

    let (dir, file_prefix) = if let Some(sep) = partial.rfind('/') {
        (&partial[..=sep], &partial[sep + 1..])
    } else {
        (".", partial)
    };

    let dir_path = Path::new(dir);
    let entries = std::fs::read_dir(dir_path).ok()?;

    let mut matches: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(file_prefix) {
                let full = if dir == "." {
                    name
                } else {
                    format!("{dir}{name}")
                };
                // Append / for directories
                if entry.file_type().ok()?.is_dir() {
                    Some(format!("{full}/"))
                } else {
                    Some(full)
                }
            } else {
                None
            }
        })
        .collect();

    matches.sort();

    match matches.len() {
        0 => None,
        1 => Some(matches.into_iter().next().expect("checked len")),
        _ => {
            // Complete to longest common prefix
            let refs: Vec<&str> = matches.iter().map(String::as_str).collect();
            let lcp = longest_common_prefix(&refs);
            if lcp.len() > partial.len() {
                Some(lcp)
            } else {
                None
            }
        }
    }
}

fn longest_common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = first
            .chars()
            .zip(s.chars())
            .take(len)
            .take_while(|(a, b)| a == b)
            .count();
    }
    first[..first
        .char_indices()
        .nth(len)
        .map_or(first.len(), |(i, _)| i)]
        .to_string()
}

/// Handle a key event against the current input buffer and history.
pub fn handle_key(
    event: KeyEvent,
    input_buffer: &mut String,
    history: &mut InputHistory,
    has_permission_prompt: bool,
    dynamic_commands: &[String],
) -> InputAction {
    match (event.modifiers, event.code) {
        // Quit (always available)
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => InputAction::Quit,

        // Toggle expand tool output
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => InputAction::ToggleExpand,

        // Cycle permission mode (Shift+Tab)
        (KeyModifiers::SHIFT, KeyCode::BackTab) => InputAction::CyclePermissionMode,

        // Permission prompt keys (when active, intercept before normal input)
        (_, KeyCode::Char('a')) if has_permission_prompt => InputAction::PermissionAllow,
        (_, KeyCode::Char('d')) if has_permission_prompt => InputAction::PermissionDeny,

        // Tab completion (slash commands + @ file paths)
        (_, KeyCode::Tab) => {
            tab_complete(input_buffer, dynamic_commands);
            InputAction::None
        }

        // Submit
        (_, KeyCode::Enter) => {
            let text = input_buffer.trim().to_string();
            input_buffer.clear();
            if text.is_empty() {
                return InputAction::None;
            }
            history.push(text.clone());
            if let Some(cmd) = parse_slash_command(&text) {
                InputAction::SlashCommand(cmd)
            } else {
                InputAction::SendMessage(text)
            }
        }

        // Editing
        (_, KeyCode::Backspace) => {
            input_buffer.pop();
            InputAction::None
        }

        // History navigation
        (_, KeyCode::Up) => {
            if let Some(entry) = history.prev(input_buffer) {
                *input_buffer = entry.to_string();
            }
            InputAction::None
        }
        (_, KeyCode::Down) => {
            match history.newer() {
                NextResult::Entry(entry) => *input_buffer = entry.to_string(),
                NextResult::Draft(draft) => *input_buffer = draft.to_string(),
                NextResult::NotBrowsing => {}
            }
            InputAction::None
        }

        // Scrolling
        (_, KeyCode::PageUp) => InputAction::PageUp,
        (_, KeyCode::PageDown) => InputAction::PageDown,
        (_, KeyCode::End) => InputAction::ScrollEnd,
        (_, KeyCode::Home) => InputAction::ScrollEnd,

        // Character input
        (_, KeyCode::Char(c)) => {
            input_buffer.push(c);
            InputAction::None
        }

        _ => InputAction::None,
    }
}

/// Parse a slash command from input text.
fn parse_slash_command(text: &str) -> Option<SlashCmd> {
    if !text.starts_with('/') {
        return None;
    }

    let parts: Vec<&str> = text.split_whitespace().collect();
    let cmd = parts.first().copied().unwrap_or("");

    match cmd {
        "/exit" | "/quit" => return Some(SlashCmd::Exit),
        "/login" => return Some(SlashCmd::Login),
        "/clear" => return Some(SlashCmd::Clear),
        "/help" => return Some(SlashCmd::Help),
        "/cost" => return Some(SlashCmd::Cost),
        _ => {}
    }

    // /persona portrait size <size>
    if parts.len() == 4 && cmd == "/persona" && parts[1] == "portrait" && parts[2] == "size" {
        let size = parts[3].to_lowercase();
        if ["small", "medium", "large", "original"].contains(&size.as_str()) {
            return Some(SlashCmd::PortraitSize(size));
        }
    }

    // Known Claude Code commands — forward as user messages
    const FORWARD_COMMANDS: &[&str] = &[
        "/compact",
        "/model",
        "/init",
        "/review",
        "/bug",
        "/stats",
        "/doctor",
        "/config",
        "/permissions",
    ];
    if FORWARD_COMMANDS.contains(&cmd) {
        return Some(SlashCmd::ForwardToAgent(text.to_string()));
    }

    // Any other / command from system/init available_slash_commands
    // is also forwarded (MCP, skills, etc.)
    Some(SlashCmd::Unknown(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exit() {
        assert_eq!(parse_slash_command("/exit"), Some(SlashCmd::Exit));
    }

    #[test]
    fn parse_quit_alias() {
        assert_eq!(parse_slash_command("/quit"), Some(SlashCmd::Exit));
    }

    #[test]
    fn parse_login() {
        assert_eq!(parse_slash_command("/login"), Some(SlashCmd::Login));
    }

    #[test]
    fn parse_clear() {
        assert_eq!(parse_slash_command("/clear"), Some(SlashCmd::Clear));
    }

    #[test]
    fn parse_help() {
        assert_eq!(parse_slash_command("/help"), Some(SlashCmd::Help));
    }

    #[test]
    fn parse_cost() {
        assert_eq!(parse_slash_command("/cost"), Some(SlashCmd::Cost));
    }

    #[test]
    fn parse_forward_compact() {
        assert_eq!(
            parse_slash_command("/compact"),
            Some(SlashCmd::ForwardToAgent("/compact".to_string()))
        );
    }

    #[test]
    fn parse_portrait_size_command() {
        match parse_slash_command("/persona portrait size large") {
            Some(SlashCmd::PortraitSize(s)) => assert_eq!(s, "large"),
            other => panic!("expected PortraitSize, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_slash_command() {
        match parse_slash_command("/unknown") {
            Some(SlashCmd::Unknown(s)) => assert_eq!(s, "/unknown"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn non_slash_returns_none() {
        assert!(parse_slash_command("hello").is_none());
    }

    #[test]
    fn tab_complete_single_match() {
        let mut buf = "/ex".to_string();
        assert!(tab_complete(&mut buf, &[]));
        assert_eq!(buf, "/exit");
    }

    #[test]
    fn tab_complete_common_prefix() {
        let mut buf = "/persona portrait size ".to_string();
        assert!(!tab_complete(&mut buf, &[]));
    }

    #[test]
    fn tab_complete_partial_prefix() {
        let mut buf = "/per".to_string();
        assert!(tab_complete(&mut buf, &[]));
        assert_eq!(buf, "/persona portrait size ");
    }

    #[test]
    fn tab_complete_no_match() {
        let mut buf = "/xyz".to_string();
        assert!(!tab_complete(&mut buf, &[]));
        assert_eq!(buf, "/xyz");
    }

    #[test]
    fn tab_complete_non_slash_ignored() {
        let mut buf = "hello".to_string();
        assert!(!tab_complete(&mut buf, &[]));
        assert_eq!(buf, "hello");
    }

    #[test]
    fn tab_complete_exact_match_no_change() {
        let mut buf = "/exit".to_string();
        assert!(!tab_complete(&mut buf, &[]));
    }

    #[test]
    fn tab_complete_with_dynamic_commands() {
        let dynamic = vec!["/my-custom-cmd".to_string()];
        let mut buf = "/my".to_string();
        assert!(tab_complete(&mut buf, &dynamic));
        assert_eq!(buf, "/my-custom-cmd");
    }

    #[test]
    fn tab_complete_login() {
        let mut buf = "/lo".to_string();
        assert!(tab_complete(&mut buf, &[]));
        assert_eq!(buf, "/login");
    }

    #[test]
    fn history_prev_cycles_backward() {
        let mut h = InputHistory::new();
        h.push("first".to_string());
        h.push("second".to_string());
        h.push("third".to_string());

        assert_eq!(h.prev("current"), Some("third"));
        assert_eq!(h.prev("current"), Some("second"));
        assert_eq!(h.prev("current"), Some("first"));
        assert_eq!(h.prev("current"), Some("first"));
    }

    #[test]
    fn history_next_cycles_forward_and_restores_draft() {
        let mut h = InputHistory::new();
        h.push("first".to_string());
        h.push("second".to_string());

        h.prev("my draft");
        h.prev("my draft");

        match h.newer() {
            NextResult::Entry(s) => assert_eq!(s, "second"),
            _ => panic!("expected Entry"),
        }
        match h.newer() {
            NextResult::Draft(s) => assert_eq!(s, "my draft"),
            _ => panic!("expected Draft"),
        }
    }

    #[test]
    fn history_skips_duplicates() {
        let mut h = InputHistory::new();
        h.push("same".to_string());
        h.push("same".to_string());
        h.push("same".to_string());
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn history_skips_empty() {
        let mut h = InputHistory::new();
        h.push(String::new());
        assert!(h.entries.is_empty());
    }
}
