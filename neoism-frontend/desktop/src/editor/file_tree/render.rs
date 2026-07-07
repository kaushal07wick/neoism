use neoism_backend::sugarloaf::Sugarloaf;
use neoism_ui::primitives::ide_theme::IdeTheme;

use super::state::FileTree;

impl FileTree {
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        y_top: f32,
        panel_height: f32,
        theme: &IdeTheme,
        text_occlusion_rects: &[[f32; 4]],
    ) {
        self.inner.render(
            sugarloaf,
            0.0,
            y_top,
            self.inner.width(),
            panel_height,
            theme,
            text_occlusion_rects,
        );
    }
}
