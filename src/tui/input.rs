//! Key handling, slash command parsing, and input history for the TUI.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
#[derive(Debug)]
pub enum SlashCmd {
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

/// Handle a key event against the current input buffer and history.
///
/// Modifies `input_buffer` in place (for character input, backspace, history).
/// Returns an `InputAction` describing what the TUI should do.
pub fn handle_key(
    event: KeyEvent,
    input_buffer: &mut String,
    history: &mut InputHistory,
) -> InputAction {
    match (event.modifiers, event.code) {
        // Quit
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => InputAction::Quit,

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
    fn history_prev_cycles_backward() {
        let mut h = InputHistory::new();
        h.push("first".to_string());
        h.push("second".to_string());
        h.push("third".to_string());

        assert_eq!(h.prev("current"), Some("third"));
        assert_eq!(h.prev("current"), Some("second"));
        assert_eq!(h.prev("current"), Some("first"));
        // At oldest — stays
        assert_eq!(h.prev("current"), Some("first"));
    }

    #[test]
    fn history_next_cycles_forward_and_restores_draft() {
        let mut h = InputHistory::new();
        h.push("first".to_string());
        h.push("second".to_string());

        // Browse back
        h.prev("my draft");
        h.prev("my draft");

        // Browse forward
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
