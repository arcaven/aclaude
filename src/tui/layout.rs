//! Layout computation for the TUI.
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ CONVERSATION VIEWPORT    ┌────────────┐ │
//! │ (scrollable, full width) │  PORTRAIT  │ │
//! │                          │  (overlay) │ │
//! │                          └────────────┘ │
//! │                                         │
//! ├─────────────────────────────────────────┤
//! │ > INPUT AREA                            │
//! ├─────────────────────────────────────────┤
//! │ STATUS BAR                              │
//! └─────────────────────────────────────────┘
//! ```

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::app::PortraitSize;

/// Minimum terminal width to show portrait overlay.
const MIN_WIDTH_FOR_PORTRAIT: u16 = 60;

/// Input area height in rows.
const INPUT_HEIGHT: u16 = 3;

/// Status bar height in rows.
const STATUS_HEIGHT: u16 = 1;

/// Computed layout areas for a single frame.
pub struct TuiLayout {
    /// Conversation viewport (scrollable text, full width).
    pub conversation: Rect,
    /// Portrait overlay area (upper-right of conversation, may be zero-size).
    pub portrait: Rect,
    /// User input area.
    pub input: Rect,
    /// Status bar.
    pub status: Rect,
}

/// Compute layout for the given terminal size and portrait configuration.
///
/// The conversation viewport gets full width. The portrait is an overlay
/// in the upper-right corner of the conversation area — it renders on top
/// of the text, not beside it.
pub fn compute_layout(area: Rect, portrait_size: PortraitSize, has_portrait: bool) -> TuiLayout {
    // Vertical split: conversation | input | status
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                // conversation (full width)
            Constraint::Length(INPUT_HEIGHT),  // input
            Constraint::Length(STATUS_HEIGHT), // status
        ])
        .split(area);

    let conversation = vertical[0];
    let input = vertical[1];
    let status = vertical[2];

    // Portrait is an overlay in the upper-right corner of the conversation area
    let portrait = if has_portrait && area.width >= MIN_WIDTH_FOR_PORTRAIT {
        let pw = portrait_column_width(portrait_size, area.width);
        // Height: roughly match aspect ratio, cap at half the conversation height
        let ph = (pw * 3 / 4).min(conversation.height / 2).max(4);
        Rect {
            x: conversation.x + conversation.width.saturating_sub(pw),
            y: conversation.y,
            width: pw,
            height: ph,
        }
    } else {
        Rect::default()
    };

    TuiLayout {
        conversation,
        portrait,
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
