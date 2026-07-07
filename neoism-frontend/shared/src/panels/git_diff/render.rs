use sugarloaf::text::DrawOpts;
use sugarloaf::Sugarloaf;

use crate::primitives::ide_theme::IdeTheme;
pub(super) use crate::primitives::{snap_to_device_px, truncate_to_fit};
use crate::widgets::diff_card::CardSpec;
use crate::widgets::{diff_card, scrollbar};

use super::state::GitDiffPanel;
use super::types::Rect;
use super::{
    CARD_GAP_TOP, CARD_PAD_X, CARD_VGAP, CLOSE_HIT, DEPTH, FILES_CARD_MAX_VISIBLE_ROWS,
    FILES_CARD_MIN_VISIBLE_ROWS, FILE_FONT_SIZE, FILE_ROW_HEIGHT, FRAME_RADIUS,
    FRAME_STROKE, GLYPH_BRANCH, GLYPH_CLOSE, HEADER_FONT_SIZE, HEADER_HEIGHT,
    ORDER_ACCENT, ORDER_FRAME, ORDER_INNER, ORDER_LINE_BG, ORDER_ROW_BG, ORDER_SCROLL,
    PADDING_X, SCROLLBAR_HIT_PAD, STATS_FONT_SIZE, STATS_HEIGHT,
};

impl GitDiffPanel {
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        window_w: f32,
        chrome_top: f32,
        bottom_y: f32,
        theme: &IdeTheme,
    ) {
        if !self.visible {
            self.panel_rect = Rect::ZERO;
            self.close_rect = Rect::ZERO;
            self.files_card_rect = Rect::ZERO;
            self.files_body_rect = Rect::ZERO;
            self.diff_card_rect = Rect::ZERO;
            self.file_row_rects.clear();
            self.selected_cursor_rect = None;
            return;
        }

        let s = self.scale;
        // Use the same width the chrome layout already reserved on
        // the right edge — `effective_width` honours the user-resized
        // `self.width` plus the window-relative cap.
        let target_w = self.effective_width(window_w);
        let height = (bottom_y - chrome_top).max(80.0);
        let open_progress = self.open_progress();
        let panel_x = window_w - target_w * open_progress;
        let panel_y = chrome_top;

        self.panel_rect = Rect {
            x: panel_x,
            y: panel_y,
            w: target_w,
            h: height,
        };

        let frame_stroke = (FRAME_STROKE * s).max(2.0);
        let frame_radius = FRAME_RADIUS * s;
        let inner_radius = (frame_radius - frame_stroke).max(0.0);

        // Frame: surface outer + bg inner — mirrors `file_tree::render`.
        sugarloaf.quad(
            None,
            panel_x,
            panel_y,
            target_w,
            height,
            theme.f32(theme.surface),
            [frame_radius, frame_radius, 0.0, 0.0],
            DEPTH,
            ORDER_FRAME,
        );
        sugarloaf.quad(
            None,
            panel_x + frame_stroke,
            panel_y + frame_stroke,
            (target_w - frame_stroke * 2.0).max(0.0),
            (height - frame_stroke).max(0.0),
            theme.f32(theme.bg),
            [inner_radius, inner_radius, 0.0, 0.0],
            DEPTH,
            ORDER_INNER,
        );

        let content_x = panel_x + frame_stroke;
        let content_y = panel_y + frame_stroke;
        let content_w = (target_w - frame_stroke * 2.0).max(0.0);
        let content_bottom = panel_y + height;
        let inner_x = content_x + PADDING_X * s;

        let mut cursor_y = content_y;

        // ── Top chrome: branch + close ───────────────────────────────
        let header_h = HEADER_HEIGHT * s;
        let header_clip = [content_x, cursor_y, content_w, header_h];
        let title_y = cursor_y + (header_h - HEADER_FONT_SIZE * s) / 2.0 - 1.0 * s;
        let title_opts = DrawOpts {
            font_size: HEADER_FONT_SIZE * s,
            color: theme.u8(theme.fg),
            bold: true,
            clip_rect: Some(header_clip),
            ..DrawOpts::default()
        };
        let icon_opts = DrawOpts {
            font_size: HEADER_FONT_SIZE * s,
            color: theme.u8(theme.dim),
            clip_rect: Some(header_clip),
            ..DrawOpts::default()
        };
        let muted_header_opts = DrawOpts {
            font_size: HEADER_FONT_SIZE * s,
            color: theme.u8(theme.muted),
            clip_rect: Some(header_clip),
            ..DrawOpts::default()
        };
        let branch_label = self
            .data
            .lock()
            .ok()
            .and_then(|d| d.branch.clone())
            .unwrap_or_default();

        let mut tx = inner_x;
        tx += sugarloaf
            .text_mut()
            .draw(tx, title_y, "Changes", &title_opts);
        if !branch_label.is_empty() {
            tx += 10.0 * s;
            tx += sugarloaf
                .text_mut()
                .draw(tx, title_y, GLYPH_BRANCH, &icon_opts);
            tx += 6.0 * s;
            let close_size = CLOSE_HIT * s;
            let title_budget =
                (content_x + content_w - tx - close_size - 12.0 * s).max(0.0);
            let branch_fit = truncate_to_fit(
                &branch_label,
                title_budget,
                sugarloaf,
                &muted_header_opts,
            );
            let _ = sugarloaf.text_mut().draw(
                tx,
                title_y,
                branch_fit.as_str(),
                &muted_header_opts,
            );
        }

        let close_size = CLOSE_HIT * s;
        let close_x = content_x + content_w - close_size - 6.0 * s;
        let close_y = cursor_y + (header_h - close_size) / 2.0;
        self.close_rect = Rect {
            x: close_x,
            y: close_y,
            w: close_size,
            h: close_size,
        };
        sugarloaf.rounded_rect(
            None,
            close_x,
            close_y,
            close_size,
            close_size,
            theme.f32(theme.hover),
            DEPTH,
            5.0 * s,
            ORDER_ROW_BG,
        );
        let close_opts = DrawOpts {
            font_size: 12.0 * s,
            color: theme.u8(theme.muted),
            bold: true,
            clip_rect: Some([close_x, close_y, close_size, close_size]),
            ..DrawOpts::default()
        };
        let cw = sugarloaf.text_mut().measure(GLYPH_CLOSE, &close_opts);
        sugarloaf.text_mut().draw(
            close_x + (close_size - cw) / 2.0,
            close_y + (close_size - 12.0 * s) / 2.0 - 1.0 * s,
            GLYPH_CLOSE,
            &close_opts,
        );
        cursor_y += header_h;

        // ── Stats row ────────────────────────────────────────────────
        let (loading, error, files, total_add, total_del, current_diff) = {
            let data = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let total_add: u32 = data.files.iter().map(|f| f.additions).sum();
            let total_del: u32 = data.files.iter().map(|f| f.deletions).sum();
            let current_diff = data
                .files
                .get(self.selected)
                .and_then(|f| data.diffs.get(&f.path).cloned());
            (
                data.loading,
                data.error.clone(),
                data.files.clone(),
                total_add,
                total_del,
                current_diff,
            )
        };

        let stats_h = STATS_HEIGHT * s;
        let stats_text_y = cursor_y + (stats_h - STATS_FONT_SIZE * s) / 2.0;
        let stats_clip = [content_x, cursor_y, content_w, stats_h];
        let muted_opts = DrawOpts {
            font_size: STATS_FONT_SIZE * s,
            color: theme.u8(theme.muted),
            clip_rect: Some(stats_clip),
            ..DrawOpts::default()
        };
        let add_opts = DrawOpts {
            font_size: STATS_FONT_SIZE * s,
            color: theme.u8(theme.green),
            bold: true,
            clip_rect: Some(stats_clip),
            ..DrawOpts::default()
        };
        let del_opts = DrawOpts {
            font_size: STATS_FONT_SIZE * s,
            color: theme.u8(theme.red),
            bold: true,
            clip_rect: Some(stats_clip),
            ..DrawOpts::default()
        };
        let files_text = format!(
            "{} {}",
            files.len(),
            if files.len() == 1 { "file" } else { "files" }
        );
        let mut sx = inner_x;
        sx +=
            sugarloaf
                .text_mut()
                .draw(sx, stats_text_y, files_text.as_str(), &muted_opts);
        sx += 10.0 * s;
        let add_text = format!("+{total_add}");
        sx += sugarloaf
            .text_mut()
            .draw(sx, stats_text_y, add_text.as_str(), &add_opts);
        sx += 8.0 * s;
        let del_text = format!("-{total_del}");
        let _ = sugarloaf
            .text_mut()
            .draw(sx, stats_text_y, del_text.as_str(), &del_opts);

        cursor_y += stats_h;

        sugarloaf.rect(
            None,
            content_x,
            cursor_y,
            content_w,
            (1.0 * s).max(1.0),
            theme.f32(theme.border),
            DEPTH,
            ORDER_ACCENT,
        );

        // Empty / error / loading branches.
        let body_top = cursor_y + (1.0 * s).max(1.0) + CARD_GAP_TOP * s;
        let body_h = (content_bottom - frame_stroke - body_top).max(0.0);
        if loading && files.is_empty() {
            let opts = DrawOpts {
                font_size: STATS_FONT_SIZE * s,
                color: theme.u8(theme.muted),
                clip_rect: Some([content_x, body_top, content_w, body_h]),
                ..DrawOpts::default()
            };
            sugarloaf
                .text_mut()
                .draw(inner_x, body_top + 12.0 * s, "Loading…", &opts);
            self.files_card_rect = Rect::ZERO;
            self.files_body_rect = Rect::ZERO;
            self.diff_card_rect = Rect::ZERO;
            self.file_row_rects.clear();
            self.selected_cursor_rect = None;
            return;
        }
        if let Some(err) = error.as_ref() {
            let opts = DrawOpts {
                font_size: STATS_FONT_SIZE * s,
                color: theme.u8(theme.red),
                clip_rect: Some([content_x, body_top, content_w, body_h]),
                ..DrawOpts::default()
            };
            sugarloaf
                .text_mut()
                .draw(inner_x, body_top + 12.0 * s, err.as_str(), &opts);
            self.files_card_rect = Rect::ZERO;
            self.files_body_rect = Rect::ZERO;
            self.diff_card_rect = Rect::ZERO;
            self.file_row_rects.clear();
            self.selected_cursor_rect = None;
            return;
        }
        if files.is_empty() {
            let opts = DrawOpts {
                font_size: STATS_FONT_SIZE * s,
                color: theme.u8(theme.muted),
                clip_rect: Some([content_x, body_top, content_w, body_h]),
                ..DrawOpts::default()
            };
            sugarloaf
                .text_mut()
                .draw(inner_x, body_top + 12.0 * s, "No changes", &opts);
            self.files_card_rect = Rect::ZERO;
            self.files_body_rect = Rect::ZERO;
            self.diff_card_rect = Rect::ZERO;
            self.file_row_rects.clear();
            self.selected_cursor_rect = None;
            return;
        }

        // ── Files card sizing ────────────────────────────────────────
        let card_x = content_x + CARD_PAD_X * s;
        let card_w = (content_w - CARD_PAD_X * 2.0 * s).max(0.0);
        let row_h = FILE_ROW_HEIGHT * s;
        let files_header_h = diff_card::HEADER_HEIGHT * s;
        let max_files_visible = files
            .len()
            .clamp(FILES_CARD_MIN_VISIBLE_ROWS, FILES_CARD_MAX_VISIBLE_ROWS);
        // Files card body: enough rows for `max_files_visible`, plus
        // its header. Diff card gets everything left over.
        let files_body_h = max_files_visible as f32 * row_h
            + (diff_card::BODY_TOP_PAD + diff_card::BODY_BOTTOM_PAD) * s;
        let files_card_h = files_header_h + files_body_h;
        let files_card_y = body_top;
        let diff_card_y = files_card_y + files_card_h + CARD_VGAP * s;
        let diff_card_h = (content_bottom - frame_stroke - diff_card_y).max(0.0);

        self.files_card_rect = Rect {
            x: card_x,
            y: files_card_y,
            w: card_w,
            h: files_card_h,
        };
        self.diff_card_rect = Rect {
            x: card_x,
            y: diff_card_y,
            w: card_w,
            h: diff_card_h,
        };

        // ── Files card chrome ────────────────────────────────────────
        let card_radius = diff_card::CARD_RADIUS * s;
        let card_stroke = (1.0 * s).max(1.0);
        // Border ring — slightly larger backing in `theme.border`,
        // then header + body fills draw on top, leaving a 1px stroke
        // around the whole card. Same trick `diff_card::render` uses
        // so the two cards read as a matched pair.
        sugarloaf.quad(
            None,
            card_x - card_stroke,
            files_card_y - card_stroke,
            card_w + card_stroke * 2.0,
            files_card_h + card_stroke * 2.0,
            theme.f32(theme.border),
            [
                card_radius + card_stroke,
                card_radius + card_stroke,
                card_radius + card_stroke,
                card_radius + card_stroke,
            ],
            DEPTH,
            ORDER_ROW_BG,
        );
        sugarloaf.quad(
            None,
            card_x,
            files_card_y,
            card_w,
            files_header_h,
            theme.f32(theme.surface),
            [card_radius, card_radius, 0.0, 0.0],
            DEPTH,
            ORDER_ROW_BG + 1,
        );
        sugarloaf.quad(
            None,
            card_x,
            files_card_y + files_header_h,
            card_w,
            files_body_h,
            theme.f32(theme.bg),
            [0.0, 0.0, card_radius, card_radius],
            DEPTH,
            ORDER_ROW_BG + 1,
        );

        let files_header_clip = [card_x, files_card_y, card_w, files_header_h];
        let files_title_opts = DrawOpts {
            font_size: diff_card::HEADER_FONT_SIZE * s,
            color: theme.u8(theme.fg),
            bold: true,
            clip_rect: Some(files_header_clip),
            ..DrawOpts::default()
        };
        let files_subtitle_opts = DrawOpts {
            font_size: diff_card::BADGE_FONT_SIZE * s,
            color: theme.u8(theme.muted),
            bold: true,
            clip_rect: Some(files_header_clip),
            ..DrawOpts::default()
        };
        let files_title_y = files_card_y
            + (files_header_h - diff_card::HEADER_FONT_SIZE * s) / 2.0
            - 1.0 * s;
        let mut hx = card_x + diff_card::HEADER_PAD_X * s;
        hx += sugarloaf
            .text_mut()
            .draw(hx, files_title_y, "Files", &files_title_opts);
        let count_text = format!("  {}", files.len());
        let _ = sugarloaf.text_mut().draw(
            hx,
            files_title_y,
            count_text.as_str(),
            &files_subtitle_opts,
        );

        // ── Files body rows ──────────────────────────────────────────
        let files_body_y = files_card_y + files_header_h;
        self.files_body_rect = Rect {
            x: card_x,
            y: files_body_y,
            w: card_w,
            h: files_body_h,
        };
        let files_body_inner_y = files_body_y + diff_card::BODY_TOP_PAD * s;
        let visible_rows = max_files_visible;
        let max_top = files.len().saturating_sub(visible_rows);
        let max_scroll = max_top as f32 * row_h;
        if self.file_scroll > max_scroll {
            self.file_scroll = max_scroll;
        }
        let scroll_offset =
            snap_to_device_px(self.tick_file_scroll(), sugarloaf.scale_factor());

        self.file_row_rects.clear();
        self.selected_cursor_rect = None;

        let row_clip_top = files_body_y;
        let row_clip_bot = files_body_y + files_body_h;

        let overscan = ((scroll_offset.abs() / row_h).ceil() as usize).saturating_add(1);
        let first_visible = (self.file_scroll / row_h) as usize;
        let start = first_visible.saturating_sub(overscan);
        let end = (first_visible + visible_rows + overscan).min(files.len());

        // Selected row backing first.
        let sel_row_y = files_body_inner_y
            + (self.selected as f32 * row_h - self.file_scroll)
            + scroll_offset;
        let sel_visible_y = sel_row_y.max(row_clip_top);
        let sel_visible_bot = (sel_row_y + row_h).min(row_clip_bot);
        let sel_visible_h = (sel_visible_bot - sel_visible_y).max(0.0);
        if sel_visible_h > 0.0 {
            sugarloaf.rect(
                None,
                card_x,
                sel_visible_y,
                card_w,
                sel_visible_h,
                theme.f32(theme.hover),
                DEPTH,
                ORDER_LINE_BG,
            );
            // Leading accent stripe (brighter when focused).
            let stripe_color = if self.focused {
                theme.f32(theme.accent)
            } else {
                theme.f32_alpha(theme.accent, 0.45)
            };
            sugarloaf.rect(
                None,
                card_x,
                sel_visible_y,
                (3.0 * s).max(2.0),
                sel_visible_h,
                stripe_color,
                DEPTH,
                ORDER_ACCENT,
            );
            // Cursor caret rect — small block on the leading edge so
            // the trail-cursor animation has a clear destination when
            // the panel takes focus, identical pattern to file_tree.
            if self.focused {
                let cursor_w = (FILE_FONT_SIZE * s * 0.55).max(2.0);
                let cursor_h = (row_h - 6.0 * s).max(FILE_FONT_SIZE * s).min(row_h);
                let cursor_y = (sel_row_y + (row_h - cursor_h) / 2.0)
                    .clamp(row_clip_top, (row_clip_bot - cursor_h).max(row_clip_top));
                let cursor_x = card_x + (3.0 * s).max(2.0) + 2.0 * s;
                self.selected_cursor_rect =
                    Some([cursor_x, cursor_y, cursor_w, cursor_h]);
            }
        }

        for absolute_ix in start..end {
            let f = &files[absolute_ix];
            let row_y = files_body_inner_y
                + (absolute_ix as f32 * row_h - self.file_scroll)
                + scroll_offset;
            let row_bot = row_y + row_h;
            if row_bot < row_clip_top || row_y > row_clip_bot {
                continue;
            }
            let visible_y = row_y.max(row_clip_top);
            let visible_h = row_bot.min(row_clip_bot) - visible_y;
            if visible_h <= 0.0 {
                continue;
            }
            self.file_row_rects.push((
                absolute_ix,
                Rect {
                    x: card_x,
                    y: visible_y,
                    w: card_w,
                    h: visible_h,
                },
            ));

            let is_selected = absolute_ix == self.selected;
            let row_clip = [card_x, visible_y, card_w, visible_h];
            let label_color = if is_selected {
                theme.u8(theme.fg)
            } else {
                theme.u8(theme.dim)
            };
            let marker_opts = DrawOpts {
                font_size: FILE_FONT_SIZE * s,
                color: f.status.color(theme),
                bold: true,
                clip_rect: Some(row_clip),
                ..DrawOpts::default()
            };
            let name_opts = DrawOpts {
                font_size: FILE_FONT_SIZE * s,
                color: label_color,
                clip_rect: Some(row_clip),
                ..DrawOpts::default()
            };
            let dir_opts = DrawOpts {
                font_size: FILE_FONT_SIZE * s,
                color: theme.u8(theme.muted),
                clip_rect: Some(row_clip),
                ..DrawOpts::default()
            };
            let row_add_opts = DrawOpts {
                font_size: STATS_FONT_SIZE * s,
                color: theme.u8(theme.green),
                clip_rect: Some(row_clip),
                ..DrawOpts::default()
            };
            let row_del_opts = DrawOpts {
                font_size: STATS_FONT_SIZE * s,
                color: theme.u8(theme.red),
                clip_rect: Some(row_clip),
                ..DrawOpts::default()
            };

            let text_y = row_y + (row_h - FILE_FONT_SIZE * s) / 2.0;
            let mut tx = card_x + diff_card::HEADER_PAD_X * s;
            tx += sugarloaf
                .text_mut()
                .draw(tx, text_y, f.status.marker(), &marker_opts);
            tx += 10.0 * s;

            let (filename, dir) = split_path(&f.path);
            let add_str = if f.additions > 0 {
                format!("+{}", f.additions)
            } else {
                String::new()
            };
            let del_str = if f.deletions > 0 {
                format!("-{}", f.deletions)
            } else {
                String::new()
            };
            let add_w = if add_str.is_empty() {
                0.0
            } else {
                sugarloaf
                    .text_mut()
                    .measure(add_str.as_str(), &row_add_opts)
            };
            let del_w = if del_str.is_empty() {
                0.0
            } else {
                sugarloaf
                    .text_mut()
                    .measure(del_str.as_str(), &row_del_opts)
            };
            let stats_total = add_w
                + del_w
                + if !add_str.is_empty() && !del_str.is_empty() {
                    6.0 * s
                } else {
                    0.0
                };
            let body_budget = (card_x + card_w
                - tx
                - diff_card::HEADER_PAD_X * s
                - stats_total
                - 8.0 * s)
                .max(0.0);

            let name_w = sugarloaf.text_mut().measure(filename, &name_opts);
            let name_used = name_w.min(body_budget * 0.6);
            let name_fit = if name_w <= name_used {
                filename.to_string()
            } else {
                truncate_to_fit(filename, name_used, sugarloaf, &name_opts)
            };
            tx += sugarloaf
                .text_mut()
                .draw(tx, text_y, name_fit.as_str(), &name_opts);
            if !dir.is_empty() {
                tx += 6.0 * s;
                let dir_budget = (card_x + card_w
                    - tx
                    - diff_card::HEADER_PAD_X * s
                    - stats_total
                    - 8.0 * s)
                    .max(0.0);
                let dir_fit = truncate_to_fit(dir, dir_budget, sugarloaf, &dir_opts);
                let _ =
                    sugarloaf
                        .text_mut()
                        .draw(tx, text_y, dir_fit.as_str(), &dir_opts);
            }

            let stats_x_right = card_x + card_w - diff_card::HEADER_PAD_X * s;
            let stats_text_y = row_y + (row_h - STATS_FONT_SIZE * s) / 2.0;
            let mut rx = stats_x_right;
            if !del_str.is_empty() {
                rx -= del_w;
                sugarloaf.text_mut().draw(
                    rx,
                    stats_text_y,
                    del_str.as_str(),
                    &row_del_opts,
                );
            }
            if !add_str.is_empty() {
                if !del_str.is_empty() {
                    rx -= 6.0 * s;
                }
                rx -= add_w;
                sugarloaf.text_mut().draw(
                    rx,
                    stats_text_y,
                    add_str.as_str(),
                    &row_add_opts,
                );
            }
        }

        // Files card scrollbar — record the thumb rect so the screen
        // layer's mouse-down handler can grab and drag it.
        self.files_scrollbar_thumb_rect = Rect::ZERO;
        if files.len() > visible_rows {
            let progress = if max_scroll > 0.0 {
                self.file_scroll / max_scroll
            } else {
                0.0
            };
            if let Some((thumb_y, thumb_h)) = scrollbar::compute_thumb(
                visible_rows,
                files.len(),
                files_body_y,
                files_body_h,
                progress,
            ) {
                let thumb_x = card_x + card_w - scrollbar::SCROLLBAR_WIDTH - 2.0 * s;
                scrollbar::draw_thumb(
                    sugarloaf,
                    thumb_x,
                    thumb_y,
                    thumb_h,
                    0.95,
                    false,
                    DEPTH,
                    ORDER_SCROLL,
                );
                self.files_scrollbar_thumb_rect = Rect {
                    x: thumb_x,
                    y: thumb_y,
                    w: scrollbar::SCROLLBAR_WIDTH,
                    h: thumb_h,
                };
            }
        }

        // ── Diff card ────────────────────────────────────────────────
        let selected_file = files.get(self.selected);
        if let Some(file) = selected_file {
            let lines = current_diff.unwrap_or_default();
            let lang = crate::syntax::Lang::from_path(&file.path);
            let body_capacity_h = (diff_card_h - diff_card::HEADER_HEIGHT * s).max(0.0);
            // Spring-damped scroll for the diff body. Clamp to the
            // last-line viewport so over-scroll can't flick the diff
            // off the bottom of the card.
            let line_h = diff_card::LINE_HEIGHT * s;
            let visual_row_offsets = diff_card::warm_render_cache(
                &lines,
                diff_card::body_text_width(card_w, s),
                s,
                lang,
            );
            let visual_line_count =
                visual_row_offsets.last().copied().unwrap_or(0).max(1);
            let max_diff_top = visual_line_count
                .saturating_sub(((body_capacity_h / line_h).floor() as usize).max(1))
                as f32
                * line_h;
            if self.diff_scroll > max_diff_top {
                self.diff_scroll = max_diff_top;
            }
            // Spring lag: `tick_diff_scroll` returns the position the
            // spring has yet to absorb. Right after a scroll it equals
            // the just-applied delta and decays back toward 0. To make
            // the body visually start at the old scroll and slide to
            // the new one, subtract the lag from the integer target.
            let diff_scroll_offset =
                snap_to_device_px(self.tick_diff_scroll(), sugarloaf.scale_factor());
            let effective_scroll = (self.diff_scroll - diff_scroll_offset).max(0.0);

            let spec = CardSpec {
                path: file.path.as_str(),
                link_target: None,
                link_hovered: false,
                additions: file.additions,
                deletions: file.deletions,
                lang,
                diff_lines: lines.as_slice(),
                visual_row_offsets: Some(visual_row_offsets.as_slice()),
                body_scroll: effective_scroll,
            };
            let _ = diff_card::render(
                sugarloaf,
                card_x,
                diff_card_y,
                card_w,
                body_capacity_h,
                &spec,
                s,
                theme,
                DEPTH,
                ORDER_ROW_BG,
                diff_card_y,
                diff_card_y + diff_card_h,
            );

            // Diff card scrollbar — same record-rect-for-drag pattern.
            self.diff_scrollbar_thumb_rect = Rect::ZERO;
            if max_diff_top > 0.0 {
                let progress = (self.diff_scroll / max_diff_top).clamp(0.0, 1.0);
                let visible_count = ((body_capacity_h / line_h).floor() as usize).max(1);
                if let Some((thumb_y, thumb_h)) = scrollbar::compute_thumb(
                    visible_count,
                    visual_line_count,
                    diff_card_y + diff_card::HEADER_HEIGHT * s,
                    body_capacity_h,
                    progress,
                ) {
                    let thumb_x = card_x + card_w - scrollbar::SCROLLBAR_WIDTH - 2.0 * s;
                    scrollbar::draw_thumb(
                        sugarloaf,
                        thumb_x,
                        thumb_y,
                        thumb_h,
                        0.95,
                        false,
                        DEPTH,
                        ORDER_SCROLL,
                    );
                    self.diff_scrollbar_thumb_rect = Rect {
                        x: thumb_x,
                        y: thumb_y,
                        w: scrollbar::SCROLLBAR_WIDTH,
                        h: thumb_h,
                    };
                }
            }
        } else {
            self.diff_scrollbar_thumb_rect = Rect::ZERO;
        }
    }
}

pub(super) fn hit_scrollbar_thumb(rect: &Rect, mx: f32, my: f32) -> bool {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return false;
    }
    // Pad the hit area horizontally so the user doesn't need
    // sub-pixel mouse precision to grab the thin scrollbar.
    mx >= rect.x - SCROLLBAR_HIT_PAD
        && mx <= rect.x + rect.w + SCROLLBAR_HIT_PAD
        && my >= rect.y
        && my <= rect.y + rect.h
}

pub(super) fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[i + 1..], &path[..i + 1]),
        None => (path, ""),
    }
}
