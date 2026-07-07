//! Snapshot boundary tests.
//!
//! These tests pin the wire contract: every `TerminalSnapshot` must
//! survive a JSON roundtrip unchanged. If a future field can't
//! roundtrip we want a test failure here before consumers depend on
//! it.

use neoism_terminal_core::snapshot::{
    CellFlags, CellSnapshot, ColorIndex, CursorShape, CursorSnapshot, DamageSnapshot,
    HyperlinkSnapshot, ModesSnapshot, MouseReporting, TerminalSnapshot, ThemeSnapshot,
};

fn sample_cursor() -> CursorSnapshot {
    CursorSnapshot {
        col: 1,
        row: 2,
        shape: CursorShape::Beam,
        visible: true,
    }
}

fn sample_modes() -> ModesSnapshot {
    ModesSnapshot {
        alt_screen: false,
        origin: true,
        auto_wrap: true,
        bracketed_paste: true,
        focus_events: false,
        mouse_reporting: MouseReporting::ButtonEvent,
    }
}

fn sample_damage() -> DamageSnapshot {
    DamageSnapshot {
        full: false,
        dirty_rows: vec![0, 2],
    }
}

fn cell(c: char, fg: ColorIndex, bg: ColorIndex, flags: CellFlags) -> CellSnapshot {
    CellSnapshot {
        c,
        fg,
        bg,
        flags,
        underline_color: None,
        hyperlink_id: None,
    }
}

#[test]
fn snapshot_roundtrips_through_json() {
    let snap = TerminalSnapshot {
        cols: 3,
        rows: 2,
        viewport: vec![
            vec![
                cell(
                    'a',
                    ColorIndex::Named(7),
                    ColorIndex::Default,
                    CellFlags::BOLD,
                ),
                cell(
                    'b',
                    ColorIndex::Indexed(42),
                    ColorIndex::Spec { r: 1, g: 2, b: 3 },
                    CellFlags::ITALIC | CellFlags::UNDERLINE,
                ),
                cell(
                    'c',
                    ColorIndex::Default,
                    ColorIndex::Default,
                    CellFlags::empty(),
                ),
            ],
            vec![
                cell(
                    ' ',
                    ColorIndex::Default,
                    ColorIndex::Default,
                    CellFlags::empty(),
                ),
                cell(
                    'x',
                    ColorIndex::Default,
                    ColorIndex::Default,
                    CellFlags::REVERSE,
                ),
                cell(
                    'y',
                    ColorIndex::Default,
                    ColorIndex::Default,
                    CellFlags::empty(),
                ),
            ],
        ],
        display_offset: 4,
        scrollback_size: 1000,
        cursor: sample_cursor(),
        modes: sample_modes(),
        damage: sample_damage(),
        title: "neoism — pane 1".to_string(),
        hyperlinks: vec![HyperlinkSnapshot {
            id: 7,
            uri: "https://example.com/?x=1".to_string(),
            group: "grp-a".to_string(),
        }],
        theme: ThemeSnapshot::default(),
    };

    let json = serde_json::to_string(&snap).expect("serialize");
    let back: TerminalSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(snap, back);
}

#[test]
fn cell_flags_roundtrip() {
    // Every individual flag, then the union, must roundtrip.
    let all = [
        CellFlags::BOLD,
        CellFlags::ITALIC,
        CellFlags::UNDERLINE,
        CellFlags::UNDERCURL,
        CellFlags::STRIKEOUT,
        CellFlags::REVERSE,
        CellFlags::DIM,
        CellFlags::HIDDEN,
        CellFlags::BLINK,
        CellFlags::WIDE_CHAR,
        CellFlags::WIDE_CHAR_SPACER,
        CellFlags::DOUBLE_UNDERLINE,
        CellFlags::DOTTED_UNDERLINE,
        CellFlags::DASHED_UNDERLINE,
        CellFlags::WRAPLINE,
    ];

    for f in &all {
        let json = serde_json::to_string(f).expect("serialize flag");
        let back: CellFlags = serde_json::from_str(&json).expect("deserialize flag");
        assert_eq!(*f, back, "flag {:?} did not roundtrip", f);
    }

    let union = all.iter().copied().fold(CellFlags::empty(), |a, b| a | b);
    let json = serde_json::to_string(&union).expect("serialize union");
    let back: CellFlags = serde_json::from_str(&json).expect("deserialize union");
    assert_eq!(union, back);
}

#[test]
fn wide_char_spacer_pattern_roundtrips() {
    // A wide CJK glyph followed by its spacer cell — the renderer
    // must be able to reconstruct this exact shape from a snapshot.
    let row = vec![
        cell(
            '中',
            ColorIndex::Default,
            ColorIndex::Default,
            CellFlags::WIDE_CHAR | CellFlags::BOLD,
        ),
        cell(
            '\0',
            ColorIndex::Default,
            ColorIndex::Default,
            CellFlags::WIDE_CHAR_SPACER,
        ),
        cell(
            'x',
            ColorIndex::Default,
            ColorIndex::Default,
            CellFlags::empty(),
        ),
    ];

    let snap = TerminalSnapshot {
        cols: 3,
        rows: 1,
        viewport: vec![row],
        display_offset: 0,
        scrollback_size: 0,
        cursor: sample_cursor(),
        modes: sample_modes(),
        damage: DamageSnapshot::default(),
        title: String::new(),
        hyperlinks: Vec::new(),
        theme: ThemeSnapshot::default(),
    };

    let json = serde_json::to_string(&snap).expect("serialize");
    let back: TerminalSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(snap, back);

    // Spot-check the wide pair after roundtrip.
    assert!(back.viewport[0][0].flags.contains(CellFlags::WIDE_CHAR));
    assert!(back.viewport[0][1]
        .flags
        .contains(CellFlags::WIDE_CHAR_SPACER));
    assert_eq!(back.viewport[0][0].c, '中');
    assert_eq!(back.viewport[0][1].c, '\0');
}

#[test]
fn large_viewport_serializes() {
    // 200 x 60 of plain ASCII 'a' cells — a realistic upper bound
    // for the renderer's working set. We don't assert size, only
    // that the roundtrip succeeds.
    let cols: u16 = 200;
    let rows: u16 = 60;
    let template = cell(
        'a',
        ColorIndex::Default,
        ColorIndex::Default,
        CellFlags::empty(),
    );
    let viewport = vec![vec![template; cols as usize]; rows as usize];

    let snap = TerminalSnapshot {
        cols,
        rows,
        viewport,
        display_offset: 0,
        scrollback_size: 0,
        cursor: sample_cursor(),
        modes: sample_modes(),
        damage: DamageSnapshot::default(),
        title: String::new(),
        hyperlinks: Vec::new(),
        theme: ThemeSnapshot::default(),
    };

    let json = serde_json::to_string(&snap).expect("serialize");
    let back: TerminalSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(snap, back);
    assert_eq!(back.viewport.len(), rows as usize);
    assert_eq!(back.viewport[0].len(), cols as usize);
}

#[test]
fn empty_helper_produces_consistent_shape() {
    let snap = TerminalSnapshot::empty(10, 5);
    assert_eq!(snap.viewport.len(), 5);
    assert!(snap.viewport.iter().all(|row| row.len() == 10));

    let json = serde_json::to_string(&snap).expect("serialize");
    let back: TerminalSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(snap, back);
}
