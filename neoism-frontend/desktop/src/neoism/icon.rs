// Tab-strip icons for agent CLIs (Claude Code, OpenAI Codex, OpenCode).
// When one of these is the foreground program in the terminal tab, the
// generic terminal glyph is replaced by the tool's logo.
//
// The POD identity bits (the `AgentKind` enum, panel ids, image ids)
// live in the shared crate so the web frontend can speak the same
// agent vocabulary without dragging desktop-only dependencies in.
// This file owns the asset bytes, `image_rs` decode, `sugarloaf` image
// upload, and Linux `/proc` foreground-process detection — none of
// which the web build needs or could compile.

use neoism_backend::sugarloaf::{
    ColorType, GraphicData, GraphicDataEntry, GraphicId, GraphicOverlay, Sugarloaf,
};

// Re-export the POD pieces so existing call sites
// (`crate::neoism::icon::AgentKind`, `ICON_PANEL_ID`, ...) keep
// resolving without any change.
pub use neoism_ui::panels::agent_pane::icon::{
    AgentKind, CLAUDE_IMAGE_ID, CODEX_IMAGE_ID, ICON_PANEL_ID, NEOISM_IMAGE_ID,
    OPENCODE_IMAGE_ID, SIDE_PANEL_ICON_PANEL_ID,
};

const CLAUDE_PNG: &[u8] = include_bytes!("../../assets/icons/claude.png");
const CODEX_PNG: &[u8] = include_bytes!("../../assets/icons/codex.png");
const OPENCODE_PNG: &[u8] = include_bytes!("../../assets/icons/opencode.png");
const NEOISM_PNG: &[u8] = include_bytes!("../../assets/icons/neoism.png");

/// Decode the embedded PNGs and upload them to sugarloaf's image
/// store. Returns `true` once all icons are registered. Idempotent —
/// safe to call every frame; subsequent calls return immediately.
pub fn register_agent_icons(sugarloaf: &mut Sugarloaf) -> bool {
    let entries: [(u32, &[u8]); 4] = [
        (CLAUDE_IMAGE_ID, CLAUDE_PNG),
        (CODEX_IMAGE_ID, CODEX_PNG),
        (OPENCODE_IMAGE_ID, OPENCODE_PNG),
        (NEOISM_IMAGE_ID, NEOISM_PNG),
    ];
    for (id, bytes) in entries {
        if sugarloaf.image_data.contains_key(&id) {
            continue;
        }
        let img = match image_rs::load_from_memory(bytes) {
            Ok(i) => i.to_rgba8(),
            Err(_) => return false,
        };
        let (w, h) = img.dimensions();
        let pixels = img.into_raw();
        let entry = GraphicDataEntry::from_graphic_data(GraphicData {
            id: GraphicId::new(id as u64),
            width: w as usize,
            height: h as usize,
            color_type: ColorType::Rgba,
            pixels,
            is_opaque: false,
            resize: None,
            display_width: None,
            display_height: None,
            transmit_time: std::time::Instant::now(),
        });
        sugarloaf.image_data.insert(id, entry);
    }
    true
}

pub fn push_cropped_icon_overlay(
    sugarloaf: &mut Sugarloaf,
    kind: AgentKind,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    source_rect: [f32; 4],
) {
    push_icon_overlay_to_panel_with_options(
        sugarloaf,
        ICON_PANEL_ID,
        kind,
        x,
        y,
        width,
        height,
        1,
        source_rect,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_icon_overlay_to_panel_with_options(
    sugarloaf: &mut Sugarloaf,
    panel_id: usize,
    kind: AgentKind,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    z_index: i32,
    source_rect: [f32; 4],
) {
    let scale = sugarloaf.scale_factor();
    sugarloaf.push_image_overlay(
        panel_id,
        GraphicOverlay {
            image_id: kind.image_id(),
            x: x * scale,
            y: y * scale,
            width: width * scale,
            height: height * scale,
            z_index,
            source_rect,
        },
    );
}

pub fn clear_icon_overlays(sugarloaf: &mut Sugarloaf) {
    sugarloaf.clear_image_overlays_for(ICON_PANEL_ID);
}

pub fn clear_side_panel_icon_overlays(sugarloaf: &mut Sugarloaf) {
    sugarloaf.clear_image_overlays_for(SIDE_PANEL_ICON_PANEL_ID);
}

/// Look at the foreground process group on `main_fd` and decide whether
/// it's one of the supported agents. Reads `/proc/<pgid>/comm` and
/// `/proc/<pgid>/cmdline` once per call — cheap, but the caller should
/// throttle to avoid running every frame.
#[cfg(target_os = "linux")]
pub fn detect_agent(
    main_fd: std::os::unix::io::RawFd,
    _shell_pid: u32,
) -> Option<AgentKind> {
    use std::os::raw::c_int;

    // tcgetpgrp returns the foreground process group id for the
    // controlling tty. With multiple processes in the chain (npx → node
    // → native binary, as with codex), the pgid is the leader's pid;
    // we read both `comm` and `cmdline` so the agent matches whether
    // it's run directly or via a wrapper.
    let pgid: c_int = unsafe { libc::tcgetpgrp(main_fd) };
    if pgid <= 0 {
        return None;
    }
    let pgid = pgid as u32;

    let comm = std::fs::read_to_string(format!("/proc/{pgid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let cmdline_bytes =
        std::fs::read(format!("/proc/{pgid}/cmdline")).unwrap_or_default();
    let cmdline = String::from_utf8_lossy(&cmdline_bytes);

    if comm == "claude"
        || cmdline_arg_is(&cmdline, "claude")
        || cmdline.contains("/claude\0")
    {
        return Some(AgentKind::Claude);
    }
    if comm == "opencode"
        || cmdline_arg_is(&cmdline, "opencode")
        || cmdline.contains("/opencode\0")
    {
        return Some(AgentKind::OpenCode);
    }
    if comm == "codex"
        || cmdline_arg_is(&cmdline, "codex")
        || cmdline.contains("/codex\0")
        || cmdline.contains("@openai/codex")
    {
        return Some(AgentKind::Codex);
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn detect_agent(_main_fd: i32, _shell_pid: u32) -> Option<AgentKind> {
    None
}

/// Returns true if any NUL-separated arg in `cmdline` (or its basename)
/// equals `name`.
#[cfg(target_os = "linux")]
fn cmdline_arg_is(cmdline: &str, name: &str) -> bool {
    cmdline.split('\0').any(|arg| {
        let basename = arg.rsplit('/').next().unwrap_or(arg);
        basename == name
    })
}
