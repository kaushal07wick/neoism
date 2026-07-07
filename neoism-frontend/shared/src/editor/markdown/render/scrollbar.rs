use sugarloaf::Sugarloaf;

use crate::editor::markdown::MarkdownPane;

use super::types::{
    DEPTH, MARKDOWN_SCROLLBAR_MARGIN, MARKDOWN_SCROLLBAR_MIN_THUMB_HEIGHT,
    MARKDOWN_SCROLLBAR_WIDTH, ORDER_TEXT,
};
use crate::editor::markdown::render::draw::draw_rounded_rect_clipped;
use crate::primitives::ide_theme::IdeTheme;

pub(super) fn draw_markdown_scrollbar(
    sugarloaf: &mut Sugarloaf,
    pane: &mut MarkdownPane,
    rect: [f32; 4],
    content_height: f32,
    theme: &IdeTheme,
    mouse: Option<[f32; 2]>,
    clip: [f32; 4],
) {
    let [x, y, w, h] = rect;
    if content_height <= h + 1.0 || h <= MARKDOWN_SCROLLBAR_MIN_THUMB_HEIGHT {
        return;
    }
    let track_h = (h - MARKDOWN_SCROLLBAR_MARGIN * 2.0).max(1.0);
    let max_scroll = (content_height - h).max(1.0);
    let thumb_h = (track_h * (h / content_height))
        .clamp(MARKDOWN_SCROLLBAR_MIN_THUMB_HEIGHT.min(track_h), track_h);
    let progress = (pane.scroll_y / max_scroll).clamp(0.0, 1.0);
    let thumb_y = y + MARKDOWN_SCROLLBAR_MARGIN + (track_h - thumb_h) * progress;
    let track_rect = [
        x + w - MARKDOWN_SCROLLBAR_WIDTH - MARKDOWN_SCROLLBAR_MARGIN,
        y + MARKDOWN_SCROLLBAR_MARGIN,
        MARKDOWN_SCROLLBAR_WIDTH,
        track_h,
    ];
    let thumb_rect = [track_rect[0], thumb_y, MARKDOWN_SCROLLBAR_WIDTH, thumb_h];
    pane.register_scrollbar_rect(track_rect, thumb_rect, h, mouse);

    let hovered = mouse.is_some_and(|[mx, my]| {
        mx >= track_rect[0] - 5.0
            && mx <= track_rect[0] + track_rect[2] + 5.0
            && my >= track_rect[1]
            && my <= track_rect[1] + track_rect[3]
    });
    draw_rounded_rect_clipped(
        sugarloaf,
        clip,
        track_rect[0],
        track_rect[1],
        track_rect[2],
        track_rect[3],
        MARKDOWN_SCROLLBAR_WIDTH * 0.5,
        theme.f32_alpha(theme.border, if hovered { 0.22 } else { 0.12 }),
        DEPTH,
        ORDER_TEXT + 2,
    );
    draw_rounded_rect_clipped(
        sugarloaf,
        clip,
        thumb_rect[0],
        thumb_rect[1],
        thumb_rect[2],
        thumb_rect[3],
        MARKDOWN_SCROLLBAR_WIDTH * 0.5,
        theme.f32_alpha(theme.fg, if hovered { 0.62 } else { 0.44 }),
        DEPTH,
        ORDER_TEXT + 3,
    );
}
