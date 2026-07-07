use serde_json::json;

use crate::chat_blockers::{is_permission_reply, parse_permission_reply_and_id};
use crate::chat_commands::{command_matches_query, CHAT_COMMANDS};
use crate::chat_header::should_attach_existing_stream;
use crate::chat_session::context_usage_label;
use crate::chat_status::{
    first_pending_id, first_queue_preview, pending_id_or_first, permission_reply_alias,
    question_label, queue_count, session_items, status_queue_count,
};

#[test]
fn queue_command_is_searchable() {
    let queue = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/queue"))
        .expect("queue command should be registered");
    assert!(command_matches_query(queue, "queue"));
    assert!(
        CHAT_COMMANDS
            .iter()
            .all(|spec| !spec.names.contains(&"/resume")),
        "/resume should stay removed; /sessions owns session switching"
    );
    let sessions = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/sessions"))
        .expect("sessions command should be registered");
    assert!(command_matches_query(sessions, "ses"));
    let compact = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/compact"))
        .expect("compact command should be registered");
    assert!(command_matches_query(compact, "compact"));
    let subagent = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/sub-agent"))
        .expect("sub-agent command should be registered");
    assert!(command_matches_query(subagent, "subagent"));
    let permissions = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/permissions"))
        .expect("permissions command should be registered");
    assert!(command_matches_query(permissions, "permission"));
    let permit = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/permit"))
        .expect("permit command should be registered");
    assert!(command_matches_query(permit, "permission"));
    let questions = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/questions"))
        .expect("questions command should be registered");
    assert!(command_matches_query(questions, "question"));
    let answer = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/answer"))
        .expect("answer command should be registered");
    assert!(command_matches_query(answer, "answer"));
    let reject = CHAT_COMMANDS
        .iter()
        .find(|spec| spec.names.contains(&"/reject"))
        .expect("reject command should be registered");
    assert!(command_matches_query(reject, "reject"));
}

#[test]
fn context_usage_label_uses_latest_assistant_and_model_limit() {
    let messages = vec![
        json!({
            "info": {
                "role": "assistant",
                "time": { "created": 1, "completed": 2 },
                "providerId": "openai",
                "modelId": "gpt-5.5",
                "tokens": {
                    "input": 20_000,
                    "output": 4_000,
                    "reasoning": 2_000,
                    "cache": { "read": 10_000, "write": 0 }
                }
            }
        }),
        json!({
            "info": {
                "role": "assistant",
                "time": { "created": 3, "completed": 4 },
                "providerId": "openai",
                "modelId": "gpt-5.5",
                "tokens": {
                    "input": 40_000,
                    "output": 8_000,
                    "reasoning": 4_000,
                    "cache": { "read": 12_000, "write": 0 }
                }
            }
        }),
    ];
    let providers = json!({
        "all": [{
            "id": "openai",
            "models": {
                "gpt-5.5": {
                    "limit": {
                        "context": 1_000_000,
                        "input": 400_000,
                        "output": 128_000
                    }
                }
            }
        }]
    });

    assert_eq!(
        context_usage_label(&messages, &providers).as_deref(),
        Some("ctx gpt-5.5 16% 64k/400k")
    );
}

#[test]
fn bootstrap_helpers_extract_session_queue_and_blockers() {
    let session_id = "ses_test";
    let queue = json!({
        "count": 2,
        "items": [
            { "text": "first queued turn with enough text to preview", "index": 0 },
            { "text": "second", "index": 1 }
        ]
    });
    let statuses = json!({
        session_id: {
            "type": "busy",
            "queue": { "count": 2, "preview": "first queued turn" }
        }
    });
    let permissions = json!([
        { "id": "perm_1", "sessionId": session_id, "title": "Allow bash?" },
        { "id": "perm_2", "sessionId": "other", "title": "Ignore" }
    ]);
    let questions = json!([
        {
            "id": "q_1",
            "sessionId": session_id,
            "questions": [{ "question": "Pick a file?" }]
        }
    ]);

    assert_eq!(queue_count(&queue), Some(2));
    assert_eq!(status_queue_count(&statuses, session_id), 2);
    assert_eq!(
        first_queue_preview(&queue).as_deref(),
        Some("first queued turn with enough text to preview")
    );
    assert_eq!(session_items(&permissions, session_id).len(), 1);
    let pending_questions = session_items(&questions, session_id);
    assert_eq!(pending_questions.len(), 1);
    assert_eq!(
        question_label(pending_questions[0]).as_deref(),
        Some("Pick a file?")
    );
}

#[test]
fn bootstrap_attach_waits_only_for_unblocked_active_streams() {
    assert!(should_attach_existing_stream("busy", true, false, false));
    assert!(should_attach_existing_stream("idle", false, true, false));
    assert!(!should_attach_existing_stream("idle", false, false, false));
    assert!(!should_attach_existing_stream("busy", true, false, true));
}

#[test]
fn pending_request_helpers_pick_session_items() {
    let permissions = json!([
        { "id": "perm_1", "sessionId": "ses_a", "title": "Allow bash?" },
        { "id": "perm_2", "sessionId": "ses_b", "title": "Ignore" }
    ]);
    assert_eq!(
        first_pending_id(&permissions, "ses_a").as_deref(),
        Some("perm_1")
    );
    assert_eq!(
        pending_id_or_first(&permissions, "ses_a", Some("perm_1")).as_deref(),
        Some("perm_1")
    );
    assert_eq!(
        pending_id_or_first(&permissions, "ses_a", Some("perm_2")),
        None
    );
    assert_eq!(permission_reply_alias("a"), "always");
    assert_eq!(permission_reply_alias("n"), "reject");
    assert_eq!(permission_reply_alias("y"), "once");
    assert_eq!(permission_reply_alias("Always"), "always");
    assert!(is_permission_reply("yes"));
    assert_eq!(
        parse_permission_reply_and_id(Some("always"), Some("perm_1")),
        ("always", Some("perm_1"))
    );
    assert_eq!(
        parse_permission_reply_and_id(Some("perm_2"), None),
        ("once", Some("perm_2"))
    );
}
