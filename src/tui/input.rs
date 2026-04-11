//! Key handling, slash command parsing, tab completion, and input history.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Known slash commands for tab completion.
const KNOWN_COMMANDS: &[&str] = &[
    "/exit",
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
    /// Set portrait size: /persona portrait size [small|medium|large|original]
    PortraitSize(String),
    /// Unknown slash command.
    Unknown(String),
}

/// Input history with up/down arrow cycling.
///
/// Stores previous inputs and allows browsing through them.
/// When browsing starts, the current draft is saved and restored
/// when the user cycles past the newest entry.
#[derive(Default)]
pub struct InputHistory {
    entries: Vec<String>,
    /// Current position in history. `None` means not browsing.
    position: Option<usize>,
    /// Saved draft from when browsing started.
    draft: String,
}

impl InputHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a submitted input.
    pub fn push(&mut self, entry: String) {
        // Don't store empty or duplicate-of-last entries
        if entry.is_empty() || self.entries.last().is_some_and(|last| last == &entry) {
            self.position = None;
            return;
        }
        self.entries.push(entry);
        self.position = None;
    }

    /// Navigate to an older entry (up arrow). Returns the text to display.
    pub fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.position {
            None => {
                // Start browsing — save current input as draft
                self.draft = current_input.to_string();
                let idx = self.entries.len() - 1;
                self.position = Some(idx);
                Some(&self.entries[idx])
            }
            Some(0) => {
                // Already at oldest — stay put
                Some(&self.entries[0])
            }
            Some(idx) => {
                let new_idx = idx - 1;
                self.position = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Navigate to a newer entry (down arrow). Returns the text to display,
    /// or `None` if we've cycled past the newest entry (restores draft).
    pub fn newer(&mut self) -> NextResult<'_> {
        match self.position {
            None => NextResult::NotBrowsing,
            Some(idx) => {
                if idx + 1 >= self.entries.len() {
                    // Past newest — restore draft
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

/// Result of navigating forward in history.
pub enum NextResult<'a> {
    /// An older entry.
    Entry(&'a str),
    /// Restored draft (cycled past newest).
    Draft(&'a str),
    /// Not currently browsing history.
    NotBrowsing,
}

/// Tab-complete a slash command.
///
/// If the input starts with `/`, finds matching commands from the known list.
/// - Single match: completes to that command.
/// - Multiple matches: completes to longest common prefix.
/// - No match: no change.
///
/// Returns true if the buffer was modified.
pub fn tab_complete(input_buffer: &mut String) -> bool {
    if !input_buffer.starts_with('/') {
        return false;
    }

    let prefix = input_buffer.as_str();
    let matches: Vec<&str> = KNOWN_COMMANDS
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
            // Complete to longest common prefix
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

/// Find the longest common prefix of a set of strings.
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
///
/// Modifies `input_buffer` in place (for character input, backspace, history, tab).
/// Returns an `InputAction` describing what the TUI should do.
pub fn handle_key(
    event: KeyEvent,
    input_buffer: &mut String,
    history: &mut InputHistory,
) -> InputAction {
    match (event.modifiers, event.code) {
        // Quit
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => InputAction::Quit,

        // Tab completion
        (_, KeyCode::Tab) => {
            tab_complete(input_buffer);
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

        // Scrolling (page-level only — up/down are for history)
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

    match parts.first().copied() {
        Some("/exit") => return Some(SlashCmd::Exit),
        Some("/login") => return Some(SlashCmd::Login),
        _ => {}
    }

    // /persona portrait size <size>
    if parts.len() == 4 && parts[0] == "/persona" && parts[1] == "portrait" && parts[2] == "size" {
        let size = parts[3].to_lowercase();
        if ["small", "medium", "large", "original"].contains(&size.as_str()) {
            return Some(SlashCmd::PortraitSize(size));
        }
    }

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
    fn parse_login() {
        assert_eq!(parse_slash_command("/login"), Some(SlashCmd::Login));
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
        assert!(tab_complete(&mut buf));
        assert_eq!(buf, "/exit");
    }

    #[test]
    fn tab_complete_common_prefix() {
        let mut buf = "/persona portrait size ".to_string();
        // All four size variants match — no common prefix beyond input
        assert!(!tab_complete(&mut buf));
    }

    #[test]
    fn tab_complete_partial_prefix() {
        let mut buf = "/per".to_string();
        assert!(tab_complete(&mut buf));
        assert_eq!(buf, "/persona portrait size ");
    }

    #[test]
    fn tab_complete_no_match() {
        let mut buf = "/xyz".to_string();
        assert!(!tab_complete(&mut buf));
        assert_eq!(buf, "/xyz");
    }

    #[test]
    fn tab_complete_non_slash_ignored() {
        let mut buf = "hello".to_string();
        assert!(!tab_complete(&mut buf));
        assert_eq!(buf, "hello");
    }

    #[test]
    fn tab_complete_exact_match_no_change() {
        let mut buf = "/exit".to_string();
        assert!(!tab_complete(&mut buf));
    }

    #[test]
    fn tab_complete_login() {
        let mut buf = "/lo".to_string();
        assert!(tab_complete(&mut buf));
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
