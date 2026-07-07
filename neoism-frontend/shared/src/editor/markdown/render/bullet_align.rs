//! Pure baseline arithmetic for markdown list markers (`-`, `*`, `+`, `1.`).
//!
//! Modern web markdown renderers center the bullet glyph on the **x-height**
//! of the first text line — not the geometric middle of the row box and not
//! the baseline. Anything else feels visually "low" because the EM-box has
//! more descender room than ascender, and the row box has padding the text
//! doesn't fill.
//!
//! These helpers are POD: given font metrics (font size in px) and the
//! `text_y` where the renderer is about to draw the glyph's EM-box top, they
//! return the y the bullet/marker should be drawn at. Callers must already
//! know `text_y`, which the markdown renderer in `render/mod.rs` computes
//! from `cursor_y`, `row_h`, and the wrapped text height.
//!
//! ## Font metrics model
//!
//! We approximate generic-font metrics with the ratios used by the rest of
//! the codebase (see `widgets::markdown::line_height` which assumes
//! `1.48 * font_size`). For a typical sans-serif:
//!
//! - ascender ≈ `0.78 * font_size`  (top of EM-box → baseline)
//! - x-height ≈ `0.52 * font_size`  (baseline → top of lowercase 'x')
//! - midline-of-x ≈ baseline − x_height / 2 = `top + 0.78*fs − 0.26*fs` = `top + 0.52*fs`
//!
//! So the x-height visual center of a text line drawn at `text_y` sits at
//! `text_y + X_HEIGHT_CENTER_RATIO * font_size`. We use 0.52 as a single
//! constant — it matches what the eye reads as "centered with text" for the
//! Inter/JetBrainsMono/Maple-class fonts neoism ships.

/// Ratio between EM-box top and the visual midline of lowercase glyphs.
///
/// Derived from ascender ≈ 0.78 · font_size and x-height ≈ 0.52 · font_size
/// for typical sans/mono fonts. See module docs.
pub const X_HEIGHT_CENTER_RATIO: f32 = 0.52;

/// Y-coordinate of the visual x-height center of a text line whose EM-box
/// top is at `text_y` and whose font size is `font_size`.
#[inline]
pub fn x_height_center_y(text_y: f32, font_size: f32) -> f32 {
    text_y + font_size * X_HEIGHT_CENTER_RATIO
}

/// For a non-text bullet glyph (filled dot/square of height `glyph_h`), the
/// y to pass to a rect-draw call so that the dot's geometric center sits on
/// the text's x-height midline.
#[inline]
pub fn bullet_dot_y(text_y: f32, font_size: f32, glyph_h: f32) -> f32 {
    x_height_center_y(text_y, font_size) - glyph_h * 0.5
}

/// For a text-glyph marker (`1.`, `1)`, `a.`) drawn at `marker_font_size`
/// next to body text drawn at `text_font_size`, the y to pass to
/// `text.draw(x, y, ...)` so the marker's x-height midline aligns with the
/// body text's x-height midline.
///
/// `text_y` is where the body text is being drawn (EM-box top). The result
/// shifts by half the difference in EM-box ascender so both midlines coincide.
#[inline]
pub fn text_marker_y(text_y: f32, text_font_size: f32, marker_font_size: f32) -> f32 {
    // body midline: text_y + R * text_fs
    // marker midline: marker_y + R * marker_fs
    // Solve: marker_y = text_y + R * (text_fs - marker_fs)
    text_y + X_HEIGHT_CENTER_RATIO * (text_font_size - marker_font_size)
}

/// For a square box widget (task checkbox) drawn next to body text, the y
/// to pass to the rect/box draw so the box is vertically centered on the
/// text's x-height midline.
#[inline]
pub fn checkbox_y(text_y: f32, font_size: f32, box_size: f32) -> f32 {
    x_height_center_y(text_y, font_size) - box_size * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!(
            (a - b).abs() < 1e-3,
            "expected {a} ≈ {b} (delta {})",
            (a - b).abs()
        );
    }

    #[test]
    fn x_height_center_matches_documented_ratio() {
        // For font_size 17, midline sits at 8.84 px below top.
        approx(x_height_center_y(0.0, 17.0), 8.84);
        // Linearity: shifting text_y shifts the midline by the same amount.
        approx(x_height_center_y(100.0, 17.0), 108.84);
    }

    #[test]
    fn bullet_dot_center_lands_on_x_height_midline() {
        // A 5px dot centered on the x-height midline of a 17-px text line.
        let text_y = 50.0;
        let fs = 17.0;
        let glyph_h = 5.0;
        let y = bullet_dot_y(text_y, fs, glyph_h);
        // Dot's geometric center = y + glyph_h/2; should equal midline.
        approx(y + glyph_h * 0.5, x_height_center_y(text_y, fs));
    }

    #[test]
    fn text_marker_midline_matches_text_midline_when_sizes_equal() {
        // Same font size → no shift.
        let text_y = 30.0;
        let fs = 16.0;
        approx(text_marker_y(text_y, fs, fs), text_y);
    }

    #[test]
    fn text_marker_midline_matches_text_midline_when_sizes_differ() {
        // text=17, marker=14 (ordered list real values).
        let text_y = 100.0;
        let text_fs = 17.0;
        let marker_fs = 14.0;
        let marker_y = text_marker_y(text_y, text_fs, marker_fs);
        // The two midlines should coincide.
        approx(
            x_height_center_y(marker_y, marker_fs),
            x_height_center_y(text_y, text_fs),
        );
    }

    #[test]
    fn text_marker_with_larger_marker_shifts_up() {
        // Marker bigger than text → marker_y above text_y so midlines meet.
        let marker_y = text_marker_y(50.0, 12.0, 18.0);
        assert!(
            marker_y < 50.0,
            "marker should sit above text_y, got {marker_y}"
        );
    }

    #[test]
    fn checkbox_y_centers_box_on_x_height() {
        let text_y = 80.0;
        let fs = 16.0;
        let size = 15.0;
        let y = checkbox_y(text_y, fs, size);
        approx(y + size * 0.5, x_height_center_y(text_y, fs));
    }

    #[test]
    fn helpers_are_pure_and_deterministic() {
        // Two identical inputs → identical outputs (no float weirdness).
        let a = text_marker_y(12.5, 17.0, 14.0);
        let b = text_marker_y(12.5, 17.0, 14.0);
        assert_eq!(a, b);
        let a = bullet_dot_y(7.0, 17.0, 5.0);
        let b = bullet_dot_y(7.0, 17.0, 5.0);
        assert_eq!(a, b);
    }

    #[test]
    fn font_size_zero_does_not_panic() {
        // Defensive: at font_size 0 the midline is at text_y; bullet sits centered on it.
        approx(x_height_center_y(20.0, 0.0), 20.0);
        approx(bullet_dot_y(20.0, 0.0, 6.0), 17.0);
        approx(text_marker_y(20.0, 0.0, 0.0), 20.0);
    }
}
