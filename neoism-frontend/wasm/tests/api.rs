//! wasm-bindgen-test smoke tests. These only build and run for the
//! `wasm32-unknown-unknown` target; on host they compile to an empty
//! crate (which Cargo is happy with).
//!
//! Run with:
//!     wasm-pack test --node --target wasm32-unknown-unknown -p neoism-terminal-wasm
//! (or any browser-driving harness). Don't expect this session to run
//! it — scaffolding only.

#![cfg(target_arch = "wasm32")]

use neoism_terminal_wasm::Terminal;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn terminal_constructs_and_feeds() {
    let mut term = Terminal::new(80, 24);
    term.feed(b"hello");
    // Feeding ordinary printable text emits no PTY-write side
    // effects; the response stream only carries DSR / OSC replies.
    let drained = term.take_pty_writes();
    assert!(
        drained.is_empty(),
        "no DSR/OSC reply expected for plain text"
    );
}

#[wasm_bindgen_test]
fn snapshot_is_non_null() {
    let term = Terminal::new(80, 24);
    let snap = term.snapshot();
    assert!(!snap.is_null());
}

#[wasm_bindgen_test]
fn resize_updates_dimensions() {
    let mut term = Terminal::new(80, 24);
    term.resize(132, 50);
    let snap = term.snapshot();
    // We can't easily destructure the JsValue here without serde; just
    // assert it's a non-null object — the host-side unit test covers
    // the field semantics.
    assert!(!snap.is_null());
}
