use std::path::Path;

use serde_json::{json, Map, Value};

pub(super) fn parse_markdown(
    path: &Path,
) -> anyhow::Result<(Map<String, Value>, String)> {
    let text = std::fs::read_to_string(path)?;
    if !text.starts_with("---") {
        return Ok((Map::new(), text));
    }
    let rest = text.strip_prefix("---").unwrap_or(&text);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let Some(index) = rest.find("\n---") else {
        return Ok((Map::new(), text));
    };
    let frontmatter = &rest[..index];
    let content = rest[index + "\n---".len()..]
        .strip_prefix('\n')
        .unwrap_or_default()
        .to_string();
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(frontmatter)?;
    let data = serde_json::to_value(yaml)?;
    Ok((data.as_object().cloned().unwrap_or_default(), content))
}

pub(super) fn parse_jsonc(text: &str) -> anyhow::Result<Value> {
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&strip_trailing_commas(
        &strip_json_comments(text),
    ))?)
}

fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            escaped = ch == '\\' && !escaped;
            if ch == '"' && !escaped {
                in_string = false;
            }
            if ch != '\\' {
                escaped = false;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            let _ = chars.next();
            let mut previous = '\0';
            for next in chars.by_ref() {
                if previous == '*' && next == '/' {
                    break;
                }
                previous = next;
            }
            continue;
        }

        out.push(ch);
    }
    out
}

fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            escaped = ch == '\\' && !escaped;
            if ch == '"' && !escaped {
                in_string = false;
            }
            if ch != '\\' {
                escaped = false;
            }
            out.push(ch);
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch == ',' {
            let mut clone = chars.clone();
            while matches!(clone.peek(), Some(next) if next.is_whitespace()) {
                let _ = clone.next();
            }
            if matches!(clone.peek(), Some('}' | ']')) {
                continue;
            }
        }
        out.push(ch);
    }
    out
}
