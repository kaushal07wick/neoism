use serde_json::Value;

use crate::{BOLD, CYAN, DIM, GREEN, ORANGE, RESET};

pub(crate) fn print_session_queue(value: &Value) -> anyhow::Result<()> {
    let count = queue_count(value).unwrap_or(0);
    let running = value
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let worker = value
        .get("worker")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if running {
        format!("{ORANGE}running{RESET}")
    } else if worker {
        format!("{ORANGE}draining{RESET}")
    } else {
        format!("{GREEN}idle{RESET}")
    };
    println!("{BOLD}queue{RESET} {DIM}{count} pending · {status}{RESET}");
    let Some(items) = value.get("items").and_then(Value::as_array) else {
        return Ok(());
    };
    if items.is_empty() {
        println!("  {DIM}empty{RESET}");
        return Ok(());
    }
    for item in items {
        let index = item.get("index").and_then(Value::as_u64).unwrap_or(0) + 1;
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("(non-text prompt)");
        let mut suffix = String::new();
        if let Some(agent) = item.get("agent").and_then(Value::as_str) {
            suffix.push_str(&format!(" {DIM}agent {agent}{RESET}"));
        }
        if item
            .get("noReply")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            suffix.push_str(&format!(" {DIM}no-reply{RESET}"));
        }
        println!("  {CYAN}{index:>2}.{RESET} {text}{suffix}");
    }
    Ok(())
}

pub(crate) fn print_queue_mutation(action: &str, value: &Value) -> anyhow::Result<()> {
    let removed = value.get("removed").and_then(Value::as_u64).unwrap_or(0);
    let queue = value.get("queue").unwrap_or(&Value::Null);
    let count = queue_count(queue).unwrap_or(0);
    println!("{action} {removed} queued turn(s); {count} remaining");
    if count > 0 {
        print_session_queue(queue)?;
    }
    Ok(())
}

pub(crate) fn print_pending_permissions(
    value: &Value,
    session_id: &str,
) -> anyhow::Result<()> {
    let items = session_items(value, session_id);
    println!(
        "{BOLD}permissions{RESET} {DIM}{} pending{RESET}",
        items.len()
    );
    if items.is_empty() {
        println!("  {DIM}empty{RESET}");
        return Ok(());
    }
    for item in items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("unknown");
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Permission required");
        let patterns = item
            .get("patterns")
            .and_then(Value::as_array)
            .map(|patterns| {
                patterns
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|patterns| !patterns.is_empty())
            .unwrap_or_default();
        if patterns.is_empty() {
            println!("  {CYAN}{id}{RESET} {title}");
        } else {
            println!("  {CYAN}{id}{RESET} {title} {DIM}{patterns}{RESET}");
        }
    }
    Ok(())
}

pub(crate) fn print_pending_questions(
    value: &Value,
    session_id: &str,
) -> anyhow::Result<()> {
    let items = session_items(value, session_id);
    println!("{BOLD}questions{RESET} {DIM}{} pending{RESET}", items.len());
    if items.is_empty() {
        println!("  {DIM}empty{RESET}");
        return Ok(());
    }
    for item in items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("unknown");
        let label =
            question_label(item).unwrap_or_else(|| "Question required".to_string());
        println!("  {CYAN}{id}{RESET} {label}");
    }
    Ok(())
}

pub(crate) fn first_pending_id(value: &Value, session_id: &str) -> Option<String> {
    session_items(value, session_id)
        .into_iter()
        .find_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

pub(crate) fn pending_id_or_first(
    value: &Value,
    session_id: &str,
    requested: Option<&str>,
) -> Option<String> {
    let items = session_items(value, session_id);
    if let Some(requested) = requested {
        return items.into_iter().find_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            (id == requested).then(|| id.to_string())
        });
    }
    items.into_iter().find_map(|item| {
        item.get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

pub(crate) fn permission_reply_alias(value: &str) -> &'static str {
    match value.to_ascii_lowercase().as_str() {
        "a" | "always" => "always",
        "n" | "no" | "reject" => "reject",
        _ => "once",
    }
}

pub(crate) fn queue_count(queue: &Value) -> Option<usize> {
    queue
        .get("count")
        .and_then(Value::as_u64)
        .map(|count| count as usize)
}

pub(crate) fn status_queue_count(statuses: &Value, session_id: &str) -> usize {
    statuses
        .get(session_id)
        .and_then(|status| status.get("queue"))
        .and_then(|queue| queue.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

pub(crate) fn session_items<'a>(value: &'a Value, session_id: &str) -> Vec<&'a Value> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item_session_matches(item, session_id))
        .collect()
}

fn item_session_matches(item: &Value, session_id: &str) -> bool {
    item.get("sessionID")
        .or_else(|| item.get("sessionId"))
        .and_then(Value::as_str)
        == Some(session_id)
}

pub(crate) fn question_label(question: &Value) -> Option<String> {
    question
        .get("questions")
        .and_then(Value::as_array)
        .and_then(|questions| questions.first())
        .and_then(|question| {
            question
                .get("question")
                .or_else(|| question.get("label"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
}

pub(crate) fn first_queue_preview(queue: &Value) -> Option<String> {
    queue
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            let mut preview = text.chars().take(72).collect::<String>();
            if text.chars().count() > 72 {
                preview.push_str("...");
            }
            preview
        })
}
