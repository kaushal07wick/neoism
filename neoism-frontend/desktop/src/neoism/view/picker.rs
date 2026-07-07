pub(crate) use neoism_ui::panels::agent_pane::view::picker::*;

use neoism_ui::panels::agent_pane::state::picker::NeoismAgentPicker;

use crate::neoism::agent::NeoismAgentPane;

impl AgentPickerPane for NeoismAgentPane {
    fn picker_mut(&mut self) -> Option<&mut NeoismAgentPicker> {
        self.picker_mut()
    }

    fn picker_rename_buffer(&self) -> Option<String> {
        self.session_rename_buffer()
    }
}
