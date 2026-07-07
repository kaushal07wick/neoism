//! Pure title-format helpers shared by native + web.
//!
//! Native `context::title::{shorten_path, create_title_extra_from_context}`
//! lifted here so the policy (replace `$HOME` prefix with `~`, collapse
//! 4+ component paths to `…/last3`) lives next to the rest of the
//! workspace formatting helpers and stays testable without pulling in
//! winit or PTY.
//!
//! Web has no `$HOME`, so callers can pass `home=None` to skip the
//! substitution step.

use std::path::Path;

/// Per-context title metadata appended after the main title string.
/// Currently just the foreground program name. Native callers extract
/// the program via `teletypewriter::foreground_process_name` then call
/// [`title_extra_from_program`] to produce the POD.
#[derive(Debug, Clone, Default)]
pub struct ContextTitleExtra {
    pub program: String,
}

/// Build a [`ContextTitleExtra`] from a foreground-program string. On
/// native unix the caller passes `teletypewriter::foreground_process_name(...)`;
/// web passes an empty string (no PTY model yet).
pub fn title_extra_from_program(program: String) -> ContextTitleExtra {
    ContextTitleExtra { program }
}

/// Shorten an absolute path for display:
/// - Replace `home` prefix with `~`
/// - If 4+ components deep, show `…/last/three/components`
///
/// `home` is the user's home directory (e.g. from `dirs::home_dir()`
/// on native). On web pass `None` to skip the home substitution and
/// just collapse deep paths.
pub fn shorten_path(absolute: &str, home: Option<&Path>) -> String {
    let path = Path::new(absolute);

    let display_path = if let Some(home) = home {
        if let Ok(stripped) = path.strip_prefix(home) {
            let s = stripped.to_string_lossy();
            if s.is_empty() {
                "~".to_string()
            } else {
                format!("~/{s}")
            }
        } else {
            absolute.to_string()
        }
    } else {
        absolute.to_string()
    };

    let components: Vec<&str> =
        display_path.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() >= 4 {
        format!("…/{}", components[components.len() - 3..].join("/"))
    } else {
        display_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn shorten_keeps_short_paths() {
        assert_eq!(shorten_path("/a/b/c", None), "/a/b/c");
    }

    #[test]
    fn shorten_collapses_deep_paths() {
        assert_eq!(shorten_path("/a/b/c/d/e", None), "…/c/d/e");
        assert_eq!(shorten_path("/a/b/c/d", None), "…/b/c/d");
    }

    #[test]
    fn shorten_substitutes_home_when_given() {
        let home = PathBuf::from("/home/x");
        assert_eq!(shorten_path("/home/x/sub", Some(&home)), "~/sub");
        assert_eq!(shorten_path("/home/x", Some(&home)), "~");
    }

    #[test]
    fn shorten_preserves_non_home_paths() {
        let home = PathBuf::from("/home/x");
        assert_eq!(
            shorten_path("/rio-sandbox-test-dir", Some(&home)),
            "/rio-sandbox-test-dir"
        );
    }
}
