use super::*;

impl Renderer {
    /// Find hint label at the specified position
    #[allow(dead_code)]
    pub(super) fn find_hint_label_at_position<'a>(
        &self,
        renderable_content: &'a RenderableContent,
        pos: Pos,
    ) -> Option<&'a crate::context::renderable::HintLabel> {
        renderable_content
            .hint_labels
            .iter()
            .find(|label| label.position == pos)
    }
}
