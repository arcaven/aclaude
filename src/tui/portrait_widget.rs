//! Portrait image overlay widget for ratatui.
//!
//! Wraps `ratatui-image` to render persona portraits in the TUI.
//! Falls back gracefully when the terminal doesn't support inline images.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use super::app::PortraitSize;
use crate::portrait::PortraitPaths;

/// Build a Picker from environment variables set by session_cmd during
/// session creation. Avoids the unreliable per-pane stdio query inside tmux.
fn picker_from_env() -> Option<Picker> {
    use ratatui_image::picker::ProtocolType;

    let proto_str = std::env::var("FORESTAGE_IMAGE_PROTOCOL").ok()?;
    let proto = match proto_str.as_str() {
        "kitty" => ProtocolType::Kitty,
        "iterm2" => ProtocolType::Iterm2,
        "sixel" => ProtocolType::Sixel,
        _ => return None, // halfblocks or unknown — fall through to query
    };

    let font_size = std::env::var("FORESTAGE_IMAGE_FONT_SIZE")
        .ok()
        .and_then(|s| {
            let parts: Vec<&str> = s.split('x').collect();
            if parts.len() == 2 {
                Some((parts[0].parse::<u16>().ok()?, parts[1].parse::<u16>().ok()?))
            } else {
                None
            }
        })
        .unwrap_or((10, 20));

    #[allow(deprecated)] // no non-deprecated constructor with known font size
    let mut picker = Picker::from_fontsize(font_size);
    picker.set_protocol_type(proto);
    Some(picker)
}

/// Portrait widget state.
pub struct PortraitWidget {
    picker: Picker,
    image_state: Option<StatefulProtocol>,
    current_path: Option<PathBuf>,
    /// Image dimensions in pixels (width, height).
    image_pixels: Option<(u32, u32)>,
}

impl PortraitWidget {
    /// Create a new portrait widget.
    ///
    /// Must be called AFTER `crossterm::terminal::enable_raw_mode()` and
    /// BEFORE `Terminal::new()`.
    ///
    /// When running under marvel (detected via `MARVEL_SESSION` env
    /// var), skip `Picker::from_query_stdio()`. The query writes a
    /// terminal-capability escape sequence to stdout and synchronously
    /// reads the response from stdin. Under marvel-managed tmux panes,
    /// any bytes the control plane sends via `marvel inject` during
    /// that window are consumed by this query — not delivered as key
    /// events — producing a silent "first-keystrokes lost" race on
    /// cold-start sessions. See aae-orc-9cp.
    ///
    /// Marvel-managed sessions still honour `FORESTAGE_IMAGE_PROTOCOL`
    /// if the operator has manually selected a protocol.
    pub fn new() -> Option<Self> {
        let picker = if std::env::var_os("MARVEL_SESSION").is_some() {
            picker_from_env()?
        } else {
            picker_from_env().or_else(|| Picker::from_query_stdio().ok())?
        };
        Some(Self {
            picker,
            image_state: None,
            current_path: None,
            image_pixels: None,
        })
    }

    /// Set the portrait size, loading the appropriate image.
    pub fn set_size(&mut self, size: PortraitSize, paths: &PortraitPaths) {
        let size_str = match size {
            PortraitSize::Small => "small",
            PortraitSize::Medium => "medium",
            PortraitSize::Large => "large",
            PortraitSize::Original => "original",
        };
        let target_path = paths.best_for_size(size_str).map(Path::to_path_buf);

        if self.current_path == target_path {
            return;
        }

        self.current_path = target_path.clone();
        self.image_pixels = None;

        if let Some(path) = target_path {
            match image::open(&path) {
                Ok(img) => {
                    self.image_pixels = Some((img.width(), img.height()));
                    let protocol = self.picker.new_resize_protocol(img);
                    self.image_state = Some(protocol);
                }
                Err(_) => {
                    self.image_state = None;
                }
            }
        } else {
            self.image_state = None;
        }
    }

    /// Whether a portrait image is loaded and ready to render.
    pub fn has_image(&self) -> bool {
        self.image_state.is_some()
    }

    /// Get the image dimensions in terminal cells for the given max width.
    ///
    /// Uses the picker's font size to convert pixel dimensions to cell
    /// dimensions. Returns (cols, rows) clamped to max_width.
    pub fn cell_size(&self, max_width: u16, max_height: u16) -> Option<(u16, u16)> {
        let (px_w, px_h) = self.image_pixels?;
        let font = self.picker.font_size();
        if font.0 == 0 || font.1 == 0 {
            return None;
        }

        // Image size in cells at native resolution (cap to prevent u16 overflow)
        let capped_w = px_w.min(u16::MAX as u32) as u16;
        let capped_h = px_h.min(u16::MAX as u32) as u16;
        let native_cols = capped_w.div_ceil(font.0);
        let native_rows = capped_h.div_ceil(font.1);

        if native_cols == 0 || native_rows == 0 {
            return None;
        }

        // Scale to fit within max_width, preserving aspect ratio
        let scale_w = max_width as f64 / native_cols as f64;
        let scale_h = max_height as f64 / native_rows as f64;
        let scale = scale_w.min(scale_h).min(1.0); // never upscale

        let cols = (native_cols as f64 * scale).round() as u16;
        let rows = (native_rows as f64 * scale).round() as u16;

        Some((cols.max(1), rows.max(1)))
    }

    /// Force the image protocol to re-send on the next render.
    ///
    /// Call this when the terminal's graphics layer has been cleared
    /// (e.g. tmux window switch) but the app state hasn't changed.
    /// The StatefulProtocol caches the rendered image and skips
    /// re-sending when it thinks nothing changed — this resets that.
    pub fn force_redraw(&mut self) {
        if let (Some(path), Some(_)) = (&self.current_path, &self.image_state) {
            if let Ok(img) = image::open(path) {
                self.image_pixels = Some((img.width(), img.height()));
                let protocol = self.picker.new_resize_protocol(img);
                self.image_state = Some(protocol);
            }
        }
    }

    /// Render the portrait in the given area.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        if let Some(state) = &mut self.image_state {
            let image = StatefulImage::default().resize(Resize::Fit(None));
            frame.render_stateful_widget(image, area, state);
        }
    }
}
