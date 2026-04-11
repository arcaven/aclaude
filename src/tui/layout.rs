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

/// Portrait margin from the terminal edge (right and top/bottom).
const PORTRAIT_MARGIN: u16 = 1;

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
/// always anchored at `terminal_width - PORTRAIT_MARGIN` regardless of
/// portrait size. Position:
/// - TopRight: anchored to top-right with margin
/// - BottomRight: anchored just above the input area's top border
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

    // Right edge anchored to terminal right minus margin
    let right_edge = conversation.x + conversation.width;
    let x = right_edge
        .saturating_sub(pw)
        .saturating_sub(PORTRAIT_MARGIN);

    let y = match portrait_position {
        PortraitPosition::TopRight => conversation.y + PORTRAIT_MARGIN,
        PortraitPosition::BottomRight => {
            // Just above the input area's top border
            input.y.saturating_sub(ph).saturating_sub(PORTRAIT_MARGIN)
        }
    };

    Rect {
        x,
        y,
        width: pw,
        height: ph,
    }
}
