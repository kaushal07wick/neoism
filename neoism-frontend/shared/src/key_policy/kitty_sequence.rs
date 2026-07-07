//! Shared kitty-keyboard-protocol escape sequence builder.
//!
//! Originally `build_key_sequence` in
//! `frontends/neoism/src/input/kitty_keyboard.rs` (which itself came
//! from alacritty under Apache 2.0). Lifted here so the web frontend
//! can produce the same PTY bytes from its own keyboard event source —
//! the desktop fork now translates the winit `KeyEvent` to the POD
//! [`KittyKeyEvent`] / [`KittyKeyName`] vocabulary and calls
//! [`build`].
//!
//! What lives here:
//!
//! * [`KittyKeyName`] — kitty-spec named key enum covering every
//!   functional key the protocol assigns a numeric base to (F1–F35,
//!   navigation, modifiers, media, audio, lock keys).
//! * [`KittyLogicalKey`] — either a character (with its base
//!   "unshifted" form for alternate-key reporting) or a [`KittyKeyName`].
//! * [`KittyKeyEvent`] — the POD event the builder consumes: logical
//!   key, location flag, press/release/repeat state, optional
//!   text-with-all-modifiers.
//! * [`SequenceModifiers`] — the kitty modifier bitset
//!   (Shift / Alt / Control / Super) plus the `encode_esc_sequence`
//!   wire format.
//! * [`SequenceBase`], [`SequenceTerminator`], [`SequenceBuilder`]
//!   — the internal builder broken into the four named subcases
//!   (numpad / kitty-named / normal-named / control-or-mod / textual).
//! * [`build`] — top-level entrypoint mirroring
//!   `build_key_sequence`.
//!
//! What stays in the desktop fork:
//!
//! * The winit `KeyEvent` / `Key` / `ModifiersState` /
//!   `KeyEventExtModifierSupplement` types.
//! * The macOS Fn+Delete workaround (it consults `key.logical_key`
//!   before extracting `text_with_all_modifiers`).
//! * The `From<ModifiersState>` for [`SequenceModifiers`] conversion.

use crate::key_policy::{
    classify_terminal_keyboard_input_mode, should_report_terminal_associated_text,
    TerminalKeyboardInputMode,
};
use std::borrow::Cow;

/// Kitty keyboard protocol named keys.
///
/// Covers every `NamedKey` the original builder pattern-matched on.
/// Hosts translate their platform `NamedKey` (winit / DOM
/// `KeyboardEvent.code` mapping) into this enum before building the
/// sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyKeyName {
    Enter,
    Escape,
    Tab,
    Backspace,
    Space,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    F26,
    F27,
    F28,
    F29,
    F30,
    F31,
    F32,
    F33,
    F34,
    F35,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
    ContextMenu,
    Shift,
    Control,
    Alt,
    Super,
    Hyper,
    Meta,
    MediaPlay,
    MediaPause,
    MediaPlayPause,
    MediaStop,
    MediaFastForward,
    MediaRewind,
    MediaTrackNext,
    MediaTrackPrevious,
    MediaRecord,
    AudioVolumeDown,
    AudioVolumeUp,
    AudioVolumeMute,
}

/// Logical key payload the builder consumes.
///
/// `Character` carries both the produced character (possibly upper-cased
/// when shift is held) and the base "key without modifiers" character so
/// the kitty alternate-key reporting branch can compute the alt code
/// without re-querying the platform key map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KittyLogicalKey {
    /// Single character or longer string produced by the key event.
    ///
    /// `base` is the character produced when no modifiers are applied
    /// (e.g. `'1'` for the `'!'` key). When `None` the builder falls
    /// back to lowercasing the produced character itself.
    Character {
        text: String,
        base: Option<String>,
    },
    Named(KittyKeyName),
    /// Anything else (dead keys, IME pre-edit only, etc.).
    Unidentified,
}

/// Press / release / repeat state of the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyKeyState {
    Pressed,
    Released,
}

impl KittyKeyState {
    pub fn is_pressed(self) -> bool {
        matches!(self, KittyKeyState::Pressed)
    }

    pub fn is_released(self) -> bool {
        matches!(self, KittyKeyState::Released)
    }
}

/// POD key event consumed by [`build`].
///
/// Mirrors the subset of `neoism_window::event::KeyEvent` the original
/// builder relied on. `text_with_all_modifiers` is the raw text payload
/// (post macOS Fn+Delete workaround on the desktop fork); the shared
/// builder applies the standard kitty associated-text gating on top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyKeyEvent {
    pub logical_key: KittyLogicalKey,
    pub state: KittyKeyState,
    pub repeat: bool,
    pub numpad_location: bool,
    pub text_with_all_modifiers: Option<String>,
}

/// Build a kitty-keyboard-protocol escape sequence for `key`.
///
/// Returns an empty `Vec<u8>` if the event has no representation in the
/// active mode — exactly the contract `build_key_sequence` always had.
#[inline(never)]
pub fn build(
    key: &KittyKeyEvent,
    mods: SequenceModifiers,
    report_all_keys_as_esc: bool,
    disambiguate_esc_codes: bool,
    report_event_types: bool,
    report_alternate_keys: bool,
    report_associated_text: bool,
) -> Vec<u8> {
    let mut modifiers = mods;

    let input_mode = classify_terminal_keyboard_input_mode(
        report_all_keys_as_esc,
        disambiguate_esc_codes,
        report_event_types,
        report_alternate_keys,
        report_associated_text,
        key.repeat,
        key.state.is_released(),
    );

    let context = SequenceBuilder {
        modifiers,
        input_mode,
    };

    let associated_text = key.text_with_all_modifiers.as_deref().filter(|text| {
        should_report_terminal_associated_text(input_mode, key.state.is_released(), text)
    });

    let sequence_base = context
        .try_build_numpad(key)
        .or_else(|| context.try_build_named_kitty(key))
        .or_else(|| context.try_build_named_normal(key, associated_text.is_some()))
        .or_else(|| context.try_build_control_char_or_mod(key, &mut modifiers))
        .or_else(|| context.try_build_textual(key, associated_text));

    let (payload, terminator) = match sequence_base {
        Some(SequenceBase {
            payload,
            terminator,
        }) => (payload, terminator),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{payload}");

    // Add modifiers information.
    if input_mode.kitty_event_type || !modifiers.is_empty() || associated_text.is_some() {
        payload.push_str(&format!(";{}", modifiers.encode_esc_sequence()));
    }

    // Push event type.
    if input_mode.kitty_event_type {
        payload.push(':');
        let event_type = match key.state {
            _ if key.repeat => '2',
            KittyKeyState::Pressed => '1',
            KittyKeyState::Released => '3',
        };
        payload.push(event_type);
    }

    if let Some(text) = associated_text {
        let mut codepoints = text.chars().map(u32::from);
        if let Some(codepoint) = codepoints.next() {
            payload.push_str(&format!(";{codepoint}"));
        }
        for codepoint in codepoints {
            payload.push_str(&format!(":{codepoint}"));
        }
    }

    payload.push(terminator.encode_esc_sequence());

    payload.into_bytes()
}

/// Helper to build escape sequence payloads from a [`KittyKeyEvent`].
pub struct SequenceBuilder {
    pub input_mode: TerminalKeyboardInputMode,
    pub modifiers: SequenceModifiers,
}

impl SequenceBuilder {
    /// Try building sequence from the event's emitting text.
    fn try_build_textual(
        &self,
        key: &KittyKeyEvent,
        associated_text: Option<&str>,
    ) -> Option<SequenceBase> {
        let (character, base) = match &key.logical_key {
            KittyLogicalKey::Character { text, base }
                if self.input_mode.kitty_sequence =>
            {
                (text.as_str(), base.as_deref())
            }
            _ => return None,
        };

        if character.chars().count() == 1 {
            let shift = self.modifiers.contains(SequenceModifiers::SHIFT);
            let ch = character.chars().next().unwrap();
            let unshifted_ch = if shift {
                ch.to_lowercase().next().unwrap()
            } else {
                ch
            };
            let alternate_key_code = u32::from(ch);
            let mut unicode_key_code = u32::from(unshifted_ch);

            // Try to get the base for keys which change based on modifier, like `1` for `!`.
            //
            // However it should only be performed when `SHIFT` is pressed.
            if shift && alternate_key_code == unicode_key_code {
                if let Some(unmodded) = base {
                    unicode_key_code =
                        u32::from(unmodded.chars().next().unwrap_or(unshifted_ch));
                }
            }

            // NOTE: Base layouts are ignored, since winit doesn't expose this information
            // yet.
            let payload = if self.input_mode.report_alternate_keys
                && alternate_key_code != unicode_key_code
            {
                format!("{unicode_key_code}:{alternate_key_code}")
            } else {
                unicode_key_code.to_string()
            };

            Some(SequenceBase::new(payload.into(), SequenceTerminator::Kitty))
        } else if self.input_mode.kitty_encode_all && associated_text.is_some() {
            // Fallback when need to report text, but we don't have any key associated with this
            // text.
            Some(SequenceBase::new("0".into(), SequenceTerminator::Kitty))
        } else {
            None
        }
    }

    /// Try building from numpad key.
    ///
    /// `None` is returned when the key is neither known nor numpad.
    fn try_build_numpad(&self, key: &KittyKeyEvent) -> Option<SequenceBase> {
        if !self.input_mode.kitty_sequence || !key.numpad_location {
            return None;
        }

        let base = match &key.logical_key {
            KittyLogicalKey::Character { text, .. } => match text.as_str() {
                "0" => "57399",
                "1" => "57400",
                "2" => "57401",
                "3" => "57402",
                "4" => "57403",
                "5" => "57404",
                "6" => "57405",
                "7" => "57406",
                "8" => "57407",
                "9" => "57408",
                "." => "57409",
                "/" => "57410",
                "*" => "57411",
                "-" => "57412",
                "+" => "57413",
                "=" => "57415",
                _ => return None,
            },
            KittyLogicalKey::Named(named) => match named {
                KittyKeyName::Enter => "57414",
                KittyKeyName::ArrowLeft => "57417",
                KittyKeyName::ArrowRight => "57418",
                KittyKeyName::ArrowUp => "57419",
                KittyKeyName::ArrowDown => "57420",
                KittyKeyName::PageUp => "57421",
                KittyKeyName::PageDown => "57422",
                KittyKeyName::Home => "57423",
                KittyKeyName::End => "57424",
                KittyKeyName::Insert => "57425",
                KittyKeyName::Delete => "57426",
                _ => return None,
            },
            KittyLogicalKey::Unidentified => return None,
        };

        Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
    }

    /// Try building from a [`KittyKeyName`] using the kitty keyboard
    /// protocol encoding for functional keys.
    fn try_build_named_kitty(&self, key: &KittyKeyEvent) -> Option<SequenceBase> {
        let named = match &key.logical_key {
            KittyLogicalKey::Named(named) if self.input_mode.kitty_sequence => *named,
            _ => return None,
        };

        let (base, terminator) = match named {
            // F3 in kitty protocol diverges from alacritty's terminfo.
            KittyKeyName::F3 => ("13", SequenceTerminator::Normal('~')),
            KittyKeyName::F13 => ("57376", SequenceTerminator::Kitty),
            KittyKeyName::F14 => ("57377", SequenceTerminator::Kitty),
            KittyKeyName::F15 => ("57378", SequenceTerminator::Kitty),
            KittyKeyName::F16 => ("57379", SequenceTerminator::Kitty),
            KittyKeyName::F17 => ("57380", SequenceTerminator::Kitty),
            KittyKeyName::F18 => ("57381", SequenceTerminator::Kitty),
            KittyKeyName::F19 => ("57382", SequenceTerminator::Kitty),
            KittyKeyName::F20 => ("57383", SequenceTerminator::Kitty),
            KittyKeyName::F21 => ("57384", SequenceTerminator::Kitty),
            KittyKeyName::F22 => ("57385", SequenceTerminator::Kitty),
            KittyKeyName::F23 => ("57386", SequenceTerminator::Kitty),
            KittyKeyName::F24 => ("57387", SequenceTerminator::Kitty),
            KittyKeyName::F25 => ("57388", SequenceTerminator::Kitty),
            KittyKeyName::F26 => ("57389", SequenceTerminator::Kitty),
            KittyKeyName::F27 => ("57390", SequenceTerminator::Kitty),
            KittyKeyName::F28 => ("57391", SequenceTerminator::Kitty),
            KittyKeyName::F29 => ("57392", SequenceTerminator::Kitty),
            KittyKeyName::F30 => ("57393", SequenceTerminator::Kitty),
            KittyKeyName::F31 => ("57394", SequenceTerminator::Kitty),
            KittyKeyName::F32 => ("57395", SequenceTerminator::Kitty),
            KittyKeyName::F33 => ("57396", SequenceTerminator::Kitty),
            KittyKeyName::F34 => ("57397", SequenceTerminator::Kitty),
            KittyKeyName::F35 => ("57398", SequenceTerminator::Kitty),
            KittyKeyName::ScrollLock => ("57359", SequenceTerminator::Kitty),
            KittyKeyName::PrintScreen => ("57361", SequenceTerminator::Kitty),
            KittyKeyName::Pause => ("57362", SequenceTerminator::Kitty),
            KittyKeyName::ContextMenu => ("57363", SequenceTerminator::Kitty),
            KittyKeyName::MediaPlay => ("57428", SequenceTerminator::Kitty),
            KittyKeyName::MediaPause => ("57429", SequenceTerminator::Kitty),
            KittyKeyName::MediaPlayPause => ("57430", SequenceTerminator::Kitty),
            KittyKeyName::MediaStop => ("57432", SequenceTerminator::Kitty),
            KittyKeyName::MediaFastForward => ("57433", SequenceTerminator::Kitty),
            KittyKeyName::MediaRewind => ("57434", SequenceTerminator::Kitty),
            KittyKeyName::MediaTrackNext => ("57435", SequenceTerminator::Kitty),
            KittyKeyName::MediaTrackPrevious => ("57436", SequenceTerminator::Kitty),
            KittyKeyName::MediaRecord => ("57437", SequenceTerminator::Kitty),
            KittyKeyName::AudioVolumeDown => ("57438", SequenceTerminator::Kitty),
            KittyKeyName::AudioVolumeUp => ("57439", SequenceTerminator::Kitty),
            KittyKeyName::AudioVolumeMute => ("57440", SequenceTerminator::Kitty),
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), terminator))
    }

    /// Try building from a [`KittyKeyName`].
    fn try_build_named_normal(
        &self,
        key: &KittyKeyEvent,
        has_associated_text: bool,
    ) -> Option<SequenceBase> {
        let named = match &key.logical_key {
            KittyLogicalKey::Named(named) => *named,
            _ => return None,
        };

        // The default parameter is 1, so we can omit it.
        let one_based = if self.modifiers.is_empty()
            && !self.input_mode.kitty_event_type
            && !has_associated_text
        {
            ""
        } else {
            "1"
        };
        let (base, terminator) = match named {
            KittyKeyName::PageUp => ("5", SequenceTerminator::Normal('~')),
            KittyKeyName::PageDown => ("6", SequenceTerminator::Normal('~')),
            KittyKeyName::Insert => ("2", SequenceTerminator::Normal('~')),
            KittyKeyName::Delete => ("3", SequenceTerminator::Normal('~')),
            KittyKeyName::Home => (one_based, SequenceTerminator::Normal('H')),
            KittyKeyName::End => (one_based, SequenceTerminator::Normal('F')),
            KittyKeyName::ArrowLeft => (one_based, SequenceTerminator::Normal('D')),
            KittyKeyName::ArrowRight => (one_based, SequenceTerminator::Normal('C')),
            KittyKeyName::ArrowUp => (one_based, SequenceTerminator::Normal('A')),
            KittyKeyName::ArrowDown => (one_based, SequenceTerminator::Normal('B')),
            KittyKeyName::F1 => (one_based, SequenceTerminator::Normal('P')),
            KittyKeyName::F2 => (one_based, SequenceTerminator::Normal('Q')),
            KittyKeyName::F3 => (one_based, SequenceTerminator::Normal('R')),
            KittyKeyName::F4 => (one_based, SequenceTerminator::Normal('S')),
            KittyKeyName::F5 => ("15", SequenceTerminator::Normal('~')),
            KittyKeyName::F6 => ("17", SequenceTerminator::Normal('~')),
            KittyKeyName::F7 => ("18", SequenceTerminator::Normal('~')),
            KittyKeyName::F8 => ("19", SequenceTerminator::Normal('~')),
            KittyKeyName::F9 => ("20", SequenceTerminator::Normal('~')),
            KittyKeyName::F10 => ("21", SequenceTerminator::Normal('~')),
            KittyKeyName::F11 => ("23", SequenceTerminator::Normal('~')),
            KittyKeyName::F12 => ("24", SequenceTerminator::Normal('~')),
            KittyKeyName::F13 => ("25", SequenceTerminator::Normal('~')),
            KittyKeyName::F14 => ("26", SequenceTerminator::Normal('~')),
            KittyKeyName::F15 => ("28", SequenceTerminator::Normal('~')),
            KittyKeyName::F16 => ("29", SequenceTerminator::Normal('~')),
            KittyKeyName::F17 => ("31", SequenceTerminator::Normal('~')),
            KittyKeyName::F18 => ("32", SequenceTerminator::Normal('~')),
            KittyKeyName::F19 => ("33", SequenceTerminator::Normal('~')),
            KittyKeyName::F20 => ("34", SequenceTerminator::Normal('~')),
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), terminator))
    }

    /// Try building escape from control characters (e.g. Enter) and modifiers.
    fn try_build_control_char_or_mod(
        &self,
        key: &KittyKeyEvent,
        mods: &mut SequenceModifiers,
    ) -> Option<SequenceBase> {
        if !self.input_mode.kitty_encode_all && !self.input_mode.kitty_sequence {
            return None;
        }

        let named = match &key.logical_key {
            KittyLogicalKey::Named(named) => *named,
            _ => return None,
        };

        let base = match named {
            KittyKeyName::Tab => "9",
            KittyKeyName::Enter => "13",
            KittyKeyName::Escape => "27",
            KittyKeyName::Space => "32",
            KittyKeyName::Backspace => "127",
            _ => "",
        };

        // Fail when the key is not a named control character and the active mode prohibits us
        // from encoding modifier keys.
        if !self.input_mode.kitty_encode_all && base.is_empty() {
            return None;
        }

        let numpad = key.numpad_location;
        let base = match (named, numpad) {
            (KittyKeyName::Shift, false) => "57441",
            (KittyKeyName::Control, false) => "57442",
            (KittyKeyName::Alt, false) => "57443",
            (KittyKeyName::Super, false) => "57444",
            (KittyKeyName::Hyper, false) => "57445",
            (KittyKeyName::Meta, false) => "57446",
            (KittyKeyName::Shift, _) => "57447",
            (KittyKeyName::Control, _) => "57448",
            (KittyKeyName::Alt, _) => "57449",
            (KittyKeyName::Super, _) => "57450",
            (KittyKeyName::Hyper, _) => "57451",
            (KittyKeyName::Meta, _) => "57452",
            (KittyKeyName::CapsLock, _) => "57358",
            (KittyKeyName::NumLock, _) => "57360",
            _ => base,
        };

        // NOTE: Kitty's protocol mandates that the modifier state is applied before
        // key press, however winit sends them after the key press, so for modifiers
        // itself apply the state based on keysyms and not the _actual_ modifiers
        // state, which is how kitty is doing so and what is suggested in such case.
        let press = key.state.is_pressed();
        match named {
            KittyKeyName::Shift => mods.set(SequenceModifiers::SHIFT, press),
            KittyKeyName::Control => mods.set(SequenceModifiers::CONTROL, press),
            KittyKeyName::Alt => mods.set(SequenceModifiers::ALT, press),
            KittyKeyName::Super => mods.set(SequenceModifiers::SUPER, press),
            _ => (),
        }

        if base.is_empty() {
            None
        } else {
            Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
        }
    }
}

pub struct SequenceBase {
    /// The base of the payload, which is the `number` and optionally an alt base from the kitty
    /// spec.
    pub payload: Cow<'static, str>,
    pub terminator: SequenceTerminator,
}

impl SequenceBase {
    pub fn new(payload: Cow<'static, str>, terminator: SequenceTerminator) -> Self {
        Self {
            payload,
            terminator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceTerminator {
    /// The normal key esc sequence terminator defined by xterm/dec.
    Normal(char),
    /// The terminator is for kitty escape sequence.
    Kitty,
}

impl SequenceTerminator {
    pub fn encode_esc_sequence(self) -> char {
        match self {
            SequenceTerminator::Normal(char) => char,
            SequenceTerminator::Kitty => 'u',
        }
    }
}

bitflags::bitflags! {
    /// The modifiers encoding for escape sequence.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SequenceModifiers : u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        // NOTE: Kitty protocol defines additional modifiers to what is present here, like
        // Capslock, but it's not a modifier as per winit.
    }
}

impl SequenceModifiers {
    /// Get the value which should be passed to escape sequence.
    pub fn encode_esc_sequence(self) -> u8 {
        self.bits() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_event(text: &str) -> KittyKeyEvent {
        KittyKeyEvent {
            logical_key: KittyLogicalKey::Character {
                text: text.to_string(),
                base: None,
            },
            state: KittyKeyState::Pressed,
            repeat: false,
            numpad_location: false,
            text_with_all_modifiers: Some(text.to_string()),
        }
    }

    fn named_event(name: KittyKeyName) -> KittyKeyEvent {
        KittyKeyEvent {
            logical_key: KittyLogicalKey::Named(name),
            state: KittyKeyState::Pressed,
            repeat: false,
            numpad_location: false,
            text_with_all_modifiers: None,
        }
    }

    #[test]
    fn empty_when_no_match() {
        // No mode flags set → most events emit nothing.
        let event = char_event("a");
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            false,
            false,
            false,
            false,
        );
        // Without kitty modes, char `a` has no normal-named/control branch
        // to fall into → empty.
        assert!(out.is_empty());
    }

    #[test]
    fn kitty_sequence_emits_for_char_key() {
        let event = char_event("a");
        // Set disambiguate_esc_codes → kitty_sequence on.
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            true,
            false,
            false,
            false,
        );
        assert_eq!(out, b"\x1b[97u");
    }

    #[test]
    fn arrow_key_normal_encoding() {
        let event = named_event(KittyKeyName::ArrowUp);
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            false,
            false,
            false,
            false,
        );
        // No modifiers + no kitty mode → bare ESC[A.
        assert_eq!(out, b"\x1b[A");
    }

    #[test]
    fn arrow_key_with_modifiers_uses_one_base() {
        let event = named_event(KittyKeyName::ArrowUp);
        let out = build(
            &event,
            SequenceModifiers::SHIFT,
            false,
            false,
            false,
            false,
            false,
        );
        // Shift = bit 0 → encode value = 2. Sequence: ESC[1;2A.
        assert_eq!(out, b"\x1b[1;2A");
    }

    #[test]
    fn numpad_digit_uses_kitty_code() {
        let mut event = char_event("5");
        event.numpad_location = true;
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            true,
            false,
            false,
            false,
        );
        assert_eq!(out, b"\x1b[57404u");
    }

    #[test]
    fn control_char_enter_in_kitty_mode() {
        let event = named_event(KittyKeyName::Enter);
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            true,
            false,
            false,
            false,
        );
        // Enter has both normal-named ('1', terminator depends) and control-char ('13')
        // paths; the normal-named branch matches first only for keys it lists — Enter
        // isn't in named_normal, so it falls through to control-char-or-mod and emits
        // ESC[13u.
        assert_eq!(out, b"\x1b[13u");
    }

    #[test]
    fn event_type_release_when_report_event_types_on() {
        let mut event = named_event(KittyKeyName::ArrowLeft);
        event.state = KittyKeyState::Released;
        // report_event_types + key_released → kitty_event_type true.
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            false,
            true,
            false,
            false,
        );
        // No mods set so modifiers field is `;1`; event type `:3`; terminator from
        // kitty_sequence path... wait, ArrowLeft falls into named_normal which uses
        // Terminator::Normal('D'). kitty_event_type forces the `;1:3` insertion before
        // the terminator. Expected: ESC[1;1:3D.
        assert_eq!(out, b"\x1b[1;1:3D");
    }

    #[test]
    fn associated_text_appended_when_enabled() {
        let mut event = char_event("é");
        event.text_with_all_modifiers = Some("é".to_string());
        // disambiguate_esc_codes + report_associated_text.
        let out = build(
            &event,
            SequenceModifiers::empty(),
            false,
            true,
            false,
            false,
            true,
        );
        // Char `é` = U+00E9 = 233.
        // Sequence: ESC[233;1;233u — modifier field forced because associated text present.
        assert_eq!(out, b"\x1b[233;1;233u");
    }

    #[test]
    fn modifier_key_press_updates_modifier_bits() {
        let event = named_event(KittyKeyName::Shift);
        // kitty_encode_all on.
        let out = build(
            &event,
            SequenceModifiers::empty(),
            true,
            false,
            false,
            false,
            false,
        );
        // Shift press: encode bit becomes shift, modifier field = ";2"; terminator 'u';
        // base 57441 (left shift).
        assert_eq!(out, b"\x1b[57441;2u");
    }

    #[test]
    fn sequence_modifiers_encode_esc_sequence_is_plus_one() {
        assert_eq!(SequenceModifiers::empty().encode_esc_sequence(), 1);
        assert_eq!(SequenceModifiers::SHIFT.encode_esc_sequence(), 2);
        assert_eq!(
            (SequenceModifiers::SHIFT | SequenceModifiers::CONTROL).encode_esc_sequence(),
            6
        );
    }

    #[test]
    fn terminator_encodes_correctly() {
        assert_eq!(SequenceTerminator::Kitty.encode_esc_sequence(), 'u');
        assert_eq!(SequenceTerminator::Normal('D').encode_esc_sequence(), 'D');
    }
}
