//! Renderer-neutral decisions for the host `user_event` dispatcher.
//!
//! The desktop fork's `Application::user_event` (in
//! `neoism-frontend/desktop/src/app/mod.rs`) routes a long match over
//! `RioEventType`. Arms split into two camps:
//!
//! * Orchestration arms — touch multiple subsystems at once
//!   (renderer + sugarloaf graphics, font library swap, native tab
//!   plumbing). These stay in the host.
//! * Delegatable arms — translate the incoming event into a short
//!   "do this" recipe. Their decisions belong here so the web frontend
//!   can replay them.
//!
//! This module owns the second camp. Each helper is a pure function
//! over POD inputs; the host wires the result into its scheduler /
//! renderer / config setters.
//!
//! Renderer-neutral: no `neoism_window`, no `sugarloaf`, no `RioEvent`.

/// Decide whether `RioEvent::Bell` should actually play the audio bell.
/// Pure: the host owns the native bell I/O — this just gates it on the
/// configured `bell.audio` flag.
pub const fn should_play_audio_bell(config_bell_audio: bool) -> bool {
    config_bell_audio
}

/// Decide whether to forward `RioEvent::DesktopNotification { … }` to
/// the OS notifier. Currently always `true`; lives here so the web
/// frontend can later mute on configurable conditions without
/// diverging from the native path.
pub const fn should_send_desktop_notification() -> bool {
    true
}

/// Output of [`render_event_route_action`]. The `Render` /
/// `RenderRoute` branches of `user_event` reduce to three observable
/// actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderEventRouteAction {
    /// Render policy says skip; the host should do nothing.
    Skip,
    /// Consume the post-occlusion redraw, then request a redraw.
    ConsumeOcclusionAndRedraw,
    /// Just request a redraw.
    Redraw,
}

/// Reduce the per-route render gating into the action the host should
/// apply. Centralises the precedence of `skip` over
/// `consume_render_after_occlusion`.
pub const fn render_event_route_action(
    disable_unfocused_render: bool,
    disable_occluded_render: bool,
    is_focused: bool,
    is_occluded: bool,
    needs_render_after_occlusion: bool,
) -> RenderEventRouteAction {
    if disable_unfocused_render && !is_focused {
        return RenderEventRouteAction::Skip;
    }
    if disable_occluded_render && is_occluded && !needs_render_after_occlusion {
        return RenderEventRouteAction::Skip;
    }
    if needs_render_after_occlusion {
        RenderEventRouteAction::ConsumeOcclusionAndRedraw
    } else {
        RenderEventRouteAction::Redraw
    }
}

/// Quit-handling outcome for `RioEvent::Exit` / `RioEvent::Quit`. The
/// host either pops the confirm-quit dialog (and redraws) or quits the
/// route immediately, depending on `confirm_before_quit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitRequestAction {
    ConfirmQuitAndRedraw,
    QuitImmediately,
}

pub const fn quit_request_action(confirm_before_quit: bool) -> QuitRequestAction {
    if confirm_before_quit {
        QuitRequestAction::ConfirmQuitAndRedraw
    } else {
        QuitRequestAction::QuitImmediately
    }
}

/// Output of [`toggle_fullscreen_action`]. The `ToggleFullScreen`
/// user-event arm sets the fullscreen state to one of these options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleFullscreenAction {
    /// Currently fullscreen — host should leave it.
    Leave,
    /// Currently windowed — host should enter borderless fullscreen.
    EnterBorderless,
}

pub const fn toggle_fullscreen_action(
    currently_fullscreen: bool,
) -> ToggleFullscreenAction {
    if currently_fullscreen {
        ToggleFullscreenAction::Leave
    } else {
        ToggleFullscreenAction::EnterBorderless
    }
}

/// Output of [`open_editor_tab_action`]. The `OpenEditorTab` arm
/// either opens a path in the editor or opens an empty buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenEditorTabAction {
    OpenPath,
    OpenEmptyBuffer,
}

pub const fn open_editor_tab_action(path_is_some: bool) -> OpenEditorTabAction {
    if path_is_some {
        OpenEditorTabAction::OpenPath
    } else {
        OpenEditorTabAction::OpenEmptyBuffer
    }
}

/// Output of [`create_window_strategy`]. The `CreateWindow` arm
/// either clones the app config and overrides the working directory,
/// or uses the app config unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateWindowStrategy {
    UseAppConfig,
    OverrideWorkingDir,
}

pub const fn create_window_strategy(
    working_dir_override_is_some: bool,
) -> CreateWindowStrategy {
    if working_dir_override_is_some {
        CreateWindowStrategy::OverrideWorkingDir
    } else {
        CreateWindowStrategy::UseAppConfig
    }
}

/// Output of [`config_editor_target`]. `CreateConfigEditor` opens a
/// split inside the focused route or spawns a brand new window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigEditorTarget {
    Split,
    NewWindow,
}

pub const fn config_editor_target(open_config_with_split: bool) -> ConfigEditorTarget {
    if open_config_with_split {
        ConfigEditorTarget::Split
    } else {
        ConfigEditorTarget::NewWindow
    }
}

/// Decide whether `RioEvent::ColorChange` is targeting the
/// terminal background colour. Returns true when `index` matches the
/// conventional "foreground + 1" background slot.
pub const fn color_change_targets_background(
    index: usize,
    named_foreground_index: usize,
) -> bool {
    index == named_foreground_index + 1
}

/// After a terminal context exits inside a route, the host either
/// removes the whole route (last context exited) or resizes the
/// remaining grid to backfill the closed pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseTerminalAction {
    RemoveRouteAndMaybeExit,
    ResizeAfterClose,
}

pub const fn close_terminal_action(
    handle_terminal_exit_returned_true: bool,
) -> CloseTerminalAction {
    if handle_terminal_exit_returned_true {
        CloseTerminalAction::RemoveRouteAndMaybeExit
    } else {
        CloseTerminalAction::ResizeAfterClose
    }
}

/// After a route is removed, decide whether the event loop should
/// exit (only when no routes remain).
pub const fn should_exit_event_loop_after_route_removed(remaining_routes: usize) -> bool {
    remaining_routes == 0
}

/// Decide whether `RioEvent::CloseWindow` (macOS) should exit the
/// event loop after the route is removed. Mirrors the desktop fork's
/// "no routes left and not waiting for a quit confirmation" guard.
pub const fn should_exit_event_loop_after_close_window(
    remaining_routes: usize,
    confirm_before_quit: bool,
) -> bool {
    remaining_routes == 0 && !confirm_before_quit
}

/// Decide whether `RioEvent::ClipboardStore` should actually write to
/// the host clipboard. Mirrors the focus gate: only the focused route
/// is allowed to mutate the system clipboard. Pure: the host owns the
/// native clipboard handle.
pub const fn should_store_clipboard(route_is_focused: bool) -> bool {
    route_is_focused
}

/// Decide whether `RioEvent::ClipboardLoad` should actually read from
/// the host clipboard and reply. Mirrors the focus gate on the
/// store side: only the focused route may exfiltrate clipboard
/// contents into a terminal context. Pure: the host owns the native
/// clipboard handle and the messenger.
pub const fn should_load_clipboard(route_is_focused: bool) -> bool {
    route_is_focused
}

/// Duration (in milliseconds) of the synthesised audio bell on
/// platforms where neoism generates its own tone (Linux/BSD via cpal).
/// Lives here so the web frontend can mirror the same pacing and so
/// the host's bell module reads from a shared constant.
pub const BELL_DURATION_MS: u64 = 200;

/// Output of [`refresh_redraw_action`]. The file-tree refresh family
/// of arms (`RefreshFileTree`, `RefreshFileTreeGitStatus`,
/// `ApplyFileTreeGitStatus`) all share the same shape: call a
/// refresh helper, redraw the route if it reported a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRedrawAction {
    /// Refresh reported no change — host should leave the route alone.
    Skip,
    /// Refresh reported a change — host should request a redraw.
    Redraw,
}

pub const fn refresh_redraw_action(changed: bool) -> RefreshRedrawAction {
    if changed {
        RefreshRedrawAction::Redraw
    } else {
        RefreshRedrawAction::Skip
    }
}

/// Decide whether `RioEvent::ProgressReport` should be forwarded to
/// the island renderer. The island is an optional sub-renderer; when
/// it's absent (welcome route, splash, etc.) the report has no place
/// to land. Pure: the host owns the renderer state.
pub const fn should_apply_progress_report(island_is_present: bool) -> bool {
    island_is_present
}

/// Output of [`resize_event_action`]. The `WindowEvent::Resized` arm
/// either ignores zero-sized resizes (compositor noise / minimise) or
/// resizes the screen to the new physical size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEventAction {
    /// New width/height includes a zero dimension — drop the event.
    SkipZeroSize,
    /// Apply the resize.
    ApplyResize,
}

pub const fn resize_event_action(width: u32, height: u32) -> ResizeEventAction {
    if width == 0 || height == 0 {
        ResizeEventAction::SkipZeroSize
    } else {
        ResizeEventAction::ApplyResize
    }
}

/// Output of [`occluded_event_action`]. The `WindowEvent::Occluded`
/// arm updates the route's occluded flag and, when the window
/// transitions from occluded → visible, schedules a one-shot redraw
/// so the renderer can paint the first newly-visible frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccludedEventAction {
    /// Just record the new occluded state.
    UpdateOnly,
    /// Record the new occluded state and arm
    /// `needs_render_after_occlusion`.
    UpdateAndArmPostOcclusionRedraw,
}

pub const fn occluded_event_action(
    was_occluded: bool,
    is_now_occluded: bool,
) -> OccludedEventAction {
    if was_occluded && !is_now_occluded {
        OccludedEventAction::UpdateAndArmPostOcclusionRedraw
    } else {
        OccludedEventAction::UpdateOnly
    }
}

/// Decide whether a `WindowEvent::Focused(true)` event represents a
/// genuine focus regain (i.e. the route was unfocused before) versus
/// a redundant focused-while-focused notification.
pub const fn focus_regained(was_focused: bool, is_now_focused: bool) -> bool {
    !was_focused && is_now_focused
}

/// Output of [`theme_changed_action`]. The `WindowEvent::ThemeChanged`
/// arm honours `force_theme` first — if the user has pinned a theme,
/// the OS change is ignored; otherwise the new theme is propagated
/// through the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChangedAction {
    /// User has pinned a theme — drop the OS notification.
    IgnoreForcedTheme,
    /// Propagate the new theme into config + renderer.
    ApplyNewTheme,
}

pub const fn theme_changed_action(force_theme_is_set: bool) -> ThemeChangedAction {
    if force_theme_is_set {
        ThemeChangedAction::IgnoreForcedTheme
    } else {
        ThemeChangedAction::ApplyNewTheme
    }
}

/// Output of [`close_requested_action`]. The `WindowEvent::CloseRequested`
/// arm has three shapes:
///
/// * On macOS/Windows the OS has already shown its own confirm dialog
///   before we see the event, so we just close the route immediately.
/// * On Linux/other when `confirm_before_quit` is set, we show the
///   in-app confirm dialog instead of closing.
/// * Otherwise we close immediately.
///
/// The two "close immediately" branches still need to maybe-exit the
/// event loop after the route is removed; the host handles that via
/// [`should_exit_event_loop_after_route_removed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseRequestedAction {
    /// Remove the route immediately, then maybe-exit.
    CloseImmediately,
    /// Open the in-app confirm-quit dialog and request a redraw.
    ShowConfirmDialog,
}

pub const fn close_requested_action(
    os_handles_confirm: bool,
    confirm_before_quit: bool,
) -> CloseRequestedAction {
    if os_handles_confirm {
        CloseRequestedAction::CloseImmediately
    } else if confirm_before_quit {
        CloseRequestedAction::ShowConfirmDialog
    } else {
        CloseRequestedAction::CloseImmediately
    }
}

/// Decide whether the "hide cursor when typing" gate should be
/// reset by an incoming mouse event. The host always calls this when
/// `config.hide_cursor_when_typing` is on — extracted to make the
/// intent explicit at the call site (and so the web frontend can
/// replay the same gate later).
pub const fn should_unhide_cursor_on_mouse_activity(
    hide_cursor_when_typing: bool,
) -> bool {
    hide_cursor_when_typing
}

/// Output of [`native_tab_config_strategy`]. The macOS-only
/// `CreateNativeTab` arm clones the app config and optionally
/// overrides the working directory — same shape as
/// [`create_window_strategy`] but the "Use" path still requires a
/// clone (because `create_native_tab` takes `&Config`, and we need a
/// stable lifetime for the borrow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTabConfigStrategy {
    UseAppConfigClone,
    OverrideWorkingDir,
}

pub const fn native_tab_config_strategy(
    working_dir_override_is_some: bool,
) -> NativeTabConfigStrategy {
    if working_dir_override_is_some {
        NativeTabConfigStrategy::OverrideWorkingDir
    } else {
        NativeTabConfigStrategy::UseAppConfigClone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_gating_is_pure() {
        assert!(should_play_audio_bell(true));
        assert!(!should_play_audio_bell(false));
    }

    #[test]
    fn desktop_notification_default_passes_through() {
        assert!(should_send_desktop_notification());
    }

    #[test]
    fn render_event_route_action_skips_unfocused() {
        assert_eq!(
            render_event_route_action(true, false, false, false, false),
            RenderEventRouteAction::Skip
        );
    }

    #[test]
    fn render_event_route_action_skips_occluded_without_post_redraw() {
        assert_eq!(
            render_event_route_action(false, true, true, true, false),
            RenderEventRouteAction::Skip
        );
    }

    #[test]
    fn render_event_route_action_consumes_post_occlusion_redraw() {
        assert_eq!(
            render_event_route_action(false, true, true, true, true),
            RenderEventRouteAction::ConsumeOcclusionAndRedraw
        );
    }

    #[test]
    fn render_event_route_action_redraws_when_visible() {
        assert_eq!(
            render_event_route_action(true, true, true, false, false),
            RenderEventRouteAction::Redraw
        );
    }

    #[test]
    fn quit_action_confirm() {
        assert_eq!(
            quit_request_action(true),
            QuitRequestAction::ConfirmQuitAndRedraw
        );
        assert_eq!(
            quit_request_action(false),
            QuitRequestAction::QuitImmediately
        );
    }

    #[test]
    fn toggle_fullscreen_branches() {
        assert_eq!(
            toggle_fullscreen_action(true),
            ToggleFullscreenAction::Leave
        );
        assert_eq!(
            toggle_fullscreen_action(false),
            ToggleFullscreenAction::EnterBorderless
        );
    }

    #[test]
    fn open_editor_tab_branches() {
        assert_eq!(open_editor_tab_action(true), OpenEditorTabAction::OpenPath);
        assert_eq!(
            open_editor_tab_action(false),
            OpenEditorTabAction::OpenEmptyBuffer
        );
    }

    #[test]
    fn create_window_strategy_branches() {
        assert_eq!(
            create_window_strategy(true),
            CreateWindowStrategy::OverrideWorkingDir
        );
        assert_eq!(
            create_window_strategy(false),
            CreateWindowStrategy::UseAppConfig
        );
    }

    #[test]
    fn config_editor_target_branches() {
        assert_eq!(config_editor_target(true), ConfigEditorTarget::Split);
        assert_eq!(config_editor_target(false), ConfigEditorTarget::NewWindow);
    }

    #[test]
    fn color_change_targets_background_slot() {
        assert!(color_change_targets_background(8, 7));
        assert!(!color_change_targets_background(7, 7));
        assert!(!color_change_targets_background(9, 7));
    }

    #[test]
    fn close_terminal_routes() {
        assert_eq!(
            close_terminal_action(true),
            CloseTerminalAction::RemoveRouteAndMaybeExit
        );
        assert_eq!(
            close_terminal_action(false),
            CloseTerminalAction::ResizeAfterClose
        );
    }

    #[test]
    fn exit_when_last_route_gone() {
        assert!(should_exit_event_loop_after_route_removed(0));
        assert!(!should_exit_event_loop_after_route_removed(1));
    }

    #[test]
    fn close_window_exit_respects_confirm() {
        assert!(should_exit_event_loop_after_close_window(0, false));
        assert!(!should_exit_event_loop_after_close_window(0, true));
        assert!(!should_exit_event_loop_after_close_window(1, false));
    }

    #[test]
    fn clipboard_store_requires_focus() {
        assert!(should_store_clipboard(true));
        assert!(!should_store_clipboard(false));
    }

    #[test]
    fn clipboard_load_requires_focus() {
        assert!(should_load_clipboard(true));
        assert!(!should_load_clipboard(false));
    }

    #[test]
    fn bell_duration_is_stable() {
        // Locked at 200ms — change requires a UX call.
        assert_eq!(BELL_DURATION_MS, 200);
    }

    #[test]
    fn refresh_redraw_action_branches() {
        assert_eq!(refresh_redraw_action(true), RefreshRedrawAction::Redraw);
        assert_eq!(refresh_redraw_action(false), RefreshRedrawAction::Skip);
    }

    #[test]
    fn progress_report_requires_island() {
        assert!(should_apply_progress_report(true));
        assert!(!should_apply_progress_report(false));
    }

    #[test]
    fn resize_skips_zero_dimensions() {
        assert_eq!(resize_event_action(0, 100), ResizeEventAction::SkipZeroSize);
        assert_eq!(resize_event_action(100, 0), ResizeEventAction::SkipZeroSize);
        assert_eq!(resize_event_action(0, 0), ResizeEventAction::SkipZeroSize);
        assert_eq!(
            resize_event_action(1280, 720),
            ResizeEventAction::ApplyResize
        );
    }

    #[test]
    fn occluded_action_only_arms_redraw_on_visibility_regain() {
        assert_eq!(
            occluded_event_action(true, false),
            OccludedEventAction::UpdateAndArmPostOcclusionRedraw
        );
        assert_eq!(
            occluded_event_action(false, true),
            OccludedEventAction::UpdateOnly
        );
        assert_eq!(
            occluded_event_action(false, false),
            OccludedEventAction::UpdateOnly
        );
        assert_eq!(
            occluded_event_action(true, true),
            OccludedEventAction::UpdateOnly
        );
    }

    #[test]
    fn focus_regained_branches() {
        assert!(focus_regained(false, true));
        assert!(!focus_regained(true, true));
        assert!(!focus_regained(false, false));
        assert!(!focus_regained(true, false));
    }

    #[test]
    fn theme_changed_honours_force_theme() {
        assert_eq!(
            theme_changed_action(true),
            ThemeChangedAction::IgnoreForcedTheme
        );
        assert_eq!(
            theme_changed_action(false),
            ThemeChangedAction::ApplyNewTheme
        );
    }

    #[test]
    fn close_requested_branches() {
        // macOS / Windows: OS confirm already happened.
        assert_eq!(
            close_requested_action(true, true),
            CloseRequestedAction::CloseImmediately
        );
        assert_eq!(
            close_requested_action(true, false),
            CloseRequestedAction::CloseImmediately
        );
        // Linux + confirm enabled → show in-app dialog.
        assert_eq!(
            close_requested_action(false, true),
            CloseRequestedAction::ShowConfirmDialog
        );
        // Linux + no confirm → close.
        assert_eq!(
            close_requested_action(false, false),
            CloseRequestedAction::CloseImmediately
        );
    }

    #[test]
    fn hide_cursor_gate_passes_through() {
        assert!(should_unhide_cursor_on_mouse_activity(true));
        assert!(!should_unhide_cursor_on_mouse_activity(false));
    }

    #[test]
    fn native_tab_strategy_branches() {
        assert_eq!(
            native_tab_config_strategy(true),
            NativeTabConfigStrategy::OverrideWorkingDir
        );
        assert_eq!(
            native_tab_config_strategy(false),
            NativeTabConfigStrategy::UseAppConfigClone
        );
    }
}
