//! Skeleton sanity tests for `neoism-ui`.
//!
//! - Every `UiEvent` variant constructs and roundtrips through
//!   `serde_json` losslessly.
//! - `ChromeTheme::default()` (the placeholder until `from_snapshot`
//!   lands) produces non-zero color tokens for every field.

use std::time::Duration;

use neoism_ui::{
    ChromeTheme, CompositionEvent, KeyDescriptor, KeyState, LogicalKey, Modifiers,
    NamedKey, PhysicalKey, PointerButton, RgbTriple, ThemeChange, UiEvent, WheelMode,
};
use smol_str::SmolStr;

fn roundtrip(event: &UiEvent) {
    let encoded = serde_json::to_string(event).expect("serialize");
    let decoded: UiEvent = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(event, &decoded, "roundtrip mismatch for {event:?}");
}

#[test]
fn ui_event_variants_roundtrip() {
    let events = vec![
        UiEvent::Key(KeyDescriptor {
            physical: PhysicalKey(0x1e),
            logical: LogicalKey::Character(SmolStr::new_inline("a")),
            state: KeyState::Pressed,
            modifiers: Modifiers::SHIFT | Modifiers::CTRL,
            repeat: false,
        }),
        UiEvent::Key(KeyDescriptor {
            physical: PhysicalKey(0x1c),
            logical: LogicalKey::Named(NamedKey::Enter),
            state: KeyState::Released,
            modifiers: Modifiers::empty(),
            repeat: true,
        }),
        UiEvent::Key(KeyDescriptor {
            physical: PhysicalKey(0),
            logical: LogicalKey::Unidentified,
            state: KeyState::Pressed,
            modifiers: Modifiers::META,
            repeat: false,
        }),
        UiEvent::Key(KeyDescriptor {
            physical: PhysicalKey(0x3b),
            logical: LogicalKey::Named(NamedKey::Function(5)),
            state: KeyState::Pressed,
            modifiers: Modifiers::empty(),
            repeat: false,
        }),
        UiEvent::Text("hello".into()),
        UiEvent::Composition(CompositionEvent::Start),
        UiEvent::Composition(CompositionEvent::Update {
            preedit: "ni".into(),
            cursor: 2,
        }),
        UiEvent::Composition(CompositionEvent::Commit("你".into())),
        UiEvent::Composition(CompositionEvent::End),
        UiEvent::PointerMove {
            x: 12.5,
            y: 34.0,
            modifiers: Modifiers::ALT,
        },
        UiEvent::PointerDown {
            button: PointerButton::Left,
            x: 1.0,
            y: 2.0,
            modifiers: Modifiers::empty(),
            click_count: 2,
        },
        UiEvent::PointerUp {
            button: PointerButton::Other(7),
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::SHIFT,
        },
        UiEvent::PointerLeave,
        UiEvent::Wheel {
            dx: -2.0,
            dy: 4.0,
            mode: WheelMode::Pixel,
            modifiers: Modifiers::empty(),
        },
        UiEvent::Wheel {
            dx: 0.0,
            dy: 1.0,
            mode: WheelMode::Line,
            modifiers: Modifiers::CTRL,
        },
        UiEvent::Wheel {
            dx: 0.0,
            dy: 1.0,
            mode: WheelMode::Page,
            modifiers: Modifiers::empty(),
        },
        UiEvent::Focus(true),
        UiEvent::Focus(false),
        UiEvent::Resize {
            w: 1920,
            h: 1080,
            scale: 1.5,
        },
        UiEvent::Theme(ThemeChange {
            palette_dirty: true,
            scale_changed: Some(2.0),
        }),
        UiEvent::Theme(ThemeChange {
            palette_dirty: false,
            scale_changed: None,
        }),
        UiEvent::Tick(Duration::from_millis(16)),
        UiEvent::ServiceReply {
            request_id: 42,
            payload: serde_json::json!({ "ok": true, "rows": [1, 2, 3] }),
        },
    ];

    for ev in &events {
        roundtrip(ev);
    }
}

#[test]
fn pointer_button_variants_roundtrip() {
    for button in [
        PointerButton::Left,
        PointerButton::Right,
        PointerButton::Middle,
        PointerButton::Back,
        PointerButton::Forward,
        PointerButton::Other(13),
    ] {
        let ev = UiEvent::PointerDown {
            button,
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::empty(),
            click_count: 1,
        };
        roundtrip(&ev);
    }
}

#[test]
fn named_key_variants_roundtrip() {
    for named in [
        NamedKey::Enter,
        NamedKey::Tab,
        NamedKey::Escape,
        NamedKey::Backspace,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
        NamedKey::Home,
        NamedKey::End,
        NamedKey::PageUp,
        NamedKey::PageDown,
        NamedKey::Delete,
        NamedKey::Insert,
        NamedKey::Space,
        NamedKey::Function(1),
        NamedKey::Function(12),
    ] {
        let ev = UiEvent::Key(KeyDescriptor {
            physical: PhysicalKey(0),
            logical: LogicalKey::Named(named),
            state: KeyState::Pressed,
            modifiers: Modifiers::empty(),
            repeat: false,
        });
        roundtrip(&ev);
    }
}

#[test]
fn chrome_theme_default_is_populated() {
    let theme = ChromeTheme::default();
    // Every token must be a non-zero color so chrome never paints a
    // pitch-black slot when running before a real theme resolves.
    for (name, c) in [
        ("bg", theme.bg),
        ("bg_elevated", theme.bg_elevated),
        ("fg", theme.fg),
        ("fg_dim", theme.fg_dim),
        ("accent", theme.accent),
        ("border", theme.border),
        ("error", theme.error),
        ("success", theme.success),
    ] {
        assert!(
            !(c.r == 0 && c.g == 0 && c.b == 0),
            "{name} resolved to pure black"
        );
    }

    // The struct can be constructed via plain literal.
    let made = RgbTriple { r: 1, g: 2, b: 3 };
    assert_eq!(made, RgbTriple { r: 1, g: 2, b: 3 });
}
