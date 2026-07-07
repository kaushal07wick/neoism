use super::border::PanelBorder;

/// Active resize drag state
#[derive(Debug, Clone, Copy)]
pub struct ResizeState {
    pub border: PanelBorder,
    /// Mouse position at drag start (physical pixels)
    pub start_pos: f32,
}
