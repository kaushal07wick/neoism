use sugarloaf::Sugarloaf;

use crate::panels::agent_pane::state::picker::NeoismAgentPicker;
use crate::panels::agent_pane::state::NeoismAgentPane;

use crate::primitives::ide_theme::IdeTheme;
use crate::widgets::inline_picker::{InlinePickerRow, InlinePickerView};

pub trait AgentPickerPane {
    fn picker_mut(&mut self) -> Option<&mut NeoismAgentPicker>;
}

impl AgentPickerPane for NeoismAgentPane {
    fn picker_mut(&mut self) -> Option<&mut NeoismAgentPicker> {
        self.picker_mut()
    }
}

pub fn render_picker(
    sugarloaf: &mut Sugarloaf,
    pane: &mut impl AgentPickerPane,
    input_rect: [f32; 4],
    theme: &IdeTheme,
    s: f32,
) {
    let Some(picker) = pane.picker_mut() else {
        return;
    };
    let list_scroll_offset = picker.tick_list_scroll();
    let cursor_offset = picker.tick_cursor();
    let rows = picker
        .options()
        .iter()
        .map(|option| InlinePickerRow {
            title: &option.title,
            description: &option.description,
            footer: &option.footer,
            is_header: option.is_header,
            is_current: option.is_current,
        })
        .collect::<Vec<_>>();
    if let Some(render_state) = crate::widgets::inline_picker::render(
        sugarloaf,
        InlinePickerView {
            title: &picker.title,
            query: &picker.query,
            selected: picker.selected,
            scroll_offset: picker.scroll_offset,
            list_scroll_offset,
            cursor_offset,
            rows: &rows,
        },
        input_rect,
        theme,
        s,
    ) {
        picker.set_last_rect(render_state.rect);
        // cursor rect is intentionally NOT updated here — the caret stays
        // in the input text area while the picker dropdown is visible.
    }
}
