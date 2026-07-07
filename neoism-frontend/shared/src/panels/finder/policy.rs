use std::path::{Path, PathBuf};

use neoism_terminal_core::crosswords::pos::Pos;
use neoism_terminal_core::crosswords::search::Match;

use super::FinderMode;
use crate::editor::markdown::is_markdown_path;

/// Inputs needed to choose how the desktop bridge should react to a
/// finder result selection: path, optional line target, and the mode
/// the finder was in (so we know whether the query needs to seed
/// `hlsearch` etc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderOpenRequest {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub mode: FinderMode,
    pub query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchCloseIntent {
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatchSelection {
    pub start: Pos,
    pub end: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchCloseAction {
    Exit,
    ResetViState,
    SelectFocusedMatch(SearchMatchSelection),
}

pub fn focused_match_selection(focused_match: &Match) -> SearchMatchSelection {
    SearchMatchSelection {
        start: *focused_match.start(),
        end: *focused_match.end(),
    }
}

pub fn search_close_action(
    intent: SearchCloseIntent,
    vi_mode: bool,
    focused_match: Option<&Match>,
) -> SearchCloseAction {
    match (intent, vi_mode, focused_match) {
        (SearchCloseIntent::Confirm, true, _) => SearchCloseAction::Exit,
        (SearchCloseIntent::Cancel, true, _) => SearchCloseAction::ResetViState,
        (_, false, Some(focused_match)) => {
            SearchCloseAction::SelectFocusedMatch(focused_match_selection(focused_match))
        }
        (_, false, None) => SearchCloseAction::Exit,
    }
}

/// Pure inputs for the finder's "which editor route should I open into?"
/// decision. Lives here so both the desktop and web frontends produce
/// identical routing fallbacks for `:Files` / `:Grep` / `:GitChanges`.
///
/// Resolution order:
///   1. If the file tree currently holds focus → the primary editor route
///      (we never open files into the tree itself).
///   2. If a pane strip is active → the pane's editor route.
///   3. Otherwise the primary editor route.
///   4. As a final fallback, the current context's route if that
///      context owns an editor at all.
#[derive(Debug, Clone, Copy)]
pub struct FinderTargetRouteInputs {
    pub file_tree_focused: bool,
    pub primary_editor_route: Option<usize>,
    /// `Some(route)` when a pane strip is the active focus target.
    pub active_pane_strip_route: Option<usize>,
    /// Resolved editor route for the active pane strip, if any. Caller
    /// pre-computes this because the strip → editor mapping is a desktop
    /// renderer-side lookup.
    pub pane_editor_route_for_strip: Option<usize>,
    /// True when `context_manager.current().editor.is_some()` — controls
    /// whether the current route is a usable final fallback.
    pub current_context_has_editor: bool,
    pub current_route: usize,
}

pub fn finder_target_route_decision(inputs: FinderTargetRouteInputs) -> Option<usize> {
    if inputs.file_tree_focused {
        return inputs.primary_editor_route;
    }
    if inputs.active_pane_strip_route.is_some() {
        return inputs.pane_editor_route_for_strip;
    }
    inputs.primary_editor_route.or_else(|| {
        inputs
            .current_context_has_editor
            .then_some(inputs.current_route)
    })
}

/// Pure inputs for the finder's cwd-resolution fallback chain. Each
/// optional field is the candidate from the corresponding stage; the
/// first one set wins, otherwise we fall back to `working_dir_config`
/// (config-level default) and finally the caller-supplied `fallback`
/// (typically `std::env::current_dir()` on the desktop / `"/"` on web).
#[derive(Debug, Clone)]
pub struct FinderCwdInputs {
    pub active_pane_workspace_root: Option<PathBuf>,
    pub active_workspace_root: Option<PathBuf>,
    pub target_route_editor_cwd: Option<PathBuf>,
    pub current_editor_cwd: Option<PathBuf>,
    pub working_dir_config: Option<PathBuf>,
    pub fallback: PathBuf,
}

pub fn finder_cwd_decision(inputs: FinderCwdInputs) -> PathBuf {
    inputs
        .active_pane_workspace_root
        .or(inputs.active_workspace_root)
        .or(inputs.target_route_editor_cwd)
        .or(inputs.current_editor_cwd)
        .or(inputs.working_dir_config)
        .unwrap_or(inputs.fallback)
}

/// What kind of editor we should dispatch a freshly-selected finder
/// result into. The desktop side is responsible for actually steering
/// the buffer (markdown viewer vs nvim) — this enum just captures the
/// branching decision so it can be unit-tested without touching the
/// renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinderOpenAction {
    /// Result is a markdown file — open in the markdown viewer and
    /// optionally jump to `line`.
    OpenMarkdown { path: PathBuf, line: Option<u32> },
    /// Result is a buffer + line, with optional grep query to seed
    /// `hlsearch`. Caller decides which route receives the command.
    EditAtLine {
        path: PathBuf,
        line: u32,
        target_route: Option<usize>,
        grep_query: Option<String>,
        is_git: bool,
    },
    /// Result is a plain file (no line target).
    EditFile {
        path: PathBuf,
        target_route: Option<usize>,
    },
}

/// Compute the open-action for a finder selection. `target_route` is the
/// caller's chosen editor route (typically the cached
/// `finder_target_route` or a fresh `finder_target_route_decision`).
pub fn plan_finder_open(
    request: FinderOpenRequest,
    target_route: Option<usize>,
) -> FinderOpenAction {
    let FinderOpenRequest {
        path,
        line,
        mode,
        query,
    } = request;

    if is_markdown_path(&path) {
        return FinderOpenAction::OpenMarkdown { path, line };
    }

    match line {
        Some(line) => {
            let trimmed = query.trim();
            let grep_query = matches!(mode, FinderMode::Grep)
                .then_some(trimmed)
                .filter(|q| !q.is_empty())
                .map(|q| q.to_owned());
            let is_git = matches!(mode, FinderMode::GitChanges);
            FinderOpenAction::EditAtLine {
                path,
                line,
                target_route,
                grep_query,
                is_git,
            }
        }
        None => FinderOpenAction::EditFile { path, target_route },
    }
}

/// Build the chained lua command that edits `path`, jumps to `line`,
/// and optionally seeds `hlsearch` (for grep results) or invokes the
/// git-changes preview helper. Kept pure so both frontends can produce
/// the identical nvim payload — see the long comment on `open_finder_selection`
/// in the desktop bridge for why this has to be a single pcall.
pub fn build_finder_edit_lua(
    path: &Path,
    line: u32,
    grep_query: Option<&str>,
    is_git: bool,
) -> String {
    let path_lit = lua_string_literal(&path.display().to_string());
    let mut cmd = format!(
        r#"lua pcall(function() vim.cmd.edit({path_lit}); vim.api.nvim_win_set_cursor(0, {{ {line}, 0 }}); vim.cmd('normal! zz')"#
    );
    if let Some(query) = grep_query {
        // `\V` (very-nomagic) makes the rest of the pattern literal so
        // regex metachars in the user's query match themselves.
        let lit_query = lua_string_literal(&format!(r"\V{}", query));
        cmd.push_str(&format!(
            r#"; vim.fn.setreg('/', {lit_query}); vim.o.hlsearch = true"#
        ));
    }
    if is_git {
        cmd.push_str(&format!(r#"; require('rio.search').preview({line})"#));
    }
    cmd.push_str(" end)");
    cmd
}

/// Quote `s` as a Lua double-quoted string literal — escape `\` and
/// `"`. Mirrors `neoism_backend::performer::nvim::lua_string_literal`;
/// duplicated here because `neoism-ui` does not depend on `neoism-backend`.
fn lua_string_literal(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// What `search_input` should do for a typed character given the
/// current search-history cursor position. Lifted here so the desktop
/// keystroke handler stays a thin dispatcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchInputAction {
    /// User isn't actively editing the search regex (no history index).
    Ignore,
    /// `c` is non-printable; drop the keystroke entirely.
    IgnoreNonPrintable,
    /// User is browsing history (index > 0). Copy the historic entry
    /// down to slot 0, then apply `edit`.
    PromoteHistory {
        source_index: usize,
        edit: SearchEdit,
    },
    /// User is already editing slot 0; apply `edit` in place.
    Apply { edit: SearchEdit },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEdit {
    /// `\x08` / `\x7f` — pop the last char.
    Backspace,
    /// Printable char (ascii ' '..='~' or unicode '\u{a0}'..).
    Push(char),
}

pub fn search_input_action(c: char, history_index: Option<usize>) -> SearchInputAction {
    let Some(index) = history_index else {
        return SearchInputAction::Ignore;
    };
    let edit = match c {
        '\x08' | '\x7f' => SearchEdit::Backspace,
        ' '..='~' | '\u{a0}'..='\u{10ffff}' => SearchEdit::Push(c),
        _ => return SearchInputAction::IgnoreNonPrintable,
    };
    if index == 0 {
        SearchInputAction::Apply { edit }
    } else {
        SearchInputAction::PromoteHistory {
            source_index: index,
            edit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoism_terminal_core::crosswords::pos::{Column, Line};

    fn pos(line: i32, col: usize) -> Pos {
        Pos::new(Line(line), Column(col))
    }

    #[test]
    fn target_route_prefers_primary_when_file_tree_focused() {
        let route = finder_target_route_decision(FinderTargetRouteInputs {
            file_tree_focused: true,
            primary_editor_route: Some(7),
            active_pane_strip_route: Some(3),
            pane_editor_route_for_strip: Some(4),
            current_context_has_editor: true,
            current_route: 99,
        });
        assert_eq!(route, Some(7));
    }

    #[test]
    fn target_route_uses_pane_strip_when_active() {
        let route = finder_target_route_decision(FinderTargetRouteInputs {
            file_tree_focused: false,
            primary_editor_route: Some(7),
            active_pane_strip_route: Some(3),
            pane_editor_route_for_strip: Some(4),
            current_context_has_editor: true,
            current_route: 99,
        });
        assert_eq!(route, Some(4));
    }

    #[test]
    fn target_route_falls_back_to_current_route_when_editor_present() {
        let route = finder_target_route_decision(FinderTargetRouteInputs {
            file_tree_focused: false,
            primary_editor_route: None,
            active_pane_strip_route: None,
            pane_editor_route_for_strip: None,
            current_context_has_editor: true,
            current_route: 42,
        });
        assert_eq!(route, Some(42));
    }

    #[test]
    fn target_route_none_without_any_editor() {
        let route = finder_target_route_decision(FinderTargetRouteInputs {
            file_tree_focused: false,
            primary_editor_route: None,
            active_pane_strip_route: None,
            pane_editor_route_for_strip: None,
            current_context_has_editor: false,
            current_route: 42,
        });
        assert_eq!(route, None);
    }

    #[test]
    fn cwd_uses_active_pane_workspace_root_first() {
        let cwd = finder_cwd_decision(FinderCwdInputs {
            active_pane_workspace_root: Some(PathBuf::from("/a")),
            active_workspace_root: Some(PathBuf::from("/b")),
            target_route_editor_cwd: Some(PathBuf::from("/c")),
            current_editor_cwd: Some(PathBuf::from("/d")),
            working_dir_config: Some(PathBuf::from("/e")),
            fallback: PathBuf::from("/f"),
        });
        assert_eq!(cwd, PathBuf::from("/a"));
    }

    #[test]
    fn cwd_falls_through_to_target_route_when_no_workspace() {
        let cwd = finder_cwd_decision(FinderCwdInputs {
            active_pane_workspace_root: None,
            active_workspace_root: None,
            target_route_editor_cwd: Some(PathBuf::from("/c")),
            current_editor_cwd: Some(PathBuf::from("/d")),
            working_dir_config: None,
            fallback: PathBuf::from("/f"),
        });
        assert_eq!(cwd, PathBuf::from("/c"));
    }

    #[test]
    fn cwd_uses_fallback_when_everything_empty() {
        let cwd = finder_cwd_decision(FinderCwdInputs {
            active_pane_workspace_root: None,
            active_workspace_root: None,
            target_route_editor_cwd: None,
            current_editor_cwd: None,
            working_dir_config: None,
            fallback: PathBuf::from("/tmp"),
        });
        assert_eq!(cwd, PathBuf::from("/tmp"));
    }

    #[test]
    fn open_action_routes_markdown_to_viewer() {
        let action = plan_finder_open(
            FinderOpenRequest {
                path: PathBuf::from("/repo/notes.md"),
                line: Some(42),
                mode: FinderMode::Files,
                query: String::new(),
            },
            Some(3),
        );
        assert_eq!(
            action,
            FinderOpenAction::OpenMarkdown {
                path: PathBuf::from("/repo/notes.md"),
                line: Some(42),
            }
        );
    }

    #[test]
    fn open_action_emits_edit_at_line_with_grep_highlight() {
        let action = plan_finder_open(
            FinderOpenRequest {
                path: PathBuf::from("/repo/src/lib.rs"),
                line: Some(17),
                mode: FinderMode::Grep,
                query: "  vec[0]  ".to_owned(),
            },
            Some(2),
        );
        assert_eq!(
            action,
            FinderOpenAction::EditAtLine {
                path: PathBuf::from("/repo/src/lib.rs"),
                line: 17,
                target_route: Some(2),
                grep_query: Some("vec[0]".to_owned()),
                is_git: false,
            }
        );
    }

    #[test]
    fn open_action_skips_grep_highlight_when_query_blank() {
        let action = plan_finder_open(
            FinderOpenRequest {
                path: PathBuf::from("/repo/src/lib.rs"),
                line: Some(1),
                mode: FinderMode::Grep,
                query: "   ".to_owned(),
            },
            None,
        );
        let FinderOpenAction::EditAtLine { grep_query, .. } = action else {
            panic!("expected EditAtLine");
        };
        assert!(grep_query.is_none());
    }

    #[test]
    fn open_action_marks_git_changes() {
        let action = plan_finder_open(
            FinderOpenRequest {
                path: PathBuf::from("/repo/src/lib.rs"),
                line: Some(5),
                mode: FinderMode::GitChanges,
                query: String::new(),
            },
            Some(1),
        );
        let FinderOpenAction::EditAtLine { is_git, .. } = action else {
            panic!("expected EditAtLine");
        };
        assert!(is_git);
    }

    #[test]
    fn open_action_no_line_yields_edit_file() {
        let action = plan_finder_open(
            FinderOpenRequest {
                path: PathBuf::from("/repo/Cargo.toml"),
                line: None,
                mode: FinderMode::Files,
                query: String::new(),
            },
            None,
        );
        assert_eq!(
            action,
            FinderOpenAction::EditFile {
                path: PathBuf::from("/repo/Cargo.toml"),
                target_route: None,
            }
        );
    }

    #[test]
    fn lua_command_plain_edit_includes_zz() {
        let cmd = build_finder_edit_lua(Path::new("/repo/a.rs"), 3, None, false);
        assert!(cmd.contains(r#"vim.cmd.edit("/repo/a.rs")"#));
        assert!(cmd.contains("nvim_win_set_cursor(0, { 3, 0 })"));
        assert!(cmd.contains("normal! zz"));
        assert!(!cmd.contains("hlsearch"));
        assert!(!cmd.contains("rio.search"));
    }

    #[test]
    fn lua_command_grep_seeds_hlsearch_with_very_nomagic() {
        let cmd =
            build_finder_edit_lua(Path::new("/repo/a.rs"), 3, Some("vec[0]"), false);
        // `\V` in the lua source becomes `\\V` after string-literal
        // escaping — that's what nvim sees back as `\V` (very-nomagic).
        assert!(cmd.contains(r#"setreg('/', "\\Vvec[0]")"#), "cmd: {cmd}");
        assert!(cmd.contains("hlsearch = true"));
    }

    #[test]
    fn lua_command_git_invokes_preview() {
        let cmd = build_finder_edit_lua(Path::new("/repo/a.rs"), 9, None, true);
        assert!(cmd.contains("require('rio.search').preview(9)"));
    }

    #[test]
    fn lua_string_escapes_backslashes_and_quotes() {
        assert_eq!(lua_string_literal(r#"a\b"c"#), r#""a\\b\"c""#);
    }

    #[test]
    fn search_input_ignores_when_history_inactive() {
        assert_eq!(search_input_action('a', None), SearchInputAction::Ignore);
    }

    #[test]
    fn search_input_drops_non_printable() {
        assert_eq!(
            search_input_action('\x01', Some(0)),
            SearchInputAction::IgnoreNonPrintable
        );
    }

    #[test]
    fn search_input_applies_in_place_at_slot_zero() {
        assert_eq!(
            search_input_action('q', Some(0)),
            SearchInputAction::Apply {
                edit: SearchEdit::Push('q'),
            }
        );
        assert_eq!(
            search_input_action('\x7f', Some(0)),
            SearchInputAction::Apply {
                edit: SearchEdit::Backspace,
            }
        );
    }

    #[test]
    fn search_input_promotes_history_when_browsing() {
        assert_eq!(
            search_input_action('x', Some(2)),
            SearchInputAction::PromoteHistory {
                source_index: 2,
                edit: SearchEdit::Push('x'),
            }
        );
    }

    #[test]
    fn close_search_keeps_confirm_in_vi_mode_as_exit_only() {
        let focused_match = pos(2, 3)..=pos(2, 8);

        assert_eq!(
            search_close_action(SearchCloseIntent::Confirm, true, Some(&focused_match)),
            SearchCloseAction::Exit
        );
    }

    #[test]
    fn close_search_resets_vi_state_on_cancel() {
        let focused_match = pos(2, 3)..=pos(2, 8);

        assert_eq!(
            search_close_action(SearchCloseIntent::Cancel, true, Some(&focused_match)),
            SearchCloseAction::ResetViState
        );
    }

    #[test]
    fn close_search_selects_focused_match_outside_vi_mode() {
        let focused_match = pos(2, 3)..=pos(2, 8);
        let expected = SearchCloseAction::SelectFocusedMatch(SearchMatchSelection {
            start: pos(2, 3),
            end: pos(2, 8),
        });

        assert_eq!(
            search_close_action(SearchCloseIntent::Confirm, false, Some(&focused_match)),
            expected
        );
        assert_eq!(
            search_close_action(SearchCloseIntent::Cancel, false, Some(&focused_match)),
            expected
        );
    }

    #[test]
    fn close_search_without_match_exits_outside_vi_mode() {
        assert_eq!(
            search_close_action(SearchCloseIntent::Confirm, false, None),
            SearchCloseAction::Exit
        );
        assert_eq!(
            search_close_action(SearchCloseIntent::Cancel, false, None),
            SearchCloseAction::Exit
        );
    }
}
