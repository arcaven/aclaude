//! Key handling, slash command parsing, tab completion, and input history.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Static slash commands (always available, handled locally by forestage).
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
    "/persona portrait on",
    "/persona portrait off",
    "/persona portrait top",
    "/persona portrait bottom",
];

// ── InputState ───────────────────────────────────────────────────────────

/// Input buffer with cursor position for mid-line editing.
#[derive(Debug, Default)]
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
}

impl InputState {
    /// Insert a character at the cursor position.
    pub fn insert(&mut self, c: char) {
        let byte_pos = self.byte_offset();
        self.buffer.insert(byte_pos, c);
        self.cursor += 1;
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_back(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let byte_pos = self.byte_offset();
            self.buffer.remove(byte_pos);
        }
    }

    /// Delete from cursor back to the previous word boundary (Ctrl+W).
    pub fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut new_cursor = self.cursor;
        // Skip trailing whitespace
        while new_cursor > 0 && chars[new_cursor - 1].is_whitespace() {
            new_cursor -= 1;
        }
        // Skip word characters
        while new_cursor > 0 && !chars[new_cursor - 1].is_whitespace() {
            new_cursor -= 1;
        }
        let start_byte = self.char_to_byte(new_cursor);
        let end_byte = self.byte_offset();
        self.buffer.drain(start_byte..end_byte);
        self.cursor = new_cursor;
    }

    /// Clear the entire buffer (Ctrl+U).
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Move cursor to start of line (Ctrl+A / Home).
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line (Ctrl+E / End).
    pub fn end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }

    /// Move cursor left one character.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right one character.
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.chars().count() {
            self.cursor += 1;
        }
    }

    /// Set buffer content and move cursor to end.
    pub fn set(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor = self.buffer.chars().count();
    }

    /// Get the trimmed text content.
    pub fn text(&self) -> String {
        self.buffer.trim().to_string()
    }

    /// Byte offset for the current cursor position.
    fn byte_offset(&self) -> usize {
        self.char_to_byte(self.cursor)
    }

    /// Convert char index to byte index.
    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map_or(self.buffer.len(), |(i, _)| i)
    }
}

// ── Actions ──────────────────────────────────────────────────────────────

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
    /// Scroll conversation up one line (mouse wheel).
    ScrollUp,
    /// Scroll conversation down one line (mouse wheel).
    ScrollDown,
    /// Toggle expanded output for most recent completed tool call (Ctrl+X).
    ToggleExpand,
    /// Cycle transcript mode: normal → transcript → focus (Ctrl+O).
    CycleTranscript,
    /// Cycle permission mode (Shift+Tab).
    CyclePermissionMode,
    /// Allow pending permission request.
    PermissionAllow,
    /// Deny pending permission request.
    PermissionDeny,
    /// Toggle thinking block display (Alt+T).
    ToggleThinking,
    /// Open external editor for input (Ctrl+G).
    OpenEditor,
    /// Toggle portrait position top/bottom (Ctrl+P).
    PortraitTogglePosition,
    /// Toggle portrait on/off (Alt+P).
    PortraitToggleVisible,
    /// Cycle portrait size (Alt+S).
    PortraitCycleSize,
    /// No action (key consumed but no effect).
    None,
}

/// Parsed slash commands.
#[derive(Debug, PartialEq)]
pub enum SlashCmd {
    Exit,
    Login,
    Clear,
    Help,
    Cost,
    PortraitSize(String),
    PortraitToggle(bool),
    PortraitMove(String),
    ForwardToAgent(String),
    Unknown(String),
}

// ── History ──────────────────────────────────────────────────────────────

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

// ── Tab completion ───────────────────────────────────────────────────────

/// Tab-complete slash commands or @ file paths.
pub fn tab_complete(input: &mut InputState, dynamic_commands: &[String]) -> bool {
    // @ file path completion
    if let Some(at_pos) = input.buffer.rfind('@') {
        let partial = &input.buffer[at_pos + 1..];
        if let Some(completed) = complete_file_path(partial) {
            let prefix = input.buffer[..at_pos + 1].to_string();
            input.set(&format!("{prefix}{completed}"));
            return true;
        }
        return false;
    }

    // Slash command completion
    if !input.buffer.starts_with('/') {
        return false;
    }

    let prefix = input.buffer.as_str();
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
            input.set(matches[0]);
            true
        }
        _ => {
            let lcp = longest_common_prefix(&matches);
            if lcp.len() > input.buffer.len() {
                input.set(&lcp);
                true
            } else {
                false
            }
        }
    }
}

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

// ── Key handling ─────────────────────────────────────────────────────────

/// Handle a key event against the input state and history.
pub fn handle_key(
    event: KeyEvent,
    input: &mut InputState,
    history: &mut InputHistory,
    has_permission_prompt: bool,
    dynamic_commands: &[String],
) -> InputAction {
    match (event.modifiers, event.code) {
        // Quit (always available)
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => InputAction::Quit,

        // Ctrl shortcuts
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => InputAction::CycleTranscript,
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => InputAction::ToggleExpand,
        (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            input.home();
            InputAction::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            input.end();
            InputAction::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            input.delete_word_back();
            InputAction::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            input.clear();
            InputAction::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('g')) => InputAction::OpenEditor,

        // Portrait hotkeys
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => InputAction::PortraitTogglePosition,
        (KeyModifiers::ALT, KeyCode::Char('p')) => InputAction::PortraitToggleVisible,
        (KeyModifiers::ALT, KeyCode::Char('s')) => InputAction::PortraitCycleSize,

        // Alt shortcuts
        (KeyModifiers::ALT, KeyCode::Char('t')) => InputAction::ToggleThinking,

        // Cycle permission mode (Shift+Tab)
        (KeyModifiers::SHIFT, KeyCode::BackTab) => InputAction::CyclePermissionMode,

        // Permission prompt keys
        (_, KeyCode::Char('a')) if has_permission_prompt => InputAction::PermissionAllow,
        (_, KeyCode::Char('d')) if has_permission_prompt => InputAction::PermissionDeny,

        // Tab completion
        (_, KeyCode::Tab) => {
            tab_complete(input, dynamic_commands);
            InputAction::None
        }

        // Submit
        (_, KeyCode::Enter) => {
            let text = input.text();
            input.clear();
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
            input.delete_back();
            InputAction::None
        }
        (_, KeyCode::Delete) => {
            // Delete char under cursor (move right then backspace)
            if input.cursor < input.buffer.chars().count() {
                input.move_right();
                input.delete_back();
            }
            InputAction::None
        }
        (_, KeyCode::Left) => {
            input.move_left();
            InputAction::None
        }
        (_, KeyCode::Right) => {
            input.move_right();
            InputAction::None
        }

        // History navigation
        (_, KeyCode::Up) => {
            if let Some(entry) = history.prev(&input.buffer) {
                input.set(entry);
            }
            InputAction::None
        }
        (_, KeyCode::Down) => {
            match history.newer() {
                NextResult::Entry(entry) => input.set(entry),
                NextResult::Draft(draft) => input.set(draft),
                NextResult::NotBrowsing => {}
            }
            InputAction::None
        }

        // Scrolling
        (_, KeyCode::PageUp) => InputAction::PageUp,
        (_, KeyCode::PageDown) => InputAction::PageDown,
        (_, KeyCode::End) if input.buffer.is_empty() => InputAction::ScrollEnd,
        (_, KeyCode::Home) if input.buffer.is_empty() => InputAction::ScrollEnd,
        (_, KeyCode::End) => {
            input.end();
            InputAction::None
        }
        (_, KeyCode::Home) => {
            input.home();
            InputAction::None
        }

        // Character input
        (_, KeyCode::Char(c)) => {
            input.insert(c);
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

    // /persona portrait <subcommand>
    if parts.len() >= 3 && cmd == "/persona" && parts[1] == "portrait" {
        match parts[2] {
            "on" => return Some(SlashCmd::PortraitToggle(true)),
            "off" => return Some(SlashCmd::PortraitToggle(false)),
            "top" => return Some(SlashCmd::PortraitMove("top".to_string())),
            "bottom" => return Some(SlashCmd::PortraitMove("bottom".to_string())),
            "size" if parts.len() == 4 => {
                let size = parts[3].to_lowercase();
                if ["small", "medium", "large", "original"].contains(&size.as_str()) {
                    return Some(SlashCmd::PortraitSize(size));
                }
            }
            _ => {}
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

    Some(SlashCmd::Unknown(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── InputState tests ─────────────────────────────────────────────────

    #[test]
    fn input_insert_at_cursor() {
        let mut s = InputState::default();
        s.insert('a');
        s.insert('b');
        s.insert('c');
        assert_eq!(s.buffer, "abc");
        assert_eq!(s.cursor, 3);
    }

    #[test]
    fn input_insert_mid_buffer() {
        let mut s = InputState::default();
        s.set("ac");
        s.cursor = 1; // between a and c
        s.insert('b');
        assert_eq!(s.buffer, "abc");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn input_delete_back() {
        let mut s = InputState::default();
        s.set("abc");
        s.delete_back();
        assert_eq!(s.buffer, "ab");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn input_delete_back_at_start() {
        let mut s = InputState::default();
        s.set("abc");
        s.cursor = 0;
        s.delete_back();
        assert_eq!(s.buffer, "abc"); // no change
    }

    #[test]
    fn input_delete_word_back() {
        let mut s = InputState::default();
        s.set("hello world");
        s.delete_word_back();
        assert_eq!(s.buffer, "hello ");
        assert_eq!(s.cursor, 6);
    }

    #[test]
    fn input_delete_word_back_multiple_spaces() {
        let mut s = InputState::default();
        s.set("hello   world");
        s.cursor = 8; // in the spaces
        s.delete_word_back();
        assert_eq!(s.buffer, "world");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn input_home_end() {
        let mut s = InputState::default();
        s.set("hello");
        assert_eq!(s.cursor, 5);
        s.home();
        assert_eq!(s.cursor, 0);
        s.end();
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn input_move_left_right() {
        let mut s = InputState::default();
        s.set("abc");
        s.move_left();
        assert_eq!(s.cursor, 2);
        s.move_left();
        assert_eq!(s.cursor, 1);
        s.move_right();
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn input_clear() {
        let mut s = InputState::default();
        s.set("hello");
        s.clear();
        assert_eq!(s.buffer, "");
        assert_eq!(s.cursor, 0);
    }

    // ── Slash command tests ──────────────────────────────────────────────

    #[test]
    fn parse_exit() {
        assert_eq!(parse_slash_command("/exit"), Some(SlashCmd::Exit));
    }

    #[test]
    fn parse_quit_alias() {
        assert_eq!(parse_slash_command("/quit"), Some(SlashCmd::Exit));
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
    fn parse_portrait_size() {
        match parse_slash_command("/persona portrait size large") {
            Some(SlashCmd::PortraitSize(s)) => assert_eq!(s, "large"),
            other => panic!("expected PortraitSize, got {other:?}"),
        }
    }

    #[test]
    fn parse_portrait_toggle() {
        assert_eq!(
            parse_slash_command("/persona portrait on"),
            Some(SlashCmd::PortraitToggle(true))
        );
        assert_eq!(
            parse_slash_command("/persona portrait off"),
            Some(SlashCmd::PortraitToggle(false))
        );
    }

    #[test]
    fn parse_unknown() {
        match parse_slash_command("/unknown") {
            Some(SlashCmd::Unknown(s)) => assert_eq!(s, "/unknown"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn non_slash_returns_none() {
        assert!(parse_slash_command("hello").is_none());
    }

    // ── Tab completion tests ─────────────────────────────────────────────

    #[test]
    fn tab_complete_single_match() {
        let mut input = InputState::default();
        input.set("/ex");
        assert!(tab_complete(&mut input, &[]));
        assert_eq!(input.buffer, "/exit");
    }

    #[test]
    fn tab_complete_partial_prefix() {
        let mut input = InputState::default();
        input.set("/per");
        assert!(tab_complete(&mut input, &[]));
        assert_eq!(input.buffer, "/persona portrait ");
    }

    #[test]
    fn tab_complete_no_match() {
        let mut input = InputState::default();
        input.set("/xyz");
        assert!(!tab_complete(&mut input, &[]));
    }

    #[test]
    fn tab_complete_with_dynamic_commands() {
        let dynamic = vec!["/my-custom-cmd".to_string()];
        let mut input = InputState::default();
        input.set("/my");
        assert!(tab_complete(&mut input, &dynamic));
        assert_eq!(input.buffer, "/my-custom-cmd");
    }

    // ── History tests ────────────────────────────────────────────────────

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
    fn history_next_restores_draft() {
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
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn history_skips_empty() {
        let mut h = InputHistory::new();
        h.push(String::new());
        assert!(h.entries.is_empty());
    }
}
