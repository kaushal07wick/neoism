use neoism_ui::panels::buffer_tabs::{
    apply_buffer_tab_policy, BufferTabPolicyInput, BufferTabPolicyOperation,
};

pub(crate) fn select_relative_index(
    len: usize,
    active: usize,
    previous: bool,
) -> Option<usize> {
    let operation = if previous {
        BufferTabPolicyOperation::SelectPrevious
    } else {
        BufferTabPolicyOperation::SelectNext
    };
    let result = apply_buffer_tab_policy(
        BufferTabPolicyInput {
            len,
            active,
            closeable: Vec::new(),
        },
        operation,
    );
    result.changed.then_some(result.active)
}
