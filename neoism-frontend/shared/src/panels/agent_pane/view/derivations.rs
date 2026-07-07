use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollFrameDerivations {
    pub markdown_layouts: u32,
    pub tool_diff_sections: u32,
    pub tool_wraps: u32,
    pub diff_wraps: u32,
    pub diff_highlights: u32,
    pub code_line_ranges: u32,
    pub code_highlights: u32,
    pub message_clones: u32,
}

impl ScrollFrameDerivations {
    pub fn total(self) -> u32 {
        self.markdown_layouts
            + self.tool_diff_sections
            + self.tool_wraps
            + self.diff_wraps
            + self.diff_highlights
            + self.code_line_ranges
            + self.code_highlights
            + self.message_clones
    }
}

thread_local! {
    static FRAME_DERIVATIONS: Cell<ScrollFrameDerivations> =
        Cell::new(ScrollFrameDerivations::default());
}

pub fn reset() {
    FRAME_DERIVATIONS.with(|counts| counts.set(ScrollFrameDerivations::default()));
}

pub fn take() -> ScrollFrameDerivations {
    FRAME_DERIVATIONS.with(|counts| {
        let value = counts.get();
        counts.set(ScrollFrameDerivations::default());
        value
    })
}

pub fn bump_markdown_layout() {
    bump(|counts| counts.markdown_layouts = counts.markdown_layouts.saturating_add(1));
}

pub fn bump_tool_diff_sections() {
    bump(|counts| {
        counts.tool_diff_sections = counts.tool_diff_sections.saturating_add(1)
    });
}

pub fn bump_tool_wrap() {
    bump(|counts| counts.tool_wraps = counts.tool_wraps.saturating_add(1));
}

pub fn bump_diff_wrap() {
    bump(|counts| counts.diff_wraps = counts.diff_wraps.saturating_add(1));
}

pub fn bump_diff_highlight() {
    bump(|counts| counts.diff_highlights = counts.diff_highlights.saturating_add(1));
}

pub fn bump_code_line_range() {
    bump(|counts| counts.code_line_ranges = counts.code_line_ranges.saturating_add(1));
}

pub fn bump_code_highlight() {
    bump(|counts| counts.code_highlights = counts.code_highlights.saturating_add(1));
}

pub fn bump_message_clone() {
    bump(|counts| counts.message_clones = counts.message_clones.saturating_add(1));
}

fn bump(update: impl FnOnce(&mut ScrollFrameDerivations)) {
    FRAME_DERIVATIONS.with(|counts| {
        let mut value = counts.get();
        update(&mut value);
        counts.set(value);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_counters_reset_and_take() {
        reset();
        bump_markdown_layout();
        bump_diff_wrap();
        bump_diff_wrap();
        bump_message_clone();

        let counts = take();
        assert_eq!(counts.markdown_layouts, 1);
        assert_eq!(counts.diff_wraps, 2);
        assert_eq!(counts.message_clones, 1);
        assert_eq!(counts.total(), 4);
        assert_eq!(take().total(), 0);
    }
}
