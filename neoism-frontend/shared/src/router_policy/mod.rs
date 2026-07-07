use web_time::{Duration, Instant};

pub mod key_routing;
pub mod user_event_dispatch;

pub const MAX_ANIMATION_FRAME_DELTA: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteScreen {
    Terminal,
    Welcome,
    ConfirmQuit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKey {
    Enter,
    Escape,
    Character(String),
    Other,
}

pub fn classify_route_key(
    is_enter_key: bool,
    is_escape_key: bool,
    character_text: Option<&str>,
) -> RouteKey {
    if is_enter_key {
        RouteKey::Enter
    } else if is_escape_key {
        RouteKey::Escape
    } else if let Some(text) = character_text {
        RouteKey::Character(text.to_string())
    } else {
        RouteKey::Other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteInputDecision {
    PassThrough,
    Consume,
    DismissAssistantOverlay,
    CancelConfirmQuit,
    AcceptConfirmQuit,
    CreateConfigAndEnterTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteErrorTarget {
    Welcome,
    AssistantOverlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowVisibilityState {
    pub is_focused: bool,
    pub is_occluded: bool,
    pub needs_render_after_occlusion: bool,
}

impl WindowVisibilityState {
    pub const fn visible_and_focused() -> Self {
        Self {
            is_focused: true,
            is_occluded: false,
            needs_render_after_occlusion: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFocusDecision {
    pub is_focused: bool,
    pub request_redraw: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowOcclusionDecision {
    pub is_occluded: bool,
    pub needs_render_after_occlusion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteRenderInput {
    pub disable_unfocused_render: bool,
    pub disable_occluded_render: bool,
    pub visibility: WindowVisibilityState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteRenderPolicy {
    pub skip: bool,
    pub consume_render_after_occlusion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOpenTarget {
    Split,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWindowDecision {
    FocusExisting,
    Create,
}

/// POD bag of cell, terminal-margin, and panel-edge dimensions used to
/// translate configured grid columns/rows into a physical window size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGridSizeDims {
    pub cell_width: f32,
    pub cell_height: f32,
    pub scale: f32,
    pub terminal_margin_left: f32,
    pub terminal_margin_right: f32,
    pub terminal_margin_top: f32,
    pub terminal_margin_bottom: f32,
    pub panel_padding_left: f32,
    pub panel_padding_right: f32,
    pub panel_padding_top: f32,
    pub panel_padding_bottom: f32,
    pub panel_margin_left: f32,
    pub panel_margin_right: f32,
    pub panel_margin_top: f32,
    pub panel_margin_bottom: f32,
}

pub fn route_error_target(configuration_not_found: bool) -> RouteErrorTarget {
    if configuration_not_found {
        RouteErrorTarget::Welcome
    } else {
        RouteErrorTarget::AssistantOverlay
    }
}

pub fn window_focus_decision(was_focused: bool, is_focused: bool) -> WindowFocusDecision {
    WindowFocusDecision {
        is_focused,
        request_redraw: !was_focused && is_focused,
    }
}

pub fn window_occlusion_decision(
    was_occluded: bool,
    is_occluded: bool,
    needs_render_after_occlusion: bool,
) -> WindowOcclusionDecision {
    WindowOcclusionDecision {
        is_occluded,
        needs_render_after_occlusion: needs_render_after_occlusion
            || (was_occluded && !is_occluded),
    }
}

pub fn should_render_visible_window(
    disable_unfocused_render: bool,
    disable_occluded_render: bool,
    state: WindowVisibilityState,
) -> bool {
    if disable_unfocused_render && !state.is_focused {
        return false;
    }

    if disable_occluded_render && state.is_occluded && !state.needs_render_after_occlusion
    {
        return false;
    }

    true
}

pub const fn route_render_policy(input: RouteRenderInput) -> RouteRenderPolicy {
    let skip_unfocused = input.disable_unfocused_render && !input.visibility.is_focused;
    let skip_occluded = input.disable_occluded_render
        && input.visibility.is_occluded
        && !input.visibility.needs_render_after_occlusion;

    RouteRenderPolicy {
        skip: skip_unfocused || skip_occluded,
        consume_render_after_occlusion: input.visibility.needs_render_after_occlusion
            && !skip_unfocused
            && !skip_occluded,
    }
}

pub const fn route_needs_event_loop_redraw(
    screen: RouteScreen,
    pending_dirty: bool,
    animating: bool,
) -> bool {
    matches!(screen, RouteScreen::Welcome | RouteScreen::ConfirmQuit)
        || pending_dirty
        || animating
}

pub fn animation_frame_delta(
    now_elapsed_since_last_frame: Duration,
    fallback_interval: Duration,
) -> Duration {
    if now_elapsed_since_last_frame.is_zero() {
        fallback_interval
    } else {
        now_elapsed_since_last_frame.min(MAX_ANIMATION_FRAME_DELTA)
    }
}

pub fn vblank_interval_from_refresh_rate(
    refresh_rate_millihertz: u32,
) -> (Duration, f64) {
    let refresh_rate_hz = refresh_rate_millihertz as f64 / 1000.0;
    let refresh_rate_hz = refresh_rate_hz.max(1.0);
    let frame_time_ns = (1_000_000_000.0 / refresh_rate_hz)
        .round()
        .clamp(1.0, u64::MAX as f64) as u64;
    (Duration::from_nanos(frame_time_ns), refresh_rate_hz)
}

pub fn route_wait_until(
    needs_render_after_occlusion: bool,
    elapsed_since_render: Duration,
    vblank_interval: Duration,
) -> Option<Duration> {
    if needs_render_after_occlusion || vblank_interval.is_zero() {
        return None;
    }

    let vblank_nanos = vblank_interval.as_nanos();
    let frames_elapsed = elapsed_since_render.as_nanos() / vblank_nanos;
    let next_frame_offset =
        Duration::from_nanos((frames_elapsed + 1) as u64 * vblank_nanos as u64);

    if next_frame_offset > elapsed_since_render {
        Some(next_frame_offset - elapsed_since_render)
    } else {
        None
    }
}

pub fn route_input_decision(
    screen: RouteScreen,
    assistant_overlay_active: bool,
    key_pressed: bool,
    key: &RouteKey,
) -> RouteInputDecision {
    if screen == RouteScreen::Terminal {
        return RouteInputDecision::PassThrough;
    }

    if assistant_overlay_active {
        if key_pressed && matches!(key, RouteKey::Enter) {
            return RouteInputDecision::DismissAssistantOverlay;
        }
        return RouteInputDecision::Consume;
    }

    if screen == RouteScreen::ConfirmQuit {
        if !key_pressed {
            return RouteInputDecision::Consume;
        }
        return match key {
            RouteKey::Escape => RouteInputDecision::CancelConfirmQuit,
            RouteKey::Character(text) if is_confirm_no(text) => {
                RouteInputDecision::CancelConfirmQuit
            }
            RouteKey::Character(text) if is_confirm_yes(text) => {
                RouteInputDecision::AcceptConfirmQuit
            }
            _ => RouteInputDecision::Consume,
        };
    }

    if screen == RouteScreen::Welcome && key_pressed && matches!(key, RouteKey::Enter) {
        return RouteInputDecision::CreateConfigAndEnterTerminal;
    }

    RouteInputDecision::PassThrough
}

pub fn selected_focused_route<RouteId: Copy, Routes>(routes: Routes) -> Option<RouteId>
where
    Routes: IntoIterator<Item = (RouteId, bool)>,
{
    routes.into_iter().find_map(
        |(route_id, is_focused)| {
            if is_focused {
                Some(route_id)
            } else {
                None
            }
        },
    )
}

pub const fn config_open_target(open_with_split: bool) -> ConfigOpenTarget {
    if open_with_split {
        ConfigOpenTarget::Split
    } else {
        ConfigOpenTarget::Window
    }
}

pub const fn config_window_decision(
    existing_route_is_open: bool,
) -> ConfigWindowDecision {
    if existing_route_is_open {
        ConfigWindowDecision::FocusExisting
    } else {
        ConfigWindowDecision::Create
    }
}

pub const fn centered_window_position(
    monitor_x: i32,
    monitor_y: i32,
    monitor_w: u32,
    monitor_h: u32,
    width: u32,
    height: u32,
) -> (i32, i32) {
    let x = monitor_x + (monitor_w as i32 - width as i32) / 2;
    let y = monitor_y + (monitor_h as i32 - height as i32) / 2;
    (x, y)
}

pub fn compute_window_size_from_grid_dims(
    columns: Option<u16>,
    rows: Option<u16>,
    dims: &WindowGridSizeDims,
    default_window_width: u32,
    default_window_height: u32,
    min_physical_width: u32,
    min_physical_height: u32,
) -> (u32, u32) {
    let scale = dims.scale;
    let scale_u32 = scale.round().max(1.0) as u32;

    let physical_width = match columns {
        Some(columns) if columns > 0 => {
            let margin = (dims.terminal_margin_left + dims.terminal_margin_right) * scale;
            let panel_edge = (dims.panel_padding_left
                + dims.panel_padding_right
                + dims.panel_margin_left
                + dims.panel_margin_right)
                * scale;
            let raw = (columns as f32 * dims.cell_width).ceil() as u32
                + margin as u32
                + panel_edge as u32;
            raw.next_multiple_of(scale_u32)
        }
        _ => default_window_width,
    };

    let physical_height = match rows {
        Some(rows) if rows > 0 => {
            let margin = (dims.terminal_margin_top + dims.terminal_margin_bottom) * scale;
            let panel_edge = (dims.panel_padding_top
                + dims.panel_padding_bottom
                + dims.panel_margin_top
                + dims.panel_margin_bottom)
                * scale;
            let raw = (rows as f32 * dims.cell_height).ceil() as u32
                + margin as u32
                + panel_edge as u32;
            raw.next_multiple_of(scale_u32)
        }
        _ => default_window_height,
    };

    (
        physical_width.max(min_physical_width),
        physical_height.max(min_physical_height),
    )
}

pub fn frame_over_budget(interval: Duration, target_interval: Duration) -> bool {
    if target_interval.is_zero() {
        return false;
    }

    let threshold = target_interval + target_interval / 2;
    interval > threshold
}

/// Rolling inter-frame + render-duration statistics, logged once every
/// `MIN_LOG_SAMPLES` frames. Shared so both the native event loop and
/// the web frame-pacing layer can record the same cadence summary.
#[derive(Debug, Clone)]
pub struct FrameCadenceStats {
    last_frame_at: Option<Instant>,
    window_started_at: Instant,
    samples: u32,
    over_budget_samples: u32,
    total_interval: Duration,
    min_interval: Option<Duration>,
    max_interval: Option<Duration>,
    render_samples: u32,
    total_render: Duration,
    min_render: Option<Duration>,
    max_render: Option<Duration>,
}

impl FrameCadenceStats {
    pub const MIN_LOG_SAMPLES: u32 = 300;

    pub fn new(now: Instant) -> Self {
        Self {
            last_frame_at: None,
            window_started_at: now,
            samples: 0,
            over_budget_samples: 0,
            total_interval: Duration::ZERO,
            min_interval: None,
            max_interval: None,
            render_samples: 0,
            total_render: Duration::ZERO,
            min_render: None,
            max_render: None,
        }
    }

    pub fn record_frame_start(&mut self, now: Instant, target_interval: Duration) {
        let Some(last_frame_at) = self.last_frame_at.replace(now) else {
            self.window_started_at = now;
            return;
        };

        let interval = now.saturating_duration_since(last_frame_at);
        self.samples += 1;
        self.total_interval += interval;
        self.min_interval =
            Some(self.min_interval.map_or(interval, |min| min.min(interval)));
        self.max_interval =
            Some(self.max_interval.map_or(interval, |max| max.max(interval)));

        if frame_over_budget(interval, target_interval) {
            self.over_budget_samples += 1;
        }
    }

    pub fn record_render_duration(&mut self, duration: Duration) {
        self.render_samples += 1;
        self.total_render += duration;
        self.min_render = Some(self.min_render.map_or(duration, |min| min.min(duration)));
        self.max_render = Some(self.max_render.map_or(duration, |max| max.max(duration)));
    }

    /// Snapshot the current sample buffer when it has enough samples to
    /// log; resets internal counters once returned. `None` until the
    /// `MIN_LOG_SAMPLES` threshold is reached. Callers do the actual
    /// `tracing::info!` to keep this struct free of any per-window-id
    /// type dependency.
    pub fn maybe_take_summary(
        &mut self,
        now: Instant,
        target_interval: Duration,
    ) -> Option<FrameCadenceSummary> {
        if self.samples < Self::MIN_LOG_SAMPLES {
            return None;
        }

        let avg_frame_ms =
            self.total_interval.as_secs_f64() * 1000.0 / f64::from(self.samples);
        let min_frame_ms = self.min_interval.unwrap_or_default().as_secs_f64() * 1000.0;
        let max_frame_ms = self.max_interval.unwrap_or_default().as_secs_f64() * 1000.0;
        let target_frame_ms = target_interval.as_secs_f64() * 1000.0;
        let avg_render_ms = if self.render_samples == 0 {
            0.0
        } else {
            self.total_render.as_secs_f64() * 1000.0 / f64::from(self.render_samples)
        };
        let min_render_ms = self.min_render.unwrap_or_default().as_secs_f64() * 1000.0;
        let max_render_ms = self.max_render.unwrap_or_default().as_secs_f64() * 1000.0;
        let wait_outside_render_ms = (avg_frame_ms - avg_render_ms).max(0.0);
        let elapsed_ms = now
            .saturating_duration_since(self.window_started_at)
            .as_secs_f64()
            * 1000.0;

        let summary = FrameCadenceSummary {
            target_frame_ms,
            avg_frame_ms,
            min_frame_ms,
            max_frame_ms,
            avg_render_ms,
            min_render_ms,
            max_render_ms,
            wait_outside_render_ms,
            samples: self.samples,
            render_samples: self.render_samples,
            over_budget_samples: self.over_budget_samples,
            elapsed_ms,
        };

        self.window_started_at = now;
        self.samples = 0;
        self.over_budget_samples = 0;
        self.total_interval = Duration::ZERO;
        self.min_interval = None;
        self.max_interval = None;
        self.render_samples = 0;
        self.total_render = Duration::ZERO;
        self.min_render = None;
        self.max_render = None;

        Some(summary)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameCadenceSummary {
    pub target_frame_ms: f64,
    pub avg_frame_ms: f64,
    pub min_frame_ms: f64,
    pub max_frame_ms: f64,
    pub avg_render_ms: f64,
    pub min_render_ms: f64,
    pub max_render_ms: f64,
    pub wait_outside_render_ms: f64,
    pub samples: u32,
    pub render_samples: u32,
    pub over_budget_samples: u32,
    pub elapsed_ms: f64,
}

fn is_confirm_no(text: &str) -> bool {
    matches!(text, "n" | "N")
}

fn is_confirm_yes(text: &str) -> bool {
    matches!(text, "y" | "Y")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_key_classification_prefers_enter_then_escape_then_text() {
        assert_eq!(classify_route_key(true, true, Some("y")), RouteKey::Enter);
        assert_eq!(classify_route_key(false, true, Some("y")), RouteKey::Escape);
        assert_eq!(
            classify_route_key(false, false, Some("y")),
            RouteKey::Character("y".into())
        );
        assert_eq!(classify_route_key(false, false, None), RouteKey::Other);
    }

    #[test]
    fn terminal_route_passes_through_even_with_assistant_overlay() {
        assert_eq!(
            route_input_decision(RouteScreen::Terminal, true, true, &RouteKey::Enter),
            RouteInputDecision::PassThrough
        );
    }

    #[test]
    fn assistant_overlay_consumes_non_enter_and_dismisses_on_pressed_enter() {
        assert_eq!(
            route_input_decision(RouteScreen::Welcome, true, true, &RouteKey::Other),
            RouteInputDecision::Consume
        );
        assert_eq!(
            route_input_decision(RouteScreen::Welcome, true, false, &RouteKey::Enter),
            RouteInputDecision::Consume
        );
        assert_eq!(
            route_input_decision(RouteScreen::Welcome, true, true, &RouteKey::Enter),
            RouteInputDecision::DismissAssistantOverlay
        );
    }

    #[test]
    fn confirm_quit_accepts_yes_and_cancels_no_or_escape() {
        assert_eq!(
            route_input_decision(
                RouteScreen::ConfirmQuit,
                false,
                true,
                &RouteKey::Character("y".into())
            ),
            RouteInputDecision::AcceptConfirmQuit
        );
        assert_eq!(
            route_input_decision(
                RouteScreen::ConfirmQuit,
                false,
                true,
                &RouteKey::Character("N".into())
            ),
            RouteInputDecision::CancelConfirmQuit
        );
        assert_eq!(
            route_input_decision(
                RouteScreen::ConfirmQuit,
                false,
                true,
                &RouteKey::Escape
            ),
            RouteInputDecision::CancelConfirmQuit
        );
    }

    #[test]
    fn confirm_quit_consumes_unknown_or_release_keys() {
        assert_eq!(
            route_input_decision(RouteScreen::ConfirmQuit, false, true, &RouteKey::Other),
            RouteInputDecision::Consume
        );
        assert_eq!(
            route_input_decision(
                RouteScreen::ConfirmQuit,
                false,
                false,
                &RouteKey::Character("y".into())
            ),
            RouteInputDecision::Consume
        );
    }

    #[test]
    fn welcome_enter_creates_config_then_enters_terminal() {
        assert_eq!(
            route_input_decision(RouteScreen::Welcome, false, true, &RouteKey::Enter),
            RouteInputDecision::CreateConfigAndEnterTerminal
        );
        assert_eq!(
            route_input_decision(RouteScreen::Welcome, false, true, &RouteKey::Other),
            RouteInputDecision::PassThrough
        );
    }

    #[test]
    fn configuration_not_found_routes_to_welcome() {
        assert_eq!(route_error_target(true), RouteErrorTarget::Welcome);
        assert_eq!(
            route_error_target(false),
            RouteErrorTarget::AssistantOverlay
        );
    }

    #[test]
    fn window_focus_decision_redraws_only_when_focus_is_regained() {
        assert_eq!(
            window_focus_decision(false, true),
            WindowFocusDecision {
                is_focused: true,
                request_redraw: true,
            }
        );
        assert_eq!(
            window_focus_decision(true, false),
            WindowFocusDecision {
                is_focused: false,
                request_redraw: false,
            }
        );
        assert_eq!(
            window_focus_decision(true, true),
            WindowFocusDecision {
                is_focused: true,
                request_redraw: false,
            }
        );
    }

    #[test]
    fn window_occlusion_decision_requests_one_visible_frame_on_unocclude() {
        assert_eq!(
            window_occlusion_decision(true, false, false),
            WindowOcclusionDecision {
                is_occluded: false,
                needs_render_after_occlusion: true,
            }
        );
        assert_eq!(
            window_occlusion_decision(false, true, false),
            WindowOcclusionDecision {
                is_occluded: true,
                needs_render_after_occlusion: false,
            }
        );
        assert_eq!(
            window_occlusion_decision(false, false, true),
            WindowOcclusionDecision {
                is_occluded: false,
                needs_render_after_occlusion: true,
            }
        );
    }

    #[test]
    fn visible_window_render_policy_honors_focus_and_occlusion_settings() {
        assert!(should_render_visible_window(
            true,
            true,
            WindowVisibilityState::visible_and_focused()
        ));
        assert!(!should_render_visible_window(
            true,
            false,
            WindowVisibilityState {
                is_focused: false,
                is_occluded: false,
                needs_render_after_occlusion: false,
            }
        ));
        assert!(!should_render_visible_window(
            false,
            true,
            WindowVisibilityState {
                is_focused: true,
                is_occluded: true,
                needs_render_after_occlusion: false,
            }
        ));
        assert!(should_render_visible_window(
            false,
            true,
            WindowVisibilityState {
                is_focused: true,
                is_occluded: true,
                needs_render_after_occlusion: true,
            }
        ));
    }

    #[test]
    fn route_render_policy_skips_and_consumes_occlusion_one_shot() {
        let focused = WindowVisibilityState::visible_and_focused();
        assert_eq!(
            route_render_policy(RouteRenderInput {
                disable_unfocused_render: true,
                disable_occluded_render: true,
                visibility: focused,
            }),
            RouteRenderPolicy {
                skip: false,
                consume_render_after_occlusion: false,
            }
        );

        assert_eq!(
            route_render_policy(RouteRenderInput {
                disable_unfocused_render: true,
                disable_occluded_render: false,
                visibility: WindowVisibilityState {
                    is_focused: false,
                    is_occluded: false,
                    needs_render_after_occlusion: false,
                },
            }),
            RouteRenderPolicy {
                skip: true,
                consume_render_after_occlusion: false,
            }
        );

        assert_eq!(
            route_render_policy(RouteRenderInput {
                disable_unfocused_render: false,
                disable_occluded_render: true,
                visibility: WindowVisibilityState {
                    is_focused: true,
                    is_occluded: true,
                    needs_render_after_occlusion: true,
                },
            }),
            RouteRenderPolicy {
                skip: false,
                consume_render_after_occlusion: true,
            }
        );
    }

    #[test]
    fn route_redraw_policy_includes_modal_routes_dirty_and_animation() {
        assert!(route_needs_event_loop_redraw(
            RouteScreen::Welcome,
            false,
            false
        ));
        assert!(route_needs_event_loop_redraw(
            RouteScreen::ConfirmQuit,
            false,
            false
        ));
        assert!(route_needs_event_loop_redraw(
            RouteScreen::Terminal,
            true,
            false
        ));
        assert!(route_needs_event_loop_redraw(
            RouteScreen::Terminal,
            false,
            true
        ));
        assert!(!route_needs_event_loop_redraw(
            RouteScreen::Terminal,
            false,
            false
        ));
    }

    #[test]
    fn animation_delta_uses_fallback_for_same_instant_and_caps_long_frames() {
        assert_eq!(
            animation_frame_delta(Duration::ZERO, Duration::from_millis(16)),
            Duration::from_millis(16)
        );
        assert_eq!(
            animation_frame_delta(Duration::from_millis(8), Duration::from_millis(16)),
            Duration::from_millis(8)
        );
        assert_eq!(
            animation_frame_delta(Duration::from_millis(80), Duration::from_millis(16)),
            MAX_ANIMATION_FRAME_DELTA
        );
    }

    #[test]
    fn route_wait_until_returns_next_vblank_delay_or_immediate() {
        assert_eq!(
            route_wait_until(false, Duration::from_millis(5), Duration::from_millis(16)),
            Some(Duration::from_millis(11))
        );
        assert_eq!(
            route_wait_until(false, Duration::from_millis(16), Duration::from_millis(16)),
            Some(Duration::from_millis(16))
        );
        assert_eq!(
            route_wait_until(true, Duration::from_millis(5), Duration::from_millis(16)),
            None
        );
        assert_eq!(
            route_wait_until(false, Duration::from_millis(5), Duration::ZERO),
            None
        );
    }

    #[test]
    fn focused_and_config_route_policies_are_deterministic() {
        assert_eq!(
            selected_focused_route([(1_u32, false), (2, true), (3, true)]),
            Some(2)
        );
        assert_eq!(selected_focused_route([(1_u32, false)]), None);
        assert_eq!(config_open_target(true), ConfigOpenTarget::Split);
        assert_eq!(config_open_target(false), ConfigOpenTarget::Window);
        assert_eq!(
            config_window_decision(true),
            ConfigWindowDecision::FocusExisting
        );
        assert_eq!(config_window_decision(false), ConfigWindowDecision::Create);
    }

    #[test]
    fn refresh_centering_grid_and_budget_math_are_shared() {
        let (interval, hz) = vblank_interval_from_refresh_rate(60_000);
        assert_eq!(interval, Duration::from_nanos(16_666_667));
        assert_eq!(hz, 60.0);
        assert_eq!(
            centered_window_position(100, 50, 1000, 800, 200, 100),
            (500, 400)
        );

        let dims = WindowGridSizeDims {
            cell_width: 10.0,
            cell_height: 20.0,
            scale: 2.0,
            terminal_margin_left: 4.0,
            terminal_margin_right: 3.0,
            terminal_margin_top: 5.0,
            terminal_margin_bottom: 2.0,
            panel_padding_left: 3.0,
            panel_padding_right: 2.0,
            panel_padding_top: 4.0,
            panel_padding_bottom: 1.0,
            panel_margin_left: 7.0,
            panel_margin_right: 6.0,
            panel_margin_top: 8.0,
            panel_margin_bottom: 5.0,
        };
        assert_eq!(
            compute_window_size_from_grid_dims(
                Some(10),
                Some(5),
                &dims,
                500,
                300,
                300,
                200
            ),
            (300, 200)
        );
        assert!(!frame_over_budget(
            Duration::from_millis(23),
            Duration::from_millis(16)
        ));
        assert!(frame_over_budget(
            Duration::from_millis(25),
            Duration::from_millis(16)
        ));
    }
}

/// Per-route POD the redraw scheduler reads when deciding whether a
/// route needs a wake-up. Native callers build one of these by
/// inspecting `route.window.render_policy(...)` / pending dirty /
/// editor scroll spring / etc.
#[derive(Debug, Clone, Copy)]
pub struct RouteRedrawState {
    /// Output of [`route_render_policy`].`skip` for this route.
    pub render_skip: bool,
    /// `renderable_content.pending_update.is_dirty()`.
    pub pending_dirty: bool,
    /// `window.screen.renderer.needs_redraw()` — true while a spring /
    /// streaming surface wants continuous frames.
    pub animating: bool,
    /// The route's animation deadline if it has one (e.g. spring tail
    /// next-frame time). `None` means schedule for the next frame now.
    pub wait_until: Option<web_time::Duration>,
    /// Route's screen kind, fed straight into
    /// [`route_needs_event_loop_redraw`].
    pub screen: RouteScreen,
}

/// Outcome of [`request_event_loop_redraws`].
#[derive(Debug, Default, Clone)]
pub struct RedrawSchedulerOutcome {
    /// Indices of routes that should be sent a `request_redraw()`
    /// call now. Animating routes with a future `wait_until` are left
    /// out until the next deadline so overlay animations cannot spin
    /// the compositor faster than the frame clock. Mirrors the
    /// iteration order the caller passed in.
    pub request_redraw: Vec<usize>,
    /// `Some(min over all `now + wait_until`)` across the woken routes,
    /// or `None` if nothing was scheduled.
    pub next_deadline: Option<web_time::Instant>,
}

/// Compute which routes need a `request_redraw()` call and what the
/// next wake-up deadline should be.
///
/// `now` is the caller's `Instant::now()`; the policy uses it as the
/// base for routes whose `wait_until == None` (treat as "ready now").
///
/// Routes are identified by index into the input slice so the native
/// side can drive its own per-route `request_redraw` after this returns.
pub fn request_event_loop_redraws(
    now: web_time::Instant,
    routes: &[RouteRedrawState],
) -> RedrawSchedulerOutcome {
    let mut out = RedrawSchedulerOutcome::default();
    for (index, route) in routes.iter().enumerate() {
        if route.render_skip {
            continue;
        }
        if !route_needs_event_loop_redraw(
            route.screen,
            route.pending_dirty,
            route.animating,
        ) {
            continue;
        }
        let deadline = match route.wait_until {
            Some(wait) => now + wait,
            None => now,
        };
        out.next_deadline =
            Some(out.next_deadline.map_or(deadline, |old| old.min(deadline)));
        if route.animating && route.wait_until.is_some() {
            continue;
        }
        out.request_redraw.push(index);
    }
    out
}

/// Combine the redraw-scheduler deadline with a scheduler-derived
/// deadline (timer queue's earliest pending instant) into the single
/// `min` deadline the event loop should wait on. Returns `None` when
/// neither side has anything pending — equivalent to `ControlFlow::Wait`.
pub fn combine_deadlines(
    redraw_deadline: Option<web_time::Instant>,
    scheduler_deadline: Option<web_time::Instant>,
) -> Option<web_time::Instant> {
    match (redraw_deadline, scheduler_deadline) {
        (Some(redraw), Some(scheduled)) => Some(redraw.min(scheduled)),
        (Some(redraw), None) => Some(redraw),
        (None, Some(scheduled)) => Some(scheduled),
        (None, None) => None,
    }
}

#[cfg(test)]
mod redraw_scheduler_tests {
    use super::*;
    use web_time::{Duration, Instant};

    fn state(
        screen: RouteScreen,
        render_skip: bool,
        pending_dirty: bool,
        animating: bool,
        wait_until: Option<Duration>,
    ) -> RouteRedrawState {
        RouteRedrawState {
            render_skip,
            pending_dirty,
            animating,
            wait_until,
            screen,
        }
    }

    #[test]
    fn skips_routes_with_render_skip() {
        let now = Instant::now();
        let routes = [
            state(RouteScreen::Terminal, true, true, false, None),
            state(RouteScreen::Terminal, false, false, false, None),
        ];
        let out = request_event_loop_redraws(now, &routes);
        assert!(out.request_redraw.is_empty());
        assert!(out.next_deadline.is_none());
    }

    #[test]
    fn welcome_always_wakes() {
        let now = Instant::now();
        let routes = [state(RouteScreen::Welcome, false, false, false, None)];
        let out = request_event_loop_redraws(now, &routes);
        assert_eq!(out.request_redraw, vec![0]);
        assert!(out.next_deadline.is_some());
    }

    #[test]
    fn animating_route_waits_for_vblank_before_requesting_redraw() {
        let now = Instant::now();
        let wait = Duration::from_millis(16);
        let routes = [state(RouteScreen::Terminal, false, false, true, Some(wait))];
        let out = request_event_loop_redraws(now, &routes);

        assert!(out.request_redraw.is_empty());
        assert_eq!(out.next_deadline, Some(now + wait));
    }

    #[test]
    fn dirty_non_animating_route_requests_redraw_immediately() {
        let now = Instant::now();
        let wait = Duration::from_millis(16);
        let routes = [state(RouteScreen::Terminal, false, true, false, Some(wait))];
        let out = request_event_loop_redraws(now, &routes);

        assert_eq!(out.request_redraw, vec![0]);
        assert_eq!(out.next_deadline, Some(now + wait));
    }

    #[test]
    fn combine_deadlines_picks_earliest() {
        let now = Instant::now();
        let later = now + Duration::from_millis(50);
        assert_eq!(combine_deadlines(Some(now), Some(later)), Some(now));
        assert_eq!(combine_deadlines(Some(now), None), Some(now));
        assert_eq!(combine_deadlines(None, Some(later)), Some(later));
        assert_eq!(combine_deadlines(None, None), None);
    }
}
