//! Minimal markdown renderer for the TUI.
//!
//! Line-by-line state machine that produces styled `ratatui::text::Line`s.
//! Handles: fenced code blocks, headers, bullet lists, inline bold, inline
//! code. No external crate — just string parsing.
//!
//! Panic-safe: `render_markdown_safe` catches panics and falls back to plain
//! text. Fast-path: `needs_markdown` scans for markup characters and skips
//! the renderer for pure prose.

use std::panic::AssertUnwindSafe;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render markdown text into styled ratatui lines, with panic safety.
///
/// If the renderer panics (malformed input, edge cases), falls back to
/// plain text rendering. Pattern from srg-claude-code-rust.
pub fn render_markdown_safe(text: &str) -> Vec<Line<'static>> {
    if !needs_markdown(text) {
        return plain_text(text);
    }
    std::panic::catch_unwind(AssertUnwindSafe(|| render_markdown(text)))
        .unwrap_or_else(|_| plain_text(text))
}

/// Quick byte scan for markdown characters.
///
/// Returns false for ~40% of assistant responses that are pure prose.
/// Pattern from pi_agent_rust's streaming_needs_markdown_renderer.
fn needs_markdown(text: &str) -> bool {
    for b in text.bytes() {
        match b {
            b'`' | b'*' | b'#' | b'-' | b'>' | b'|' | b'~' | b'[' | b']' => return true,
            _ => {}
        }
    }
    false
}

/// Plain text fallback — one Line per text line, no styling.
fn plain_text(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect()
}

/// Parser state for the line-by-line state machine.
enum State {
    Normal,
    InCodeBlock { _lang: String },
}

/// Render markdown text into styled ratatui lines.
fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut state = State::Normal;

    for line in text.lines() {
        match &state {
            State::InCodeBlock { .. } => {
                if line.starts_with("```") {
                    // End code block
                    lines.push(Line::from(Span::styled(
                        "└───",
                        Style::default().fg(Color::DarkGray),
                    )));
                    state = State::Normal;
                } else {
                    // Code line
                    lines.push(Line::from(Span::styled(
                        format!("│ {line}"),
                        Style::default().fg(Color::Cyan),
                    )));
                }
            }
            State::Normal => {
                // Fenced code block start
                if line.starts_with("```") {
                    let lang = line.trim_start_matches('`').trim().to_string();
                    let header = if lang.is_empty() {
                        "┌───".to_string()
                    } else {
                        format!("┌─── {lang} ")
                    };
                    lines.push(Line::from(Span::styled(
                        header,
                        Style::default().fg(Color::DarkGray),
                    )));
                    state = State::InCodeBlock { _lang: lang };
                    continue;
                }

                // Headers
                if let Some(rest) = line.strip_prefix("### ") {
                    lines.push(Line::from(Span::styled(
                        rest.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )));
                    continue;
                }
                if let Some(rest) = line.strip_prefix("## ") {
                    lines.push(Line::from(Span::styled(
                        rest.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )));
                    continue;
                }
                if let Some(rest) = line.strip_prefix("# ") {
                    lines.push(Line::from(Span::styled(
                        rest.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    )));
                    continue;
                }

                // Bullet lists
                if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                    let mut spans =
                        vec![Span::styled("  • ", Style::default().fg(Color::DarkGray))];
                    spans.extend(render_inline(rest));
                    lines.push(Line::from(spans));
                    continue;
                }

                // Numbered lists
                if line.chars().take_while(char::is_ascii_digit).count() > 0 {
                    if let Some(rest) = line
                        .find(". ")
                        .and_then(|i| if i <= 3 { Some(&line[i + 2..]) } else { None })
                    {
                        let num = &line[..line.find(". ").expect("just found it")];
                        let mut spans = vec![Span::styled(
                            format!("  {num}. "),
                            Style::default().fg(Color::DarkGray),
                        )];
                        spans.extend(render_inline(rest));
                        lines.push(Line::from(spans));
                        continue;
                    }
                }

                // Blockquotes
                if let Some(rest) = line.strip_prefix("> ") {
                    let mut spans = vec![Span::styled("▎ ", Style::default().fg(Color::DarkGray))];
                    spans.extend(render_inline(rest));
                    lines.push(Line::from(spans));
                    continue;
                }

                // Horizontal rule
                if line.chars().all(|c| c == '-' || c == ' ') && line.contains("---") {
                    lines.push(Line::from(Span::styled(
                        "─".repeat(40),
                        Style::default().fg(Color::DarkGray),
                    )));
                    continue;
                }

                // Normal text with inline formatting
                lines.push(Line::from(render_inline(line)));
            }
        }
    }

    lines
}

/// Render inline formatting within a line: **bold**, `code`, *italic*.
fn render_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Inline code: `...`
        if let Some(start) = remaining.find('`') {
            if let Some(end) = remaining[start + 1..].find('`') {
                // Push text before the code
                if start > 0 {
                    spans.extend(render_emphasis(&remaining[..start]));
                }
                // Push the code span
                let code = &remaining[start + 1..start + 1 + end];
                spans.push(Span::styled(
                    code.to_string(),
                    Style::default().fg(Color::Yellow),
                ));
                remaining = &remaining[start + 1 + end + 1..];
                continue;
            }
        }

        // No more inline code — render rest with emphasis
        spans.extend(render_emphasis(remaining));
        break;
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

/// Render bold (**...**) and italic (*...*) within text.
fn render_emphasis(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Bold: **...**
        if let Some(start) = remaining.find("**") {
            if let Some(end) = remaining[start + 2..].find("**") {
                if start > 0 {
                    spans.push(Span::raw(remaining[..start].to_string()));
                }
                let bold_text = &remaining[start + 2..start + 2 + end];
                spans.push(Span::styled(
                    bold_text.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                remaining = &remaining[start + 2 + end + 2..];
                continue;
            }
        }

        // Single *italic* (only if not **)
        if let Some(start) = remaining.find('*') {
            // Make sure it's not the start of **
            if !remaining[start..].starts_with("**") {
                if let Some(end) = remaining[start + 1..].find('*') {
                    if !remaining[start + 1 + end..].starts_with("**") {
                        if start > 0 {
                            spans.push(Span::raw(remaining[..start].to_string()));
                        }
                        let italic_text = &remaining[start + 1..start + 1 + end];
                        spans.push(Span::styled(
                            italic_text.to_string(),
                            Style::default().add_modifier(Modifier::ITALIC),
                        ));
                        remaining = &remaining[start + 1 + end + 1..];
                        continue;
                    }
                }
            }
        }

        // No more emphasis — push remaining as plain text
        spans.push(Span::raw(remaining.to_string()));
        break;
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passthrough() {
        let lines = render_markdown_safe("hello world");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "hello world");
    }

    #[test]
    fn needs_markdown_detects_backtick() {
        assert!(needs_markdown("use `foo`"));
        assert!(!needs_markdown("plain text only"));
    }

    #[test]
    fn header_rendering() {
        let lines = render_markdown("# Title");
        assert_eq!(lines.len(), 1);
        let text = lines[0].to_string();
        assert_eq!(text, "Title");
    }

    #[test]
    fn h2_h3_rendering() {
        let lines = render_markdown("## Subtitle\n### Section");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "Subtitle");
        assert_eq!(lines[1].to_string(), "Section");
    }

    #[test]
    fn code_block_rendering() {
        let text = "```rust\nfn main() {}\n```";
        let lines = render_markdown(text);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].to_string().contains("rust"));
        assert!(lines[1].to_string().contains("fn main"));
    }

    #[test]
    fn bullet_list() {
        let lines = render_markdown("- item one\n- item two");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].to_string().contains("•"));
        assert!(lines[0].to_string().contains("item one"));
    }

    #[test]
    fn inline_code() {
        let lines = render_markdown("use `foo` here");
        let text = lines[0].to_string();
        assert!(text.contains("foo"));
    }

    #[test]
    fn inline_bold() {
        let lines = render_markdown("this is **bold** text");
        let text = lines[0].to_string();
        assert!(text.contains("bold"));
    }

    #[test]
    fn blockquote() {
        let lines = render_markdown("> quoted text");
        let text = lines[0].to_string();
        assert!(text.contains("quoted text"));
    }

    #[test]
    fn horizontal_rule() {
        let lines = render_markdown("---");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("─"));
    }

    #[test]
    fn numbered_list() {
        let lines = render_markdown("1. first\n2. second");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].to_string().contains("1."));
        assert!(lines[0].to_string().contains("first"));
    }

    #[test]
    fn panic_safe_fallback() {
        // Even with weird input, should not panic
        let result = render_markdown_safe("```\n```\n```\n```");
        assert!(!result.is_empty());
    }

    #[test]
    fn empty_string() {
        let lines = render_markdown_safe("");
        assert!(lines.is_empty());
    }

    #[test]
    fn mixed_content() {
        let text = "# Header\n\nSome **bold** and `code`.\n\n- item\n\n```\ncode block\n```";
        let lines = render_markdown(text);
        assert!(lines.len() >= 6);
    }
}
