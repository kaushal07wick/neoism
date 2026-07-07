use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFileMention {
    pub anchor: usize,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedFileMention {
    pub input: String,
    pub cursor: usize,
    pub token: String,
}

pub fn compact_directory_label(path: &str) -> String {
    let home = std::env::var("HOME").ok();
    compact_directory_label_with_home(path, home.as_deref())
}

pub fn compact_directory_label_with_home(path: &str, home: Option<&str>) -> String {
    let mut label = path.trim().replace('\\', "/");
    if label.is_empty() {
        return "-".to_string();
    }
    if let Some(home) = home {
        let home = home.trim_end_matches('/').replace('\\', "/");
        if label == home {
            label = "~".to_string();
        } else if let Some(rest) = label.strip_prefix(&format!("{home}/")) {
            label = format!("~/{rest}");
        }
    }
    if !label.ends_with('/') {
        label.push('/');
    }
    if label.chars().count() <= 44 {
        return label;
    }
    let trimmed = label.trim_end_matches('/');
    let parts = trimmed
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 3 {
        return label;
    }
    let tail_start = parts.len().saturating_sub(2);
    let tail = parts[tail_start..].join("/");
    if trimmed.starts_with("~/") {
        format!("~/.../{tail}/")
    } else if trimmed.starts_with('/') {
        format!("/.../{tail}/")
    } else {
        format!("{}/.../{tail}/", parts[0])
    }
}

pub fn display_path_for_attachment(root: &Path, path: &Path, is_dir: bool) -> String {
    let mut display = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if is_dir && !display.ends_with('/') {
        display.push('/');
    }
    display
}

pub fn file_attachment_token(filename: &str, mime: &str, next_index: usize) -> String {
    if mime.starts_with("image/") {
        return format!("[image{next_index}]");
    }
    if mime == "application/pdf" {
        return format!("[pdf{next_index}]");
    }
    format!("[file{next_index}: {filename}]")
}

pub fn unique_attachment_token<'a, I>(
    input: &str,
    existing_tokens: I,
    base: &str,
) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let existing_tokens = existing_tokens.into_iter().collect::<Vec<_>>();
    if !input.contains(base) && !existing_tokens.iter().any(|token| *token == base) {
        return base.to_string();
    }
    let stem = base.strip_suffix(']').unwrap_or(base);
    for index in 2.. {
        let candidate = if base.ends_with(']') {
            format!("{stem} #{index}]")
        } else {
            format!("{base} #{index}")
        };
        if !input.contains(&candidate)
            && !existing_tokens.iter().any(|token| *token == candidate)
        {
            return candidate;
        }
    }
    base.to_string()
}

pub fn fuzzy_score(value: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(100 - value.matches('/').count() as i64);
    }
    let value_lower = value.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    if value_lower.contains(&query_lower) {
        return Some(1_000 - value_lower.find(&query_lower).unwrap_or(0) as i64);
    }
    let mut score = 0i64;
    let mut chars = query_lower.chars();
    let mut current = chars.next()?;
    for (index, ch) in value_lower.chars().enumerate() {
        if ch == current {
            score += 20 - i64::try_from(index).unwrap_or_default().min(20);
            if let Some(next) = chars.next() {
                current = next;
            } else {
                return Some(score);
            }
        }
    }
    None
}

pub fn file_mention_description(display: &str, kind: &str) -> String {
    display
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| format!("{kind} in {parent}"))
        .unwrap_or_else(|| kind.to_string())
}

pub fn active_file_mention(input: &str, cursor: usize) -> Option<ActiveFileMention> {
    let prefix = input.get(..cursor)?;
    let (trigger, _) = prefix.char_indices().rev().find(|(_, ch)| *ch == '@')?;
    if trigger > 0 {
        let previous = prefix[..trigger].chars().last()?;
        if !previous.is_whitespace() && !matches!(previous, '(' | '[' | '{' | '"' | '\'')
        {
            return None;
        }
    }
    let query = &prefix[trigger + 1..];
    (!query.contains(char::is_whitespace)).then(|| ActiveFileMention {
        anchor: trigger,
        query: query.to_string(),
    })
}

pub fn apply_file_mention(
    input: &str,
    cursor: usize,
    anchor: usize,
    value: &str,
) -> Option<AppliedFileMention> {
    if anchor > cursor
        || !input.is_char_boundary(anchor)
        || !input.is_char_boundary(cursor)
    {
        return None;
    }
    let token = format!("@{value}");
    let mut next = input.to_string();
    next.replace_range(anchor..cursor, &token);
    let mut next_cursor = anchor.saturating_add(token.len());
    if next
        .get(next_cursor..)
        .and_then(|rest| rest.chars().next())
        .is_none_or(|ch| !ch.is_whitespace())
    {
        next.insert(next_cursor, ' ');
        next_cursor = next_cursor.saturating_add(1);
    }
    Some(AppliedFileMention {
        input: next,
        cursor: next_cursor,
        token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn compact_directory_label_uses_home_and_middle_elision() {
        assert_eq!(
            compact_directory_label_with_home(
                "/home/me/projects/neoism/frontends/web/src/terminal/services",
                Some("/home/me"),
            ),
            "~/.../terminal/services/"
        );
        assert_eq!(compact_directory_label_with_home("", Some("/home/me")), "-");
        assert_eq!(
            compact_directory_label_with_home("/home/me", Some("/home/me")),
            "~/"
        );
    }

    #[test]
    fn attachment_display_and_tokens_match_agent_copy() {
        assert_eq!(
            display_path_for_attachment(
                Path::new("/repo"),
                Path::new("/repo/src/main.rs"),
                false,
            ),
            "src/main.rs"
        );
        assert_eq!(
            display_path_for_attachment(Path::new("/repo"), Path::new("/repo/src"), true),
            "src/"
        );
        assert_eq!(file_attachment_token("a.png", "image/png", 2), "[image2]");
        assert_eq!(
            file_attachment_token("paper.pdf", "application/pdf", 1),
            "[pdf1]"
        );
        assert_eq!(
            file_attachment_token("notes.txt", "text/plain", 3),
            "[file3: notes.txt]"
        );
    }

    #[test]
    fn attachment_token_uniqueness_checks_input_and_existing_tokens() {
        assert_eq!(
            unique_attachment_token("", Vec::<&str>::new(), "[image1]"),
            "[image1]"
        );
        assert_eq!(
            unique_attachment_token("[image1]", Vec::<&str>::new(), "[image1]"),
            "[image1 #2]"
        );
        assert_eq!(
            unique_attachment_token("", vec!["[file1: a.txt]"], "[file1: a.txt]"),
            "[file1: a.txt #2]"
        );
    }

    #[test]
    fn file_mentions_share_fuzzy_score_and_description() {
        assert!(fuzzy_score("src/panels/agent.rs", "pa").is_some());
        assert!(fuzzy_score("src/panels/agent.rs", "zz").is_none());
        assert_eq!(
            file_mention_description("src/panels/agent.rs", "file"),
            "file in src/panels"
        );
        assert_eq!(file_mention_description("src/", "directory"), "directory");
    }

    #[test]
    fn active_file_mention_requires_a_word_boundary_and_unspaced_query() {
        assert_eq!(
            active_file_mention("open @src/ma", "open @src/ma".len()),
            Some(ActiveFileMention {
                anchor: 5,
                query: "src/ma".to_string(),
            })
        );
        assert_eq!(
            active_file_mention("see (@docs", "see (@docs".len()),
            Some(ActiveFileMention {
                anchor: 5,
                query: "docs".to_string(),
            })
        );
        assert_eq!(active_file_mention("email a@b", "email a@b".len()), None);
        assert_eq!(
            active_file_mention("open @src main", "open @src main".len()),
            None
        );
    }

    #[test]
    fn apply_file_mention_replaces_query_and_separates_following_text() {
        assert_eq!(
            apply_file_mention("open @srctoday", 9, 5, "src/main.rs"),
            Some(AppliedFileMention {
                input: "open @src/main.rs today".to_string(),
                cursor: 18,
                token: "@src/main.rs".to_string(),
            })
        );
        assert_eq!(
            apply_file_mention("open @src today", 9, 5, "src/main.rs"),
            Some(AppliedFileMention {
                input: "open @src/main.rs today".to_string(),
                cursor: 17,
                token: "@src/main.rs".to_string(),
            })
        );
    }
}
