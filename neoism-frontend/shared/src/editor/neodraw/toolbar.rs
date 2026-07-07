//! The floating editor toolbar: tool buttons, a colour palette, and
//! stroke sizes. One pure layout function drives both rendering and
//! hit-testing so the visible buttons and the clickable regions can
//! never drift apart.

use super::pane::{DrawPane, Tool};
use super::scene::Color;

/// What a toolbar button does when clicked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolbarItem {
    Tool(Tool),
    Color(Color),
    Size(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct ToolbarButton {
    pub rect: [f32; 4],
    pub item: ToolbarItem,
    /// Single-glyph label for tool buttons (colour/size draw swatches).
    pub glyph: &'static str,
}

pub struct Toolbar {
    pub bar_rect: [f32; 4],
    pub buttons: Vec<ToolbarButton>,
}

/// Curated palette, mirroring the Excalidraw-style swatch row.
pub const PALETTE: [Color; 8] = [
    Color::rgb(0xff, 0xff, 0xff),
    Color::rgb(0x86, 0x8e, 0x96),
    Color::rgb(0x42, 0x63, 0xeb),
    Color::rgb(0x41, 0xb8, 0xff),
    Color::rgb(0x2f, 0x9e, 0x44),
    Color::rgb(0xf1, 0xc4, 0x0f),
    Color::rgb(0xe8, 0x59, 0x0c),
    Color::rgb(0xff, 0x5a, 0x5a),
];

/// Stroke widths behind the S / M / L / XL buttons.
pub const SIZES: [(f32, &str); 4] = [(1.0, "S"), (2.0, "M"), (4.0, "L"), (7.0, "XL")];

// Nerd Font (Font Awesome) glyphs — the same icon set the file tree
// and tabs use, so the toolbar matches the rest of the chrome.
const TOOLS: [(Tool, &str); 10] = [
    (Tool::Select, "\u{f245}"),      // mouse-pointer
    (Tool::Hand, "\u{f256}"),        // hand
    (Tool::Pen, "\u{f040}"),         // pencil
    (Tool::Highlighter, "\u{f1fc}"), // paint-brush (highlighter/marker)
    (Tool::Eraser, "\u{f12d}"),      // eraser
    (Tool::Rect, "\u{f096}"),        // square-o
    (Tool::Ellipse, "\u{f10c}"),     // circle-o
    (Tool::Arrow, "\u{f178}"),       // long-arrow-right
    (Tool::Line, "\u{f068}"),        // minus (line)
    (Tool::Text, "\u{f031}"),        // font
];

const BTN: f32 = 26.0;
const GAP: f32 = 5.0;
const SEP: f32 = 12.0;
const PAD: f32 = 8.0;
const MARGIN_BOTTOM: f32 = 18.0;

/// Build the toolbar geometry for a pane `rect` (window-logical px).
pub fn build_toolbar(rect: [f32; 4]) -> Toolbar {
    let count = TOOLS.len() + PALETTE.len() + SIZES.len();
    let inner_w = count as f32 * BTN + (count as f32 - 1.0) * GAP + 2.0 * SEP;
    let bar_w = inner_w + 2.0 * PAD;
    let bar_h = BTN + 2.0 * PAD;
    let bar_x = rect[0] + (rect[2] - bar_w) * 0.5;
    let bar_y = rect[1] + rect[3] - bar_h - MARGIN_BOTTOM;

    let mut buttons = Vec::with_capacity(count);
    let mut x = bar_x + PAD;
    let y = bar_y + PAD;
    let mut push = |x: &mut f32, item: ToolbarItem, glyph: &'static str| {
        buttons.push(ToolbarButton {
            rect: [*x, y, BTN, BTN],
            item,
            glyph,
        });
        *x += BTN + GAP;
    };

    for (tool, glyph) in TOOLS {
        push(&mut x, ToolbarItem::Tool(tool), glyph);
    }
    x += SEP - GAP;
    for color in PALETTE {
        push(&mut x, ToolbarItem::Color(color), "");
    }
    x += SEP - GAP;
    for (w, glyph) in SIZES {
        push(&mut x, ToolbarItem::Size(w), glyph);
    }

    Toolbar {
        bar_rect: [bar_x, bar_y, bar_w, bar_h],
        buttons,
    }
}

/// Hit-test a window-logical point against the toolbar.
pub fn toolbar_hit(rect: [f32; 4], x: f32, y: f32) -> Option<ToolbarItem> {
    let bar = build_toolbar(rect);
    bar.buttons
        .iter()
        .find(|b| {
            x >= b.rect[0]
                && x <= b.rect[0] + b.rect[2]
                && y >= b.rect[1]
                && y <= b.rect[1] + b.rect[3]
        })
        .map(|b| b.item)
}

/// Whether a window-logical point falls anywhere on the toolbar bar.
pub fn point_on_toolbar(rect: [f32; 4], x: f32, y: f32) -> bool {
    let [bx, by, bw, bh] = build_toolbar(rect).bar_rect;
    x >= bx && x <= bx + bw && y >= by && y <= by + bh
}

impl DrawPane {
    /// Apply a toolbar click: switch tool, or set the stroke colour /
    /// width for new shapes (and the current selection).
    pub fn apply_toolbar_item(&mut self, item: ToolbarItem) {
        match item {
            ToolbarItem::Tool(tool) => self.set_tool(tool),
            ToolbarItem::Color(color) => {
                self.style_defaults.stroke = color;
                if self.has_selection() {
                    self.checkpoint();
                    let ids = self.selection.clone();
                    for s in &mut self.scene.shapes {
                        if ids.contains(&s.id) {
                            s.style.stroke = color;
                            // Recolour an existing fill so it stays visible.
                            if let Some(fill) = s.style.fill.as_mut() {
                                *fill = color;
                            }
                        }
                    }
                    self.dirty = true;
                }
            }
            ToolbarItem::Size(width) => {
                self.style_defaults.width = width;
                if self.has_selection() {
                    self.checkpoint();
                    let ids = self.selection.clone();
                    for s in &mut self.scene.shapes {
                        if ids.contains(&s.id) {
                            s.style.width = width;
                        }
                    }
                    self.dirty = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_centered_and_within_rect() {
        let rect = [0.0, 0.0, 2000.0, 1000.0];
        let bar = build_toolbar(rect);
        let [bx, _, bw, _] = bar.bar_rect;
        let center = bx + bw * 0.5;
        assert!((center - 1000.0).abs() < 0.5, "bar horizontally centered");
        assert_eq!(bar.buttons.len(), 10 + 8 + 4);
    }

    #[test]
    fn hit_test_matches_first_tool_button() {
        let rect = [0.0, 0.0, 2000.0, 1000.0];
        let first = build_toolbar(rect).buttons[0];
        let cx = first.rect[0] + first.rect[2] * 0.5;
        let cy = first.rect[1] + first.rect[3] * 0.5;
        assert_eq!(
            toolbar_hit(rect, cx, cy),
            Some(ToolbarItem::Tool(Tool::Select))
        );
        assert!(point_on_toolbar(rect, cx, cy));
        assert_eq!(toolbar_hit(rect, 5.0, 5.0), None);
    }
}
