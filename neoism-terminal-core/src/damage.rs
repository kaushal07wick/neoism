//! Damage tracking primitives shared between the terminal engine and
//! the renderer.
//!
//! `Crosswords` produces a stream of `TerminalDamage` updates that
//! describe how much of the viewport changed since the last frame.
//! Both this enum and its supporting `LineDamage` value live here so
//! every host (native, wasm, daemon) can speak the same vocabulary
//! without dragging `neoism-window` or `sugarloaf` into the dependency
//! graph.

use std::collections::BTreeSet;

/// Terminal damage information for efficient rendering.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TerminalDamage {
    /// Nothing changed — skip rendering entirely.
    #[default]
    Noop,
    /// The entire terminal needs to be redrawn.
    Full,
    /// Only specific lines need to be redrawn.
    Partial(BTreeSet<LineDamage>),
    /// Only the cursor position has changed.
    CursorOnly,
}

/// Per-line damage record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineDamage {
    /// Line number.
    pub line: usize,
    /// Whether this line is damaged.
    pub damaged: bool,
}

impl LineDamage {
    #[inline]
    pub fn new(line: usize, damaged: bool) -> Self {
        Self { line, damaged }
    }

    #[inline]
    pub fn undamaged(line: usize) -> Self {
        Self {
            line,
            damaged: false,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.damaged = false;
    }

    #[inline]
    pub fn is_damaged(&self) -> bool {
        self.damaged
    }

    #[inline]
    pub fn mark_damaged(&mut self) {
        self.damaged = true;
    }
}
