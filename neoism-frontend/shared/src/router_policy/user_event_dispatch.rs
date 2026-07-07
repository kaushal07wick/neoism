//! Shared `user_event` dispatch plan.
//!
//! Renderer-neutral classifier that folds the desktop fork's many
//! "wake X subsystem and redraw" / "broadcast Y to every route" /
//! "mutate one screen field" match arms into a single
//! [`UserEventAction`] enum. The host (desktop or web) reduces the
//! arm to an action via [`user_event_dispatch_plan`], then calls the
//! single corresponding native helper.
//!
//! Splitting the *classification* into shared code means the web
//! frontend reproduces the same action vocabulary byte-for-byte and
//! exercises the same tests. The actual subsystem calls (e.g.
//! `screen.drain_acp_events()`, `screen.reset_mouse()`) stay native
//! because they touch winit-bound types.
//!
//! Renderer-neutral: no `neoism_window`, no `sugarloaf`, no `RioEvent`.

/// Variants of "act on the current route and redraw" that the host
/// `user_event` dispatcher receives. Each variant maps 1:1 to a
/// single subsystem call the host owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteRedrawAction {
    /// `RioEvent::AcpWake` — drain pending ACP events on the route.
    DrainAcpEvents,
    /// `RioEvent::WorkspaceNotesWake` — drain the workspace note index.
    DrainWorkspaceNotes,
    /// `RioEvent::SelectionScrollTick` — advance the selection-scroll
    /// animation by one tick.
    SelectionScrollTick,
    /// `RioEvent::CursorBlinkingChange` — request a redraw only; the
    /// per-route cursor blink toggle is handled separately.
    CursorBlinkRedraw,
}

/// Variants of "broadcast to every route and redraw the ones that
/// changed". The file-tree refresh arms collapse into this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTreeBroadcastAction {
    /// `RioEvent::RefreshFileTreeGitStatus` — kick off the async
    /// git-status refresh for every route.
    RefreshGitStatus,
    /// `RioEvent::RefreshFileTree` — rescan the file-tree for every
    /// route.
    Refresh,
    /// `RioEvent::ApplyFileTreeGitStatus` — fold a completed
    /// git-status refresh into the rendered tree.
    ApplyGitStatusRefresh,
}

/// Variants of "set the focused-window title bar" produced by
/// `RioEvent::Title` and `RioEvent::TitleWithSubtitle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleAction {
    SetTitle { title: String },
    SetTitleAndSubtitle { title: String, subtitle: String },
}

/// Top-level classifier for the dispatch arms. The host reduces an
/// incoming `RioEvent` to a `UserEventAction`, then runs the single
/// matching native helper. Variants intentionally hold owned data
/// because the originating event is moved into the match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserEventAction {
    /// Mutate the route for `window_id` then request a redraw.
    RouteRedraw(RouteRedrawAction),
    /// Broadcast the action to every route, redrawing the ones whose
    /// state actually changed.
    FileTreeBroadcast(FileTreeBroadcastAction),
    /// Update the window title bar on the route bound to `window_id`.
    Title(TitleAction),
    /// `RioEvent::MouseCursorDirty` — reset the per-route mouse state.
    ResetMouse,
    /// `RioEvent::UpdateTitles` — refresh the context-manager title
    /// for every focused route.
    UpdateFocusedTitles,
    /// `RioEvent::Bell` — play the audio bell when configured.
    Bell { audio_enabled: bool },
    /// `RioEvent::DesktopNotification` — forward to the OS notifier.
    DesktopNotification { title: String, body: String },
}

impl UserEventAction {
    /// Convenience constructor for `RioEvent::AcpWake`.
    pub const fn drain_acp() -> Self {
        Self::RouteRedraw(RouteRedrawAction::DrainAcpEvents)
    }

    /// Convenience constructor for `RioEvent::WorkspaceNotesWake`.
    pub const fn drain_workspace_notes() -> Self {
        Self::RouteRedraw(RouteRedrawAction::DrainWorkspaceNotes)
    }

    /// Convenience constructor for `RioEvent::SelectionScrollTick`.
    pub const fn selection_scroll_tick() -> Self {
        Self::RouteRedraw(RouteRedrawAction::SelectionScrollTick)
    }

    /// Convenience constructor for `RioEvent::CursorBlinkingChange`.
    pub const fn cursor_blink_redraw() -> Self {
        Self::RouteRedraw(RouteRedrawAction::CursorBlinkRedraw)
    }

    /// Convenience constructor for `RioEvent::RefreshFileTreeGitStatus`.
    pub const fn refresh_git_status() -> Self {
        Self::FileTreeBroadcast(FileTreeBroadcastAction::RefreshGitStatus)
    }

    /// Convenience constructor for `RioEvent::RefreshFileTree`.
    pub const fn refresh_file_tree() -> Self {
        Self::FileTreeBroadcast(FileTreeBroadcastAction::Refresh)
    }

    /// Convenience constructor for `RioEvent::ApplyFileTreeGitStatus`.
    pub const fn apply_git_status() -> Self {
        Self::FileTreeBroadcast(FileTreeBroadcastAction::ApplyGitStatusRefresh)
    }

    /// Convenience constructor for `RioEvent::Title(title)`.
    pub fn set_title(title: String) -> Self {
        Self::Title(TitleAction::SetTitle { title })
    }

    /// Convenience constructor for `RioEvent::TitleWithSubtitle`.
    pub fn set_title_and_subtitle(title: String, subtitle: String) -> Self {
        Self::Title(TitleAction::SetTitleAndSubtitle { title, subtitle })
    }
}

/// Plan how `RioEvent::Bell` should resolve given the configured
/// `bell.audio` flag. Returns the matching [`UserEventAction`] so the
/// host has a uniform call site.
pub const fn bell_dispatch(audio_enabled: bool) -> UserEventAction {
    UserEventAction::Bell { audio_enabled }
}

/// Plan how `RioEvent::DesktopNotification` should resolve. Pure
/// passthrough today; lives here so the web frontend can later mute
/// without diverging from the native path.
pub fn desktop_notification_dispatch(title: String, body: String) -> UserEventAction {
    UserEventAction::DesktopNotification { title, body }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_redraw_constructors_match_variants() {
        assert_eq!(
            UserEventAction::drain_acp(),
            UserEventAction::RouteRedraw(RouteRedrawAction::DrainAcpEvents)
        );
        assert_eq!(
            UserEventAction::drain_workspace_notes(),
            UserEventAction::RouteRedraw(RouteRedrawAction::DrainWorkspaceNotes)
        );
        assert_eq!(
            UserEventAction::selection_scroll_tick(),
            UserEventAction::RouteRedraw(RouteRedrawAction::SelectionScrollTick)
        );
        assert_eq!(
            UserEventAction::cursor_blink_redraw(),
            UserEventAction::RouteRedraw(RouteRedrawAction::CursorBlinkRedraw)
        );
    }

    #[test]
    fn file_tree_broadcast_constructors_match_variants() {
        assert_eq!(
            UserEventAction::refresh_git_status(),
            UserEventAction::FileTreeBroadcast(FileTreeBroadcastAction::RefreshGitStatus)
        );
        assert_eq!(
            UserEventAction::refresh_file_tree(),
            UserEventAction::FileTreeBroadcast(FileTreeBroadcastAction::Refresh)
        );
        assert_eq!(
            UserEventAction::apply_git_status(),
            UserEventAction::FileTreeBroadcast(
                FileTreeBroadcastAction::ApplyGitStatusRefresh
            )
        );
    }

    #[test]
    fn title_constructors_match_variants() {
        assert_eq!(
            UserEventAction::set_title("hi".into()),
            UserEventAction::Title(TitleAction::SetTitle { title: "hi".into() })
        );
        assert_eq!(
            UserEventAction::set_title_and_subtitle("hi".into(), "sub".into()),
            UserEventAction::Title(TitleAction::SetTitleAndSubtitle {
                title: "hi".into(),
                subtitle: "sub".into(),
            })
        );
    }

    #[test]
    fn bell_dispatch_carries_audio_flag() {
        assert_eq!(
            bell_dispatch(true),
            UserEventAction::Bell {
                audio_enabled: true
            }
        );
        assert_eq!(
            bell_dispatch(false),
            UserEventAction::Bell {
                audio_enabled: false
            }
        );
    }

    #[test]
    fn desktop_notification_dispatch_carries_title_and_body() {
        assert_eq!(
            desktop_notification_dispatch("t".into(), "b".into()),
            UserEventAction::DesktopNotification {
                title: "t".into(),
                body: "b".into(),
            }
        );
    }
}
