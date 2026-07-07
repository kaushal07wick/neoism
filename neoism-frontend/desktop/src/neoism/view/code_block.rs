pub(crate) use neoism_ui::panels::agent_pane::view::code_block::*;

use crate::neoism::agent::NeoismAgentMessage;

impl AgentCodeMessage for NeoismAgentMessage {
    fn id(&self) -> &str {
        &self.id
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn lang(&self) -> &str {
        &self.lang
    }

    fn line_offset(&self) -> Option<usize> {
        self.line_offset
    }
}
