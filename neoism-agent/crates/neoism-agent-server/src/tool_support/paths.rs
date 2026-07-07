use std::path::{Path, PathBuf};

use anyhow::Context;

use super::ToolContext;

pub(super) fn existing_project_path(
    context: &ToolContext,
    raw: &str,
) -> anyhow::Result<PathBuf> {
    let base = context.cwd.canonicalize().with_context(|| {
        format!(
            "failed to resolve project directory {}",
            context.cwd.display()
        )
    })?;
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        base.join(raw)
    };
    let path = candidate
        .canonicalize()
        .with_context(|| format!("failed to resolve path {}", candidate.display()))?;
    if !path.starts_with(&base) {
        context.ensure_explicit_allowed(
            "external_directory",
            &external_directory_pattern(&path, path.is_dir()),
        )?;
    }
    Ok(path)
}

pub(super) fn project_path_for_write(
    context: &ToolContext,
    raw: &str,
) -> anyhow::Result<PathBuf> {
    let base = context.cwd.canonicalize().with_context(|| {
        format!(
            "failed to resolve project directory {}",
            context.cwd.display()
        )
    })?;
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        base.join(raw)
    };
    let parent = candidate
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path {} has no parent", candidate.display()))?;
    let parent = parent
        .canonicalize()
        .with_context(|| format!("failed to resolve directory {}", parent.display()))?;
    if !parent.starts_with(&base) {
        context.ensure_explicit_allowed(
            "external_directory",
            &external_directory_pattern(&parent, true),
        )?;
    }
    Ok(parent.join(candidate.file_name().ok_or_else(|| {
        anyhow::anyhow!("path {} has no file name", candidate.display())
    })?))
}

pub(super) fn directory_entries(path: &Path) -> anyhow::Result<Vec<String>> {
    let mut entries = std::fs::read_dir(path)
        .with_context(|| format!("failed to list {}", path.display()))?
        .map(|entry| {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let mut name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                name.push('/');
            }
            Ok(name)
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}

pub(super) fn display_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()))
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

pub(super) fn external_directory_pattern(path: &Path, directory: bool) -> String {
    let dir = if directory {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    format!("{}/*", dir.display())
}

pub(super) fn truncate_line(line: &str) -> String {
    const MAX: usize = 2000;
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    line.chars().take(MAX).collect::<String>() + "..."
}

pub(super) fn is_ignored_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.ends_with("/.claude/worktrees")
        || normalized.ends_with("/.neoism/cache")
    {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            matches!(
                name,
                ".git" | ".codex" | ".tmp" | "target" | "node_modules" | "dist"
            )
        })
        .unwrap_or(false)
}

pub(super) fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.strip_prefix("**/").unwrap_or(pattern);
    wildcard_match_inner(pattern.as_bytes(), value.as_bytes())
}

fn wildcard_match_inner(pattern: &[u8], value: &[u8]) -> bool {
    let (mut pattern_index, mut value_index) = (0, 0);
    let mut star = None;
    let mut star_value_index = 0;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?'
                || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}
