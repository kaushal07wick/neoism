use sugarloaf::text::DrawOpts;
use sugarloaf::Sugarloaf;

use crate::primitives::IdeTheme;

const DEPTH: f32 = 0.0;
const ORDER: u8 = 180;
const ROW_H: f32 = 34.0;
const TITLE_H: f32 = 30.0;
const MAX_ROWS: usize = 8;
const RADIUS: f32 = 14.0;

#[derive(Clone, Copy)]
pub struct InlinePickerRow<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub footer: &'a str,
    pub is_header: bool,
    /// Draw a filled accent-colored dot on the left to mark the currently
    /// active item (e.g. the session the user is presently inside).
    pub is_current: bool,
}

#[derive(Clone, Copy)]
pub struct InlinePickerView<'a> {
    pub title: &'a str,
    pub query: &'a str,
    pub selected: usize,
    pub scroll_offset: usize,
    pub list_scroll_offset: f32,
    pub cursor_offset: f32,
    pub rows: &'a [InlinePickerRow<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct InlinePickerRenderState {
    pub rect: [f32; 4],
    pub selected_cursor_rect: Option<[f32; 4]>,
}

/// Trim `text` with an ellipsis so its measured width is ≤ `max_w`.
fn truncate_to_pixel_width(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    opts: &DrawOpts,
    max_w: f32,
) -> String {
    if sugarloaf.text_mut().measure(text, opts) <= max_w {
        return text.to_string();
    }
    let ellipsis = "…";
    let mut buf: Vec<char> = text.chars().collect();
    while !buf.is_empty() {
        buf.pop();
        let candidate: String = buf.iter().collect::<String>() + ellipsis;
        if sugarloaf.text_mut().measure(&candidate, opts) <= max_w {
            return candidate;
        }
    }
    ellipsis.to_string()
}

pub fn layout(row_count: usize, input_rect: [f32; 4], scale: f32) -> Option<[f32; 4]> {
    if row_count == 0 {
        return None;
    }

    let s = scale.clamp(0.5, 3.0);
    let row_h = ROW_H * s;
    let title_h = TITLE_H * s;
    let visible_rows = row_count.min(MAX_ROWS).max(1);
    // Lock to the composer's width and x position so the popover lines up
    // edge-to-edge with the input chrome.
    let width = input_rect[2];
    let height = title_h + visible_rows as f32 * row_h;
    let x = input_rect[0];
    let y = (input_rect[1] - height - 6.0 * s).max(8.0 * s);
    Some([x, y, width, height])
}

pub fn render(
    sugarloaf: &mut Sugarloaf,
    view: InlinePickerView<'_>,
    input_rect: [f32; 4],
    theme: &IdeTheme,
    scale: f32,
) -> Option<InlinePickerRenderState> {
    let s = scale.clamp(0.5, 3.0);
    let row_h = ROW_H * s;
    let title_h = TITLE_H * s;
    let [x, y, width, height] = layout(view.rows.len(), input_rect, scale)?;
    let visible_rows = view.rows.len().min(MAX_ROWS).max(1);
    let selected = view.selected.min(view.rows.len().saturating_sub(1));
    let first = view
        .scroll_offset
        .min(view.rows.len().saturating_sub(visible_rows));
    let header_clip = [x, y, width, title_h];

    sugarloaf.rect(
        None,
        x,
        y,
        width,
        height,
        theme.f32(theme.black),
        DEPTH,
        ORDER,
    );

    sugarloaf.rounded_rect(
        None,
        x,
        y,
        width,
        height,
        theme.f32(theme.black),
        DEPTH,
        RADIUS * s,
        ORDER,
    );
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        width,
        height,
        theme.f32(theme.border),
        DEPTH,
        RADIUS * s,
        ORDER + 1,
    );
    sugarloaf.rounded_rect(
        None,
        x + s,
        y + s,
        (width - 2.0 * s).max(0.0),
        (height - 2.0 * s).max(0.0),
        theme.f32(theme.bg),
        DEPTH,
        (RADIUS - 1.0) * s,
        ORDER + 2,
    );

    let title = if view.query.is_empty() {
        view.title.to_string()
    } else {
        format!("{}  /{}", view.title, view.query)
    };
    sugarloaf.text_mut().draw(
        x + 14.0 * s,
        y + 8.0 * s,
        &title,
        &DrawOpts {
            font_size: 12.0 * s,
            color: theme.u8(theme.muted),
            bold: true,
            clip_rect: Some(header_clip),
            ..DrawOpts::default()
        },
    );

    let list_y = y + title_h;
    let list_clip = [x, list_y, width, visible_rows as f32 * row_h];
    let title_opts = DrawOpts {
        font_size: 14.0 * s,
        color: theme.u8(theme.fg),
        bold: true,
        clip_rect: Some(list_clip),
        ..DrawOpts::default()
    };
    let desc_opts = DrawOpts {
        font_size: 12.0 * s,
        color: theme.u8(theme.dim),
        clip_rect: Some(list_clip),
        ..DrawOpts::default()
    };
    let footer_opts = DrawOpts {
        font_size: 11.0 * s,
        color: theme.u8(theme.muted),
        clip_rect: Some(list_clip),
        ..DrawOpts::default()
    };
    let header_opts = DrawOpts {
        font_size: 13.5 * s,
        color: theme.u8(theme.cyan),
        bold: true,
        clip_rect: Some(list_clip),
        ..DrawOpts::default()
    };

    let list_bottom = list_y + visible_rows as f32 * row_h;
    let mut selected_cursor_rect = None;
    let overscan =
        ((view.list_scroll_offset.abs() / row_h).ceil() as usize).saturating_add(1);
    let start = first.saturating_sub(overscan);
    let end = (first + visible_rows + overscan).min(view.rows.len());
    // Snap the spring offsets to device-pixel boundaries before they
    // land in any row Y. Matches the same fix applied to the editor
    // `pixel_offset_y` and Finder's `list_scroll_offset` — sub-pixel
    // float positions push every glyph onto a slightly different
    // sub-pixel anchor per frame during continuous scroll, which the
    // eye integrates as smeared text. Snap once here so every row's
    // y inherits an integer pixel position.
    let list_scroll_snapped =
        crate::primitives::snap_to_device_px(view.list_scroll_offset * s, s);
    let cursor_offset_snapped =
        crate::primitives::snap_to_device_px(view.cursor_offset * s, s);
    for absolute_ix in start..end {
        let row = &view.rows[absolute_ix];
        let visible_ix = absolute_ix as isize - first as isize;
        let row_y = list_y + visible_ix as f32 * row_h + list_scroll_snapped;
        if row_y + row_h <= list_y || row_y >= list_bottom {
            continue;
        }
        let selected_row = absolute_ix == selected;
        if row.is_header {
            if row.title.is_empty() {
                continue;
            }
            let title_x = x + 22.0 * s;
            let title_text = truncate_to_pixel_width(
                sugarloaf,
                row.title,
                &header_opts,
                width - 44.0 * s,
            );
            sugarloaf.text_mut().draw(
                title_x,
                row_y + 10.0 * s,
                &title_text,
                &header_opts,
            );
            continue;
        }
        if selected_row {
            let selected_y = row_y + cursor_offset_snapped;
            let visible_y = selected_y.max(list_y);
            let visible_h = (selected_y + row_h).min(list_bottom) - visible_y;
            sugarloaf.rounded_rect(
                None,
                x + 6.0 * s,
                visible_y + 3.0 * s,
                width - 12.0 * s,
                (visible_h - 6.0 * s).max(0.0),
                theme.f32(theme.hover),
                DEPTH,
                9.0 * s,
                ORDER + 3,
            );
            sugarloaf.rounded_rect(
                None,
                x + 10.0 * s,
                visible_y + 9.0 * s,
                3.0 * s,
                (visible_h - 18.0 * s).max(0.0),
                theme.f32(theme.accent),
                DEPTH,
                2.0 * s,
                ORDER + 4,
            );
            let cursor_w = (14.0 * s * 0.6).max(2.0);
            let cursor_h = (row_h - 8.0 * s).max(14.0 * s).min(row_h);
            let cursor_x = (x + 18.0 * s - cursor_w - 2.0 * s).max(x + 6.0 * s);
            let cursor_y = (selected_y + (row_h - cursor_h) / 2.0)
                .clamp(list_y, (list_bottom - cursor_h).max(list_y));
            selected_cursor_rect = Some([cursor_x, cursor_y, cursor_w, cursor_h]);
        }
        // Current-session dot — small filled circle in accent color,
        // left-aligned in the 22 px gutter, independent of selection.
        if row.is_current {
            let dot_d = 6.0 * s;
            let dot_x = x + 7.0 * s;
            let dot_y = row_y + (row_h - dot_d) / 2.0;
            sugarloaf.rounded_rect(
                None,
                dot_x,
                dot_y,
                dot_d,
                dot_d,
                theme.f32(theme.accent),
                DEPTH,
                dot_d / 2.0,
                ORDER + 5,
            );
        }
        let title_x = x + 22.0 * s;
        let footer_w = if row.footer.is_empty() {
            0.0
        } else {
            sugarloaf.text_mut().measure(row.footer, &footer_opts) + 22.0 * s
        };
        let footer_x = x + width - footer_w;
        // Reserve a gap before the footer so long titles don't smear
        // through the time column; trim with an ellipsis when needed.
        let title_max_w = (footer_x - title_x - 14.0 * s).max(48.0);
        let title_text =
            truncate_to_pixel_width(sugarloaf, row.title, &title_opts, title_max_w);
        sugarloaf
            .text_mut()
            .draw(title_x, row_y + 7.0 * s, &title_text, &title_opts);
        if !row.description.is_empty() {
            let desc_x =
                title_x + sugarloaf.text_mut().measure(row.title, &title_opts) + 14.0 * s;
            if desc_x < footer_x - 12.0 * s {
                sugarloaf.text_mut().draw(
                    desc_x,
                    row_y + 8.0 * s,
                    row.description,
                    &desc_opts,
                );
            }
        }
        if !row.footer.is_empty() {
            sugarloaf.text_mut().draw(
                footer_x,
                row_y + 9.0 * s,
                row.footer,
                &footer_opts,
            );
        }
    }
    Some(InlinePickerRenderState {
        rect: [x, y, width, height],
        selected_cursor_rect,
    })
}
