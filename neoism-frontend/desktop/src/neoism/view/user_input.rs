pub(crate) use neoism_ui::panels::agent_pane::view::user_input::*;

use crate::neoism::agent::{
    NeoismAgentPane, NeoismAgentPendingPermission, NeoismAgentPermissionChoice,
    NeoismAgentStreamingState,
};

neoism_ui::neoism_ui_impl_agent_user_input!(
    NeoismAgentPane,
    NeoismAgentPendingPermission,
    NeoismAgentPermissionChoice,
    NeoismAgentStreamingState
);
