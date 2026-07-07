use serde::{Deserialize, Serialize};

/// RGBA. reMarkable 2 ink renders grayscale on-device, but we keep full
/// colour so Neoism-side ink (and a colour Paper Pro later) round-trips
/// losslessly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const BLACK: Color = Color([0, 0, 0, 255]);
}

/// One sampled pen point in the shared page frame (see [`crate::PAGE_WIDTH`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    /// Pen pressure in `0.0..=1.0`; sources without pressure use `1.0`.
    #[serde(default = "one")]
    pub pressure: f32,
}

fn one() -> f32 {
    1.0
}

/// A single pen stroke in page coordinates.
///
/// A stroke is **atomic**: you add a whole stroke or erase a whole
/// stroke, you never merge two halves of one. That makes it a perfect
/// fit for a CRDT list element — concurrent drawing on two devices just
/// unions the sets of strokes with no conflict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Stable id so erase/dedup references the same stroke across peers.
    pub id: u64,
    pub points: Vec<StrokePoint>,
    pub width: f32,
    pub color: Color,
    /// The text position this stroke was drawn beside, encoded as a Loro
    /// cursor so it rides along as the markdown reflows (the golden
    /// standard for annotation anchoring). `None` = pinned to absolute
    /// page coordinates instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Vec<u8>>,
    /// Which reMarkable page this stroke came from, for multi-page
    /// notebooks stacked into one note. `None` = authored in Neoism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

impl Stroke {
    /// A new stroke pinned to page coordinates (no text anchor yet).
    pub fn new(id: u64, points: Vec<StrokePoint>, width: f32, color: Color) -> Self {
        Self {
            id,
            points,
            width,
            color,
            anchor: None,
            page: None,
        }
    }

    /// Axis-aligned bounds `(min_x, min_y, max_x, max_y)`, or `None` when
    /// the stroke has no points.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut it = self.points.iter();
        let first = it.next()?;
        let (mut minx, mut miny, mut maxx, mut maxy) =
            (first.x, first.y, first.x, first.y);
        for p in it {
            minx = minx.min(p.x);
            miny = miny.min(p.y);
            maxx = maxx.max(p.x);
            maxy = maxy.max(p.y);
        }
        Some((minx, miny, maxx, maxy))
    }
}
