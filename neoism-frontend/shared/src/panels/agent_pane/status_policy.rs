//! Shared status/update policies for the agent pane.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStatusDecision {
    pub count: usize,
    pub preview: Option<String>,
    pub should_enter_thinking: bool,
    pub started_at: Option<u64>,
}

pub fn queue_status_decision(
    count: usize,
    preview: Option<String>,
    started_at: Option<u64>,
    is_streaming: bool,
) -> QueueStatusDecision {
    let (count, preview) = if started_at.is_some() {
        (count, preview)
    } else {
        (0, None)
    };
    QueueStatusDecision {
        count,
        preview,
        should_enter_thinking: started_at.is_some() && !is_streaming,
        started_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_queue_status_clears_count_and_preview() {
        assert_eq!(
            queue_status_decision(3, Some("queued prompt".to_string()), None, false),
            QueueStatusDecision {
                count: 0,
                preview: None,
                should_enter_thinking: false,
                started_at: None,
            }
        );
    }

    #[test]
    fn active_queue_status_preserves_prompt_and_enters_thinking_when_idle() {
        assert_eq!(
            queue_status_decision(2, Some("next prompt".to_string()), Some(1234), false),
            QueueStatusDecision {
                count: 2,
                preview: Some("next prompt".to_string()),
                should_enter_thinking: true,
                started_at: Some(1234),
            }
        );
    }

    #[test]
    fn active_queue_status_does_not_replace_existing_streaming_state() {
        let decision = queue_status_decision(1, None, Some(1234), true);
        assert_eq!(decision.count, 1);
        assert!(!decision.should_enter_thinking);
    }
}
