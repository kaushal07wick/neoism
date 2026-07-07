use std::path::{Path, PathBuf};

use crate::chat_commands::{command_matches_query, CHAT_COMMANDS};

use super::AgentChoice;

#[derive(Clone)]
pub(crate) struct CompletionOption {
    pub(crate) display: String,
    pub(super) replacement: String,
    pub(crate) description: String,
    pub(super) completion: CompletionKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionMode {
    Slash,
    Mention,
}

#[derive(Clone)]
pub(super) enum CompletionKind {
    Command { execute_immediately: bool },
    File { path: PathBuf },
    Agent,
}

pub(crate) struct CompletionMenu {
    pub(crate) mode: CompletionMode,
    pub(super) trigger: usize,
    pub(crate) selected: usize,
    pub(crate) options: Vec<CompletionOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandCompletion {
    pub(crate) replacement: String,
    pub(crate) execute_immediately: bool,
}

impl CompletionMenu {
    pub(crate) fn move_previous(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or_else(|| self.options.len().saturating_sub(1));
    }

    pub(crate) fn move_next(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.options.len();
    }

    pub(crate) fn selected_command(&self) -> Option<CommandCompletion> {
        let option = self.options.get(self.selected)?;
        match option.completion {
            CompletionKind::Command {
                execute_immediately,
            } => Some(CommandCompletion {
                replacement: option.replacement.clone(),
                execute_immediately,
            }),
            _ => None,
        }
    }
}

pub(super) fn update_completion(
    text: &str,
    previous_selected: Option<usize>,
    root: &Path,
    agents: &[AgentChoice],
) -> Option<CompletionMenu> {
    if let Some(menu) = update_slash_completion(text, previous_selected) {
        return Some(menu);
    }

    if let Some(trigger) = mention_trigger(text) {
        let query = &text[trigger + 1..];
        return clamp_menu_selection(Some(CompletionMenu {
            mode: CompletionMode::Mention,
            trigger,
            selected: previous_selected.unwrap_or(0),
            options: mention_completion_options(root, agents, query),
        }));
    }

    None
}

pub(super) fn update_slash_completion(
    text: &str,
    previous_selected: Option<usize>,
) -> Option<CompletionMenu> {
    if text.starts_with('/') && !text.contains(char::is_whitespace) {
        let query = text.trim_start_matches('/');
        return clamp_menu_selection(Some(CompletionMenu {
            mode: CompletionMode::Slash,
            trigger: 0,
            selected: previous_selected.unwrap_or(0),
            options: slash_completion_options(query),
        }));
    }
    None
}

fn clamp_menu_selection(mut menu: Option<CompletionMenu>) -> Option<CompletionMenu> {
    if let Some(menu) = menu.as_mut() {
        if menu.options.is_empty() {
            menu.selected = 0;
        } else if menu.selected >= menu.options.len() {
            menu.selected = menu.options.len() - 1;
        }
    }
    menu
}

fn slash_completion_options(query: &str) -> Vec<CompletionOption> {
    let query = query.to_ascii_lowercase();
    CHAT_COMMANDS
        .iter()
        .filter(|spec| spec.names[0] != "/")
        .filter(|spec| query.is_empty() || command_matches_query(spec, &query))
        .map(|spec| CompletionOption {
            display: spec.usage.to_string(),
            replacement: spec.names[0].to_string(),
            description: spec.description.to_string(),
            completion: CompletionKind::Command {
                execute_immediately: !spec.usage.contains('[')
                    && !spec.usage.contains('<'),
            },
        })
        .take(10)
        .collect()
}

fn mention_trigger(text: &str) -> Option<usize> {
    let trigger = text.rfind('@')?;
    if trigger > 0 && !text[..trigger].ends_with(char::is_whitespace) {
        return None;
    }
    (!text[trigger + 1..].contains(char::is_whitespace)).then_some(trigger)
}

fn mention_completion_options(
    root: &Path,
    agents: &[AgentChoice],
    query: &str,
) -> Vec<CompletionOption> {
    let query_lower = query.to_ascii_lowercase();
    let mut options = agents
        .iter()
        .filter(|agent| agent.mode != "primary")
        .filter(|agent| {
            query_lower.is_empty()
                || agent.name.to_ascii_lowercase().contains(&query_lower)
        })
        .map(|agent| CompletionOption {
            display: format!("@{}", agent.name),
            replacement: format!("@{} ", agent.name),
            description: "subagent".to_string(),
            completion: CompletionKind::Agent,
        })
        .collect::<Vec<_>>();
    options.extend(file_completion_options(
        root,
        query,
        10usize.saturating_sub(options.len()),
    ));
    options.truncate(10);
    options
}

fn file_completion_options(
    root: &Path,
    query: &str,
    limit: usize,
) -> Vec<CompletionOption> {
    if limit == 0 {
        return Vec::new();
    }
    let mut scored = Vec::new();
    collect_file_candidates(root, root, query, 0, &mut scored);
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, display, path)| CompletionOption {
            display: format!("@{display}"),
            replacement: format!("@{display} "),
            description: if path.is_dir() { "directory" } else { "file" }.to_string(),
            completion: CompletionKind::File { path },
        })
        .collect()
}

fn collect_file_candidates(
    root: &Path,
    dir: &Path,
    query: &str,
    depth: usize,
    output: &mut Vec<(i64, String, PathBuf)>,
) {
    if output.len() > 512 || depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if matches!(name, ".git" | "target" | "node_modules" | ".direnv") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(score) = fuzzy_score(&relative, query) {
            output.push((
                score - i64::try_from(depth).unwrap_or_default(),
                if path.is_dir() {
                    format!("{relative}/")
                } else {
                    relative.clone()
                },
                path.clone(),
            ));
        }
        if path.is_dir() {
            collect_file_candidates(root, &path, query, depth + 1, output);
        }
    }
}

fn fuzzy_score(value: &str, query: &str) -> Option<i64> {
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

pub(super) fn file_url(path: &Path) -> String {
    let absolute = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace(' ', "%20");
    format!("file://{absolute}")
}
