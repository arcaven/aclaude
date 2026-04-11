//! Layout computation for the TUI.
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ CONVERSATION VIEWPORT    ┌────────────┐ │
//! │ (scrollable, full width) │  PORTRAIT  │ │
//! │                          │  (overlay) │ │
//! │                          └────────────┘ │
//! │                                         │
//! ├─────────────────────────────────────────┤  ← optional permission prompt
//! │ ┌─ Permission Required ───────────────┐ │
//! │ │  Tool: Edit                         │ │
//! │ │  [a] Allow  [d] Deny               │ │
//! │ └────────────────────────────────────-┘ │
//! ├─────────────────────────────────────────┤
//! │ > INPUT AREA                            │
//! ├─────────────────────────────────────────┤
//! │ STATUS BAR                              │
//! └─────────────────────────────────────────┘
//! ```

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::app::{PortraitPosition, PortraitSize};

/// Minimum terminal width to show portrait overlay.
const MIN_WIDTH_FOR_PORTRAIT: u16 = 60;

/// Input area height in rows.
const INPUT_HEIGHT: u16 = 3;

/// Status bar height in rows.
const STATUS_HEIGHT: u16 = 1;

/// Permission prompt height in rows (when active).
const PERMISSION_HEIGHT: u16 = 6;

/// Computed layout areas for a single frame.
pub struct TuiLayout {
    /// Conversation viewport (scrollable text, full width).
    pub conversation: Rect,
    /// Portrait overlay area (upper-right of conversation, may be zero-size).
    pub portrait: Rect,
    /// Permission prompt area (above input, zero-size when no prompt active).
    pub permission_prompt: Rect,
    /// User input area.
    pub input: Rect,
    /// Status bar.
    pub status: Rect,
}

/// Compute layout for the given terminal size and portrait configuration.
pub fn compute_layout(
    area: Rect,
    portrait_size: PortraitSize,
    portrait_position: PortraitPosition,
    has_portrait: bool,
    has_permission_prompt: bool,
    focus_mode: bool,
) -> TuiLayout {
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

        let portrait = if has_portrait && area.width >= MIN_WIDTH_FOR_PORTRAIT {
            let pw = portrait_column_width(portrait_size, area.width);
            let ph = (pw * 3 / 4).min(conversation.height / 2).max(4);
            let x = conversation.x + conversation.width.saturating_sub(pw);
            let y = match portrait_position {
                PortraitPosition::TopRight => conversation.y,
                PortraitPosition::BottomRight => {
                    conversation.y + conversation.height.saturating_sub(ph)
                }
            };
            Rect {
                x,
                y,
                width: pw,
                height: ph,
            }
        } else {
            Rect::default()
        };

        return TuiLayout {
            conversation,
            portrait,
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
            Constraint::Length(INPUT_HEIGHT),      // input
            Constraint::Length(STATUS_HEIGHT),     // status
        ]
    } else {
        vec![
            Constraint::Min(1),                // conversation
            Constraint::Length(INPUT_HEIGHT),  // input
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

    // Portrait overlay — positioned in upper-right or lower-right of conversation
    let portrait = if has_portrait && area.width >= MIN_WIDTH_FOR_PORTRAIT {
        let pw = portrait_column_width(portrait_size, area.width);
        let ph = (pw * 3 / 4).min(conversation.height / 2).max(4);
        let x = conversation.x + conversation.width.saturating_sub(pw);
        let y = match portrait_position {
            PortraitPosition::TopRight => conversation.y,
            PortraitPosition::BottomRight => {
                conversation.y + conversation.height.saturating_sub(ph)
            }
        };
        Rect {
            x,
            y,
            width: pw,
            height: ph,
        }
    } else {
        Rect::default()
    };

    TuiLayout {
        conversation,
        portrait,
        permission_prompt,
        input,
        status,
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
