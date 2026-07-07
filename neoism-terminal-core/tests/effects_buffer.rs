//! Core-only test for the terminal effect buffer.
//!
//! Feeds a `\x07` (BEL) byte into a copa parser whose `Perform`
//! implementation pushes `TerminalEffect::Bell` into a local buffer,
//! then asserts a drain returns exactly `[Bell]`. This verifies the
//! terminal-core effect plumbing without depending on the full
//! `Crosswords` terminal state in `neoism-backend`.

use copa::{Parser, Perform};
use neoism_terminal_core::TerminalEffect;

#[derive(Default)]
struct BellPerformer {
    effects: Vec<TerminalEffect>,
}

impl Perform for BellPerformer {
    fn execute(&mut self, byte: u8) {
        if byte == 0x07 {
            self.effects.push(TerminalEffect::Bell);
        }
    }
}

#[test]
fn bell_byte_emits_bell_effect() {
    let mut parser = Parser::new();
    let mut perf = BellPerformer::default();
    parser.advance(&mut perf, &[0x07]);

    let drained: Vec<_> = perf.effects.drain(..).collect();
    assert_eq!(drained.len(), 1);
    assert!(matches!(drained[0], TerminalEffect::Bell));
}

#[test]
fn drain_is_empty_when_no_bell() {
    let mut parser = Parser::new();
    let mut perf = BellPerformer::default();
    parser.advance(&mut perf, b"hello");

    let drained: Vec<_> = perf.effects.drain(..).collect();
    assert!(drained.is_empty());
}
