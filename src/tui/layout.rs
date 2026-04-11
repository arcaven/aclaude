//! Layout computation for the TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::app::{PortraitPosition, PortraitSize};

/// Minimum terminal width to show portrait overlay.
const MIN_WIDTH_FOR_PORTRAIT: u16 = 60;

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
    portrait_size: PortraitSize,
    portrait_position: PortraitPosition,
    has_portrait: bool,
    has_permission_prompt: bool,
    focus_mode: bool,
    input_len: usize,
) -> TuiLayout {
    // Compute input height based on text length — grows as user types.
    // No left/right borders, so inner width = area width.
    // +2 for "> " prefix, +2 for borders (top + bottom).
    let text_cols = area.width; // no left/right borders
    let text_with_prefix = input_len as u16 + 2; // "> " prefix
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
                portrait_size,
                portrait_position,
                has_portrait,
                area.width,
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
        portrait: compute_portrait_rect(
            portrait_size,
            portrait_position,
            has_portrait,
            area.width,
            conversation,
            input,
        ),
        conversation,
        permission_prompt,
        input,
        status,
    }
}

/// Compute the portrait overlay Rect.
///
/// The portrait is always right-aligned with a small margin from the
/// terminal edge. All sizes share the same right edge. Position:
/// - TopRight: top-right corner of conversation, slight margin from top and right
/// - BottomRight: bottom-right, just above the input area's top border
fn compute_portrait_rect(
    portrait_size: PortraitSize,
    portrait_position: PortraitPosition,
    has_portrait: bool,
    terminal_width: u16,
    conversation: Rect,
    input: Rect,
) -> Rect {
    if !has_portrait || terminal_width < MIN_WIDTH_FOR_PORTRAIT {
        return Rect::default();
    }

    let pw = portrait_column_width(portrait_size, terminal_width);
    // Height proportional to width (roughly 4:3 aspect ratio for portraits)
    let max_height = conversation.height.saturating_sub(PORTRAIT_MARGIN * 2);
    let ph = (pw * 3 / 4).min(max_height).max(4);

    // Right edge: flush to terminal right with margin
    let x = conversation
        .x
        .saturating_add(conversation.width)
        .saturating_sub(pw)
        .saturating_sub(PORTRAIT_MARGIN);

    let y = match portrait_position {
        PortraitPosition::TopRight => conversation.y + PORTRAIT_MARGIN,
        PortraitPosition::BottomRight => {
            // Just above the input area's top border, with margin
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

/// Portrait overlay width for a given size setting.
fn portrait_column_width(size: PortraitSize, terminal_width: u16) -> u16 {
    match size {
        PortraitSize::Small => 20,
        PortraitSize::Medium => 32,
        PortraitSize::Large => 48,
        PortraitSize::Original => (terminal_width / 3).min(64),
    }
}
