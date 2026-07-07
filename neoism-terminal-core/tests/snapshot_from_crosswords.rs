//! Crosswords -> TerminalSnapshot wiring.

use neoism_terminal_core::ansi::CursorShape;
use neoism_terminal_core::handler::Processor;
use neoism_terminal_core::{Crosswords, TerminalId};

fn build(rows: usize, cols: usize) -> Crosswords {
    Crosswords::new((rows, cols), CursorShape::Block, TerminalId::new(0), 100)
}

#[test]
fn snapshot_after_simple_feed() {
    let mut term = build(3, 10);
    let mut proc: Processor = Processor::new();
    proc.advance(&mut term, b"hello");

    let snap = term.snapshot();
    assert_eq!(snap.cols, 10);
    assert_eq!(snap.rows, 3);
    assert_eq!(snap.viewport.len(), 3);
    assert_eq!(snap.viewport[0][0].c, 'h');
    assert_eq!(snap.viewport[0][4].c, 'o');
    assert_eq!(snap.cursor.col, 5);
}

#[test]
fn snapshot_title_roundtrip() {
    let mut term = build(1, 10);
    let mut proc: Processor = Processor::new();
    proc.advance(&mut term, b"\x1b]0;hi\x07");
    let snap = term.snapshot();
    assert_eq!(snap.title, "hi");
}

#[test]
fn snapshot_carries_bold_flag() {
    let mut term = build(1, 10);
    let mut proc: Processor = Processor::new();
    proc.advance(&mut term, b"\x1b[1mB\x1b[0m");

    let snap = term.snapshot();
    assert!(
        snap.viewport[0][0]
            .flags
            .contains(neoism_terminal_core::snapshot::CellFlags::BOLD),
        "expected BOLD on cell 0, got flags={:?}",
        snap.viewport[0][0].flags
    );
}

#[test]
fn snapshot_carries_fg_color() {
    let mut term = build(1, 10);
    let mut proc: Processor = Processor::new();
    // SGR 31 = red foreground.
    proc.advance(&mut term, b"\x1b[31mR\x1b[0m");

    let snap = term.snapshot();
    use neoism_terminal_core::snapshot::ColorIndex;
    match snap.viewport[0][0].fg {
        ColorIndex::Named(n) => assert!(n > 0, "expected non-default named color"),
        ColorIndex::Indexed(_) | ColorIndex::Spec { .. } => {} // also acceptable
        ColorIndex::Default => panic!("expected non-default fg, got Default"),
    }
}

#[test]
fn snapshot_carries_theme() {
    let mut term = build(1, 10);
    let mut proc: Processor = Processor::new();
    proc.advance(&mut term, b"x");
    let snap = term.snapshot();
    assert_eq!(snap.theme.palette.len(), 256);
    let bg = snap.theme.default_bg;
    let fg = snap.theme.default_fg;
    assert_ne!((fg.r, fg.g, fg.b), (bg.r, bg.g, bg.b));
}

#[test]
fn snapshot_roundtrips_through_json() {
    let mut term = build(2, 8);
    let mut proc: Processor = Processor::new();
    proc.advance(&mut term, b"abc");

    let snap = term.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let parsed: neoism_terminal_core::snapshot::TerminalSnapshot =
        serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.cols, 8);
    assert_eq!(parsed.viewport[0][0].c, 'a');
}
