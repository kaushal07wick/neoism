//! Platform-neutral UI event vocabulary.
//!
//! `UiEvent` is the sole interface between the host (winit on native,
//! DOM listeners on web) and the panels. Panels never see
//! `winit::WindowEvent` or `web_sys::KeyboardEvent` directly.
//!
//! All variants are `Serialize + Deserialize` so events can be
//! shipped over a wire (e.g. recorded for replay, or piped from a
//! browser tab through the workspace daemon).

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use web_time::Duration;

bitflags::bitflags! {
    /// Modifier keys held during an input event.
    ///
    /// Bit names match the cross-platform conventions: `SHIFT`,
    /// `CTRL`, `ALT`, and `META` (called "super" on Linux/X11 and
    /// "command" on macOS). The host normalizes per-platform key
    /// differences before constructing `Modifiers`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
    pub struct Modifiers: u8 {
        const SHIFT = 1 << 0;
        const CTRL  = 1 << 1;
        const ALT   = 1 << 2;
        const META  = 1 << 3;
    }
}

/// Named keys that don't produce a printable character.
///
/// Mirrors the subset of `winit::keyboard::NamedKey` /
/// `KeyboardEvent.key` strings that chrome panels actually consume.
/// New variants append; old variants never change ordinal so the
/// shape is wire-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NamedKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    Space,
    /// Function keys F1..=F24. Hosts that see higher Fn numbers clip
    /// to the supported range before emitting.
    Function(u8),
}

/// Logical key — what the OS thinks the user pressed, after keymap
/// translation. `Character` carries the produced grapheme cluster
/// (an `SmolStr` to avoid allocating for the common single-codepoint
/// case). `Unidentified` is for keys the host couldn't classify.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogicalKey {
    Named(NamedKey),
    Character(SmolStr),
    Unidentified,
}

/// Opaque physical key code. On native this is the winit scan code;
/// on web it is a hash of `KeyboardEvent.code`. Panels should prefer
/// `LogicalKey` for action mapping and only fall back to this for
/// raw game-style "by-location" handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhysicalKey(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyDescriptor {
    pub physical: PhysicalKey,
    pub logical: LogicalKey,
    pub state: KeyState,
    pub modifiers: Modifiers,
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// How the host reports wheel deltas.
///
/// `Pixel` is the normalized native form. `Line` and `Page` match
/// `WheelEvent.deltaMode` on web; panels typically convert these to
/// pixels using the active text size at hit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WheelMode {
    Pixel,
    Line,
    Page,
}

/// IME composition lifecycle.
///
/// `Start` opens a pre-edit session. `Update` carries the in-flight
/// pre-edit string and the cursor index within it. `Commit` delivers
/// the final text that should be inserted (also emitted as
/// `UiEvent::Text` by the host so non-IME-aware panels still see
/// it). `End` closes the session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompositionEvent {
    Start,
    Update { preedit: String, cursor: usize },
    Commit(String),
    End,
}

/// Theme change notification. `palette_dirty` means the color tokens
/// changed (system light/dark flip or user theme swap);
/// `scale_changed` carries the new DPI scale when it differs from
/// the previously reported value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThemeChange {
    pub palette_dirty: bool,
    pub scale_changed: Option<f32>,
}

/// The single event vocabulary panels consume.
///
/// `ServiceReply` is how the asynchronous web case re-enters the
/// panel after a service trait returned `IoError::Pending(req_id)`:
/// the host delivers the resolved payload as a `ServiceReply` with
/// the same request id, and the panel re-runs its handler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiEvent {
    Key(KeyDescriptor),
    Text(String),
    Composition(CompositionEvent),
    PointerMove {
        x: f32,
        y: f32,
        modifiers: Modifiers,
    },
    PointerDown {
        button: PointerButton,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        click_count: u8,
    },
    PointerUp {
        button: PointerButton,
        x: f32,
        y: f32,
        modifiers: Modifiers,
    },
    PointerLeave,
    Wheel {
        dx: f32,
        dy: f32,
        mode: WheelMode,
        modifiers: Modifiers,
    },
    Focus(bool),
    Resize {
        w: u32,
        h: u32,
        scale: f32,
    },
    Theme(ThemeChange),
    Tick(Duration),
    ServiceReply {
        request_id: u64,
        payload: serde_json::Value,
    },
}
