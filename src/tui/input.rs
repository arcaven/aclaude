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

/// Input buffer with cursor position and optional selection.
#[derive(Debug, Default)]
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
    /// Selection anchor — when set, text between anchor and cursor is selected.
    pub selection_anchor: Option<usize>,
}

impl InputState {
    /// Insert a character at the cursor position.
    /// If there's a selection, replaces it with the character.
    pub fn insert(&mut self, c: char) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
        }
        let byte_pos = self.byte_offset();
        self.buffer.insert(byte_pos, c);
        self.cursor += 1;
    }

    /// Delete the character before the cursor (backspace).
    /// If there's a selection, deletes the selection instead.
    pub fn delete_back(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
            let byte_pos = self.byte_offset();
            self.buffer.remove(byte_pos);
        }
    }

    /// Delete from cursor back to the previous word boundary (Ctrl+W).
    pub fn delete_word_back(&mut self) {
        self.selection_anchor = None;
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
        self.selection_anchor = None;
    }

    /// Move cursor to start of line (Ctrl+A / Home).
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line (Ctrl+E / End).
    pub fn end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }

    /// Move cursor left one character. Clears selection.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right one character. Clears selection.
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.chars().count() {
            self.cursor += 1;
        }
    }

    /// Set buffer content and move cursor to end. Clears selection.
    pub fn set(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor = self.buffer.chars().count();
        self.selection_anchor = None;
    }

    /// Get the trimmed text content.
    pub fn text(&self) -> String {
        self.buffer.trim().to_string()
    }

    /// Return the selected text range (start, end) in char indices, if any.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor.map(|anchor| {
            if anchor <= self.cursor {
                (anchor, self.cursor)
            } else {
                (self.cursor, anchor)
            }
        })
    }

    /// Return the selected text, if any.
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let chars: Vec<char> = self.buffer.chars().collect();
        if start >= chars.len() {
            return None;
        }
        let end = end.min(chars.len());
        Some(chars[start..end].iter().collect())
    }

    /// Delete the selected text and collapse cursor to the start of selection.
    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            let start_byte = self.char_to_byte(start);
            let end_byte = self.char_to_byte(end);
            self.buffer.drain(start_byte..end_byte);
            self.cursor = start;
            self.selection_anchor = None;
        }
    }

    /// Clear selection without deleting text.
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Start or extend selection from current cursor position.
    fn ensure_anchor(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
    }

    /// Move cursor left with selection (Shift+Left).
    pub fn select_left(&mut self) {
        self.ensure_anchor();
        self.move_left();
    }

    /// Move cursor right with selection (Shift+Right).
    pub fn select_right(&mut self) {
        self.ensure_anchor();
        self.move_right();
    }

    /// Move cursor to start with selection (Shift+Home).
    pub fn select_home(&mut self) {
        self.ensure_anchor();
        self.home();
    }

    /// Move cursor to end with selection (Shift+End).
    pub fn select_end(&mut self) {
        self.ensure_anchor();
        self.end();
    }

    /// Whether the buffer contains newlines (is multiline).
    pub fn is_multiline(&self) -> bool {
        self.buffer.contains('\n')
    }

    /// Whether the cursor is on the first line of a multiline buffer.
    pub fn cursor_on_first_line(&self) -> bool {
        let chars: Vec<char> = self.buffer.chars().collect();
        // No newline before cursor → first line
        !chars[..self.cursor.min(chars.len())].contains(&'\n')
    }

    /// Whether the cursor is on the last line of a multiline buffer.
    pub fn cursor_on_last_line(&self) -> bool {
        let chars: Vec<char> = self.buffer.chars().collect();
        // No newline after cursor → last line
        !chars[self.cursor.min(chars.len())..].contains(&'\n')
    }

    /// Move cursor up one line within multiline text. Returns false if already on first line.
    pub fn move_up(&mut self) -> bool {
        let chars: Vec<char> = self.buffer.chars().collect();
        let pos = self.cursor.min(chars.len());

        // Find start of current line
        let current_line_start = chars[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);

        if current_line_start == 0 {
            return false; // already on first line
        }

        // Column within current line
        let col = pos - current_line_start;

        // Find start of previous line
        let prev_line_start = if current_line_start >= 2 {
            chars[..current_line_start - 1]
                .iter()
                .rposition(|&c| c == '\n')
                .map_or(0, |i| i + 1)
        } else {
            0
        };

        // Previous line length
        let prev_line_len = current_line_start - 1 - prev_line_start;

        self.cursor = prev_line_start + col.min(prev_line_len);
        true
    }

    /// Move cursor down one line within multiline text. Returns false if already on last line.
    pub fn move_down(&mut self) -> bool {
        let chars: Vec<char> = self.buffer.chars().collect();
        let pos = self.cursor.min(chars.len());

        // Find start of current line
        let current_line_start = chars[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);

        // Column within current line
        let col = pos - current_line_start;

        // Find end of current line (next newline or end of buffer)
        let current_line_end = chars[pos..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(chars.len(), |i| pos + i);

        if current_line_end >= chars.len() {
            return false; // already on last line
        }

        // Next line starts after the newline
        let next_line_start = current_line_end + 1;

        // Next line length
        let next_line_end = chars[next_line_start..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(chars.len(), |i| next_line_start + i);
        let next_line_len = next_line_end - next_line_start;

        self.cursor = next_line_start + col.min(next_line_len);
        true
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
    /// Copy selected text to clipboard (Ctrl+C when selection exists).
    CopySelection(String),
    /// Cut selected text to clipboard (Ctrl+X when selection exists).
    CutSelection(String),
    /// Open external editor for input (Ctrl+G).
    OpenEditor,
    /// Toggle mouse capture for text selection (Alt+M).
    ToggleMouseCapture,
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
        // Copy/Cut selection — Ctrl+C copies if selection exists, otherwise quit
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if let Some(text) = input.selected_text() {
                input.clear_selection();
                InputAction::CopySelection(text)
            } else {
                InputAction::Quit
            }
        }
        // Ctrl+X — cut if selection exists, otherwise toggle expand
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
            if let Some(text) = input.selected_text() {
                input.delete_selection();
                InputAction::CutSelection(text)
            } else {
                InputAction::ToggleExpand
            }
        }

        // Ctrl shortcuts
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => InputAction::CycleTranscript,
        (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            input.clear_selection();
            input.home();
            InputAction::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            input.clear_selection();
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
        (KeyModifiers::ALT, KeyCode::Char('m')) => InputAction::ToggleMouseCapture,
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

        // Selection with Shift+Arrow
        (KeyModifiers::SHIFT, KeyCode::Left) => {
            input.select_left();
            InputAction::None
        }
        (KeyModifiers::SHIFT, KeyCode::Right) => {
            input.select_right();
            InputAction::None
        }
        (KeyModifiers::SHIFT, KeyCode::Home) => {
            input.select_home();
            InputAction::None
        }
        (KeyModifiers::SHIFT, KeyCode::End) => {
            input.select_end();
            InputAction::None
        }

        // Editing
        (_, KeyCode::Backspace) => {
            input.delete_back();
            InputAction::None
        }
        (_, KeyCode::Delete) => {
            if input.selection_anchor.is_some() {
                input.delete_selection();
            } else if input.cursor < input.buffer.chars().count() {
                input.move_right();
                input.delete_back();
            }
            InputAction::None
        }
        (_, KeyCode::Left) => {
            input.clear_selection();
            input.move_left();
            InputAction::None
        }
        (_, KeyCode::Right) => {
            input.clear_selection();
            input.move_right();
            InputAction::None
        }

        // Up/Down: navigate within multiline text, fall through to history at boundaries
        (_, KeyCode::Up) => {
            if input.is_multiline() && !input.cursor_on_first_line() {
                input.clear_selection();
                input.move_up();
            } else if let Some(entry) = history.prev(&input.buffer) {
                input.set(entry);
            }
            InputAction::None
        }
        (_, KeyCode::Down) => {
            if input.is_multiline() && !input.cursor_on_last_line() {
                input.clear_selection();
                input.move_down();
            } else {
                match history.newer() {
                    NextResult::Entry(entry) => input.set(entry),
                    NextResult::Draft(draft) => input.set(draft),
                    NextResult::NotBrowsing => {}
                }
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

    // ── Selection tests ─────────────────────────────────────────────────

    #[test]
    fn select_left_creates_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.select_left();
        assert_eq!(s.selection_anchor, Some(5));
        assert_eq!(s.cursor, 4);
        assert_eq!(s.selected_text(), Some("o".to_string()));
    }

    #[test]
    fn select_right_extends_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.cursor = 0;
        s.select_right();
        s.select_right();
        assert_eq!(s.selection_anchor, Some(0));
        assert_eq!(s.cursor, 2);
        assert_eq!(s.selected_text(), Some("he".to_string()));
    }

    #[test]
    fn select_home_selects_to_start() {
        let mut s = InputState::default();
        s.set("hello");
        s.cursor = 3;
        s.select_home();
        assert_eq!(s.selected_text(), Some("hel".to_string()));
    }

    #[test]
    fn select_end_selects_to_end() {
        let mut s = InputState::default();
        s.set("hello");
        s.cursor = 2;
        s.select_end();
        assert_eq!(s.selected_text(), Some("llo".to_string()));
    }

    #[test]
    fn delete_selection_removes_text() {
        let mut s = InputState::default();
        s.set("hello world");
        s.cursor = 5;
        s.selection_anchor = Some(0);
        s.delete_selection();
        assert_eq!(s.buffer, " world");
        assert_eq!(s.cursor, 0);
        assert!(s.selection_anchor.is_none());
    }

    #[test]
    fn insert_replaces_selection() {
        let mut s = InputState::default();
        s.set("hello world");
        s.cursor = 5;
        s.selection_anchor = Some(0);
        s.insert('X');
        assert_eq!(s.buffer, "X world");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.cursor = 5;
        s.selection_anchor = Some(3);
        s.delete_back();
        assert_eq!(s.buffer, "hel");
        assert_eq!(s.cursor, 3);
    }

    #[test]
    fn move_left_clears_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.selection_anchor = Some(2);
        s.move_left();
        // Selection cleared by caller in handle_key, not by move_left itself
        // move_left only moves the cursor
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn clear_removes_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.selection_anchor = Some(0);
        s.clear();
        assert!(s.selection_anchor.is_none());
        assert_eq!(s.buffer, "");
    }

    #[test]
    fn set_clears_selection() {
        let mut s = InputState::default();
        s.set("hello");
        s.selection_anchor = Some(2);
        s.set("new text");
        assert!(s.selection_anchor.is_none());
    }

    #[test]
    fn no_selection_returns_none() {
        let s = InputState::default();
        assert!(s.selected_text().is_none());
        assert!(s.selection_range().is_none());
    }

    // ── Multiline navigation tests ──────────────────────────────────────

    #[test]
    fn is_multiline_detects_newlines() {
        let mut s = InputState::default();
        s.set("single line");
        assert!(!s.is_multiline());
        s.set("line one\nline two");
        assert!(s.is_multiline());
    }

    #[test]
    fn cursor_on_first_line() {
        let mut s = InputState::default();
        s.set("line one\nline two");
        s.cursor = 3; // middle of first line
        assert!(s.cursor_on_first_line());
        assert!(!s.cursor_on_last_line());
    }

    #[test]
    fn cursor_on_last_line() {
        let mut s = InputState::default();
        s.set("line one\nline two");
        s.cursor = 12; // middle of second line
        assert!(!s.cursor_on_first_line());
        assert!(s.cursor_on_last_line());
    }

    #[test]
    fn move_up_within_multiline() {
        let mut s = InputState::default();
        s.set("abc\ndef\nghi");
        s.cursor = 7; // 'e' in second line (col 3)
        // Actually: a(0)b(1)c(2)\n(3)d(4)e(5)f(6)\n(7)g(8)h(9)i(10)
        s.cursor = 5; // 'e' in second line (col 1)
        assert!(s.move_up());
        assert_eq!(s.cursor, 1); // 'b' in first line (col 1)
    }

    #[test]
    fn move_up_from_first_line_returns_false() {
        let mut s = InputState::default();
        s.set("abc\ndef");
        s.cursor = 1;
        assert!(!s.move_up());
        assert_eq!(s.cursor, 1); // unchanged
    }

    #[test]
    fn move_down_within_multiline() {
        let mut s = InputState::default();
        s.set("abc\ndef\nghi");
        s.cursor = 1; // 'b' in first line (col 1)
        assert!(s.move_down());
        assert_eq!(s.cursor, 5); // 'e' in second line (col 1)
    }

    #[test]
    fn move_down_from_last_line_returns_false() {
        let mut s = InputState::default();
        s.set("abc\ndef");
        s.cursor = 5; // second line
        assert!(!s.move_down());
        assert_eq!(s.cursor, 5); // unchanged
    }

    #[test]
    fn move_down_clamps_to_shorter_line() {
        let mut s = InputState::default();
        s.set("abcdef\nhi");
        s.cursor = 5; // col 5 in first line (6 chars)
        assert!(s.move_down());
        // second line "hi" is only 2 chars, clamp to col 2
        assert_eq!(s.cursor, 9); // 'i' position: 7(start) + 2(len) = 9
    }
}
