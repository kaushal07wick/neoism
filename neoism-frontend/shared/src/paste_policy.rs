//! Terminal paste-payload policy.
//!
//! Pure functions that decide what bytes the PTY should receive for a given
//! paste, given the terminal's BRACKETED_PASTE state and whether the caller
//! asked for bracketed framing. Desktop and web share this so any policy bug
//! (e.g. failing to strip the bracketed-paste end sentinel) is fixed once.

/// What a `paste` call should send to the PTY.
///
/// `Bracketed` framing wraps the payload in `\x1b[200~` / `\x1b[201~` and
/// scrubs the payload of `\x1b` and `\x03` so a malicious paste can't terminate
/// bracketed mode early.
///
/// `Raw` is what the terminal receives outside bracketed mode: CRLF/LF are
/// collapsed to `\r` when the caller asked for bracketed (because they
/// otherwise expect Enter-like semantics from a paste), but passed through
/// unchanged when the caller did NOT ask for bracketed (paste-as-keystrokes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PastePayload {
    Bracketed { filtered: Vec<u8> },
    Raw(Vec<u8>),
}

/// Bracketed-paste start sentinel.
pub const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";

/// Bracketed-paste end sentinel.
pub const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

/// Scrub a bracketed-paste payload of bytes that would let the inner content
/// terminate the paste early.
///
/// Removes `\x1b` (any ESC that could spell out `\x1b[201~`) and `\x03` (which
/// some shells incorrectly treat as a bracketed-paste terminator).
#[inline]
pub fn filter_bracketed_paste_bytes(text: &str) -> Vec<u8> {
    text.replace(['\x1b', '\x03'], "").into_bytes()
}

/// Convert CRLF and LF to plain CR for unbracketed paste-as-keystrokes mode.
///
/// Outside bracketed mode the terminal can't tell keystrokes from pasted
/// bytes, so we mirror what the Enter key actually produces (`\r`) — line
/// endings would otherwise be interpreted as extra characters by `cat`-style
/// readers, or worse, executed by a shell.
#[inline]
pub fn paste_normalize_line_endings(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
}

/// Decide the payload the PTY should receive.
///
/// `bracketed` is what the caller requested. `bracketed_mode_active` is the
/// terminal-level state (the `BRACKETED_PASTE` mode bit). Only when both are
/// true do we frame the payload; otherwise we hand back raw bytes (with the
/// CRLF normalization quirk noted above).
pub fn paste_payload(
    text: &str,
    bracketed: bool,
    bracketed_mode_active: bool,
) -> PastePayload {
    if bracketed && bracketed_mode_active {
        PastePayload::Bracketed {
            filtered: filter_bracketed_paste_bytes(text),
        }
    } else if bracketed {
        // Caller asked for bracketed framing but the terminal isn't in
        // bracketed mode, so the payload will look like raw keystrokes
        // and the shell will see Enter-like line breaks.
        PastePayload::Raw(paste_normalize_line_endings(text))
    } else {
        PastePayload::Raw(text.as_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_bracketed_paste_strips_escape_and_eot() {
        let scrubbed = filter_bracketed_paste_bytes("ab\x1bcd\x03ef");
        assert_eq!(scrubbed, b"abcdef");
    }

    #[test]
    fn paste_normalize_line_endings_collapses_crlf_and_lf_to_cr() {
        let out = paste_normalize_line_endings("a\r\nb\nc");
        assert_eq!(out, b"a\rb\rc");
    }

    #[test]
    fn paste_payload_uses_bracketed_only_when_both_requested_and_active() {
        match paste_payload("hi\x1b", true, true) {
            PastePayload::Bracketed { filtered } => assert_eq!(filtered, b"hi"),
            other => panic!("expected bracketed, got {other:?}"),
        }
    }

    #[test]
    fn paste_payload_falls_back_to_raw_when_terminal_not_in_bracketed_mode() {
        match paste_payload("a\r\nb", true, false) {
            PastePayload::Raw(bytes) => assert_eq!(bytes, b"a\rb"),
            other => panic!("expected raw, got {other:?}"),
        }
    }

    #[test]
    fn paste_payload_preserves_bytes_verbatim_when_caller_disables_bracketed() {
        match paste_payload("a\r\nb", false, true) {
            PastePayload::Raw(bytes) => assert_eq!(bytes, b"a\r\nb"),
            other => panic!("expected raw, got {other:?}"),
        }
    }
}
