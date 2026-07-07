use crate::neoism::agent::{NeoismAgentPane, NeoismWordmarkState};

neoism_ui::neoism_ui_impl_wordmark_state!(NeoismWordmarkState, std::time::Instant::now);
neoism_ui::neoism_ui_impl_agent_home_pane!(NeoismAgentPane, NeoismWordmarkState);
