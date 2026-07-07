/// Geometry snapshot captured the moment the splash was injected
/// into a pane. Lets the GPU overlay anchor its effects to the
/// same cell rect the wordmark lives in.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct SplashInjection {
    /// Row index in the *terminal grid* (not absolute scrollback)
    /// where the wordmark's first row lives at injection time.
    pub wordmark_row: usize,
    /// Column index where the wordmark begins.
    pub wordmark_col: usize,
    /// Wordmark cell width and height — used to compute pixel
    /// extents from cell coords later.
    pub wordmark_cells_w: usize,
    pub wordmark_cells_h: usize,
    /// Cell rows of breathing room between the wordmark band
    /// and the menu band. Captured at inject time because the
    /// adaptive layout might use a smaller value than the
    /// `WORDMARK_TO_MENU_GAP_ROWS` const on a small pane.
    pub gap_cells_h: usize,
    /// Cell rows reserved for the menu buttons. Same reason —
    /// adaptive layout can shrink this on small panes.
    pub menu_cells_h: usize,
    /// Cursor row immediately after the splash bytes were
    /// flushed into the terminal — i.e. the row the shell will
    /// land its prompt on. Used to detect the FIRST `Enter`
    /// press: when the live cursor row exceeds this value, the
    /// user has submitted a command and the dismiss animation
    /// should kick off (the shell's `\r\n` advances cursor.row
    /// by 1 the moment Enter is hit, well before output reaches
    /// the splash).
    pub baseline_cursor_row: i32,
}
