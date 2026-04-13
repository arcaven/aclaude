//! Layout computation for the TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::app::PortraitPosition;

/// Minimum input area height in rows (1 border + 1 text + 1 border).
const MIN_INPUT_HEIGHT: u16 = 3;

/// Maximum input area height (cap growth to prevent eating the conversation).
const MAX_INPUT_HEIGHT: u16 = 10;

/// Status bar height in rows.
const STATUS_HEIGHT: u16 = 1;

/// Permission prompt height in rows (when active).
const PERMISSION_HEIGHT: u16 = 6;

/// Portrait margin from the right terminal edge.
const PORTRAIT_MARGIN_RIGHT: u16 = 1;

/// Computed layout areas for a single frame.
pub struct TuiLayout {
    pub conversation: Rect,
    pub portrait: Rect,
    pub permission_prompt: Rect,
    pub input: Rect,
    pub status: Rect,
}

/// Compute layout for the given terminal size.
pub fn compute_layout(
    area: Rect,
    portrait_position: PortraitPosition,
    portrait_cell_size: Option<(u16, u16)>,
    has_permission_prompt: bool,
    focus_mode: bool,
    input_len: usize,
) -> TuiLayout {
    // Compute input height based on text length — grows as user types.
    // No left/right borders, so inner width = area width.
    // +2 for "> " prefix, +2 for borders (top + bottom).
    let text_cols = area.width; // no left/right borders
    let capped_len = input_len.min(u16::MAX as usize - 2) as u16;
    let text_with_prefix = capped_len + 2; // "> " prefix
    let text_lines = if text_cols == 0 {
        1
    } else {
        (text_with_prefix / text_cols) + 1
    };
    let input_height = (text_lines + 2).clamp(MIN_INPUT_HEIGHT, MAX_INPUT_HEIGHT);
    // Focus mode: maximize conversation, minimal chrome
    if focus_mode {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // conversation
                Constraint::Length(1), // minimal input (no borders)
            ])
            .split(area);

        let conversation = vertical[0];
        let input = vertical[1];

        // In focus mode there's no status bar — use input as the bottom edge
        return TuiLayout {
            portrait: compute_portrait_rect(
                portrait_position,
                portrait_cell_size,
                conversation,
                input,
            ),
            conversation,
            permission_prompt: Rect::default(),
            input,
            status: Rect::default(),
        };
    }

    // Build constraints based on whether permission prompt is active
    let constraints = if has_permission_prompt {
        vec![
            Constraint::Min(1),                    // conversation
            Constraint::Length(PERMISSION_HEIGHT), // permission prompt
            Constraint::Length(input_height),      // input
            Constraint::Length(STATUS_HEIGHT),     // status
        ]
    } else {
        vec![
            Constraint::Min(1),                // conversation
            Constraint::Length(input_height),  // input
            Constraint::Length(STATUS_HEIGHT), // status
        ]
    };

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let (conversation, permission_prompt, input, status) = if has_permission_prompt {
        (vertical[0], vertical[1], vertical[2], vertical[3])
    } else {
        (vertical[0], Rect::default(), vertical[1], vertical[2])
    };

    TuiLayout {
        portrait: compute_portrait_rect(portrait_position, portrait_cell_size, conversation, input),
        conversation,
        permission_prompt,
        input,
        status,
    }
}

/// Compute the portrait overlay Rect.
///
/// Uses actual image cell dimensions for a tight fit. The right edge is
/// always anchored at `terminal_width - PORTRAIT_MARGIN_RIGHT` regardless of
/// portrait size. Position:
/// - TopRight: flush with top of conversation area
/// - BottomRight: bottom edge at the input area's bottom border (above status bar)
fn compute_portrait_rect(
    portrait_position: PortraitPosition,
    portrait_cell_size: Option<(u16, u16)>,
    conversation: Rect,
    input: Rect,
) -> Rect {
    let Some((pw, ph)) = portrait_cell_size else {
        return Rect::default();
    };

    if pw == 0 || ph == 0 {
        return Rect::default();
    }

    // Right edge: portrait right side sits at terminal right minus margin
    let right_edge = conversation.x + conversation.width;
    let x = right_edge
        .saturating_sub(PORTRAIT_MARGIN_RIGHT)
        .saturating_sub(pw);

    let y = match portrait_position {
        PortraitPosition::TopRight => conversation.y,
        PortraitPosition::BottomRight => {
            // Bottom edge at the input area's bottom border (just above status bar).
            // Portrait overlays conversation and input but not the status bar.
            let bottom = input.y + input.height;
            bottom.saturating_sub(ph)
        }
    };

    Rect {
        x,
        y,
        width: pw,
        height: ph,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn portrait_top_right_flush_with_corner() {
        let conversation = rect(0, 0, 80, 30);
        let input = rect(0, 30, 80, 3);
        let r = compute_portrait_rect(
            PortraitPosition::TopRight,
            Some((20, 15)),
            conversation,
            input,
        );
        // y flush with conversation top (no margin)
        assert_eq!(r.y, 0);
        // right edge: 80 - 1 (margin) - 20 (width) = 59
        assert_eq!(r.x, 59);
        assert_eq!(r.width, 20);
        assert_eq!(r.height, 15);
    }

    #[test]
    fn portrait_bottom_right_above_status_bar() {
        let conversation = rect(0, 0, 80, 30);
        // input at y=30 h=3 (status at y=33)
        let input = rect(0, 30, 80, 3);
        let r = compute_portrait_rect(
            PortraitPosition::BottomRight,
            Some((20, 15)),
            conversation,
            input,
        );
        // y: (input.y + input.height) - height = (30 + 3) - 15 = 18
        assert_eq!(r.y, 18);
        assert_eq!(r.x, 59);
    }

    #[test]
    fn portrait_none_returns_default() {
        let r = compute_portrait_rect(
            PortraitPosition::TopRight,
            None,
            rect(0, 0, 80, 30),
            rect(0, 30, 80, 3),
        );
        assert_eq!(r, Rect::default());
    }

    #[test]
    fn portrait_zero_size_returns_default() {
        let r = compute_portrait_rect(
            PortraitPosition::TopRight,
            Some((0, 10)),
            rect(0, 0, 80, 30),
            rect(0, 30, 80, 3),
        );
        assert_eq!(r, Rect::default());
    }

    #[test]
    fn layout_normal_mode_has_all_areas() {
        let area = rect(0, 0, 80, 40);
        let layout = compute_layout(area, PortraitPosition::BottomRight, None, false, false, 0);
        // Status bar is 1 row at the bottom
        assert_eq!(layout.status.height, 1);
        assert_eq!(layout.status.y, 39);
        // Input is MIN_INPUT_HEIGHT (3) above status
        assert_eq!(layout.input.height, 3);
        assert_eq!(layout.input.y, 36);
        // Conversation fills the rest
        assert_eq!(layout.conversation.y, 0);
        assert_eq!(layout.conversation.height, 36);
        // No permission prompt
        assert_eq!(layout.permission_prompt, Rect::default());
    }
}
