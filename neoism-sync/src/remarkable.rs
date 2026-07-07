//! reMarkable `.rm` ("lines") codec: device handwriting ⇄ [`Stroke`].
//!
//! reMarkable stores each notebook page as a `.rm` file of pen strokes.
//! Decoding it is what makes "see my handwriting in Neoism" possible; the
//! strokes drop straight into a [`NoteDoc`](crate::NoteDoc)'s ink layer.
//!
//! There are several format versions. **v3/v5** are a fixed little-endian
//! layout (implemented + round-trip tested here). **v6** (reMarkable
//! software 3.0+) is a tagged, block-structured format — we *detect* it
//! but finishing the parser blind is untrustworthy, so the bridge will
//! pull a real sample off the device to nail it. We also confirm the
//! device's firmware up front so we pick the right decoder.

use crate::stroke::{Color, Stroke, StrokePoint};

/// The on-disk `.rm` format version, read from the 43-byte ASCII header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmVersion {
    V3,
    V5,
    V6,
}

#[derive(Debug, thiserror::Error)]
pub enum RmError {
    #[error("not a reMarkable .rm file (bad/short header)")]
    BadHeader,
    #[error("unexpected end of .rm data at offset {0}")]
    Eof(usize),
}

const HEADER_LEN: usize = 43;
/// Brush id written when we *encode* (a plain ballpoint); irrelevant to
/// decoding, which ignores brush type.
const DEFAULT_BRUSH: i32 = 2;

/// Sniff the format version from the header. `None` if the bytes aren't a
/// `.rm` file at all.
pub fn detect_version(bytes: &[u8]) -> Option<RmVersion> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    let header = std::str::from_utf8(&bytes[..HEADER_LEN]).ok()?;
    if header.starts_with("reMarkable .lines file, version=6") {
        Some(RmVersion::V6)
    } else if header.starts_with("reMarkable .lines file, version=5") {
        Some(RmVersion::V5)
    } else if header.starts_with("reMarkable .lines file, version=3") {
        Some(RmVersion::V3)
    } else if header.starts_with("reMarkable lines with selections") {
        Some(RmVersion::V3)
    } else {
        None
    }
}

/// Decode a `.rm` page into strokes in page coordinates.
pub fn parse_rm(bytes: &[u8]) -> Result<Vec<Stroke>, RmError> {
    match detect_version(bytes).ok_or(RmError::BadHeader)? {
        RmVersion::V3 => parse_v3_v5(bytes, false),
        RmVersion::V5 => parse_v3_v5(bytes, true),
        RmVersion::V6 => Ok(parse_v6(bytes)),
    }
}

/// Encode strokes as a v5 `.rm` page (single layer). Used for round-trip
/// tests and any future "write ink to the device" path.
pub fn encode_rm_v5(strokes: &[Stroke]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut header = b"reMarkable .lines file, version=5".to_vec();
    header.resize(HEADER_LEN, b' ');
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&1i32.to_le_bytes()); // one layer
    buf.extend_from_slice(&(strokes.len() as i32).to_le_bytes());
    for s in strokes {
        buf.extend_from_slice(&DEFAULT_BRUSH.to_le_bytes());
        buf.extend_from_slice(&rgba_to_rm_color(s.color).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes()); // padding
        buf.extend_from_slice(&s.width.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes()); // v5 unknown field
        buf.extend_from_slice(&(s.points.len() as i32).to_le_bytes());
        for p in &s.points {
            buf.extend_from_slice(&p.x.to_le_bytes());
            buf.extend_from_slice(&p.y.to_le_bytes());
            buf.extend_from_slice(&0f32.to_le_bytes()); // speed
            buf.extend_from_slice(&0f32.to_le_bytes()); // direction/tilt
            buf.extend_from_slice(&s.width.to_le_bytes()); // per-point width
            buf.extend_from_slice(&p.pressure.to_le_bytes());
        }
    }
    buf
}

// ---- v3/v5 reader -------------------------------------------------------

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }

    fn skip(&mut self, n: usize) -> Result<(), RmError> {
        let end = self.pos + n;
        if end > self.b.len() {
            return Err(RmError::Eof(self.pos));
        }
        self.pos = end;
        Ok(())
    }

    fn i32(&mut self) -> Result<i32, RmError> {
        let end = self.pos + 4;
        if end > self.b.len() {
            return Err(RmError::Eof(self.pos));
        }
        let v = i32::from_le_bytes(self.b[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }

    fn f32(&mut self) -> Result<f32, RmError> {
        let end = self.pos + 4;
        if end > self.b.len() {
            return Err(RmError::Eof(self.pos));
        }
        let v = f32::from_le_bytes(self.b[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }
}

fn parse_v3_v5(bytes: &[u8], v5: bool) -> Result<Vec<Stroke>, RmError> {
    let mut r = Reader::new(bytes);
    r.skip(HEADER_LEN)?;
    let num_layers = r.i32()?.max(0);
    let mut out = Vec::new();
    for _ in 0..num_layers {
        let num_lines = r.i32()?.max(0);
        for _ in 0..num_lines {
            let _brush_type = r.i32()?;
            let color = r.i32()?;
            let _padding = r.i32()?;
            let base_size = r.f32()?;
            if v5 {
                let _unknown = r.i32()?;
            }
            let num_points = r.i32()?.max(0);
            let mut points = Vec::with_capacity(num_points as usize);
            for _ in 0..num_points {
                let x = r.f32()?;
                let y = r.f32()?;
                let _speed = r.f32()?;
                let _direction = r.f32()?;
                let _width = r.f32()?;
                let pressure = r.f32()?;
                points.push(StrokePoint {
                    x,
                    y,
                    pressure: pressure.clamp(0.0, 1.0),
                });
            }
            let id = stable_id(&points, base_size, color);
            out.push(Stroke {
                id,
                points,
                width: base_size,
                color: rm_color_to_rgba(color),
                anchor: None,
                page: None,
            });
        }
    }
    Ok(out)
}

// ---- helpers ------------------------------------------------------------

/// Content-derived id so re-pulling an unchanged page yields the *same*
/// stroke ids — the CRDT then dedups instead of piling up duplicates.
fn stable_id(points: &[StrokePoint], width: f32, color: i32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for &x in bytes {
            h ^= x as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for p in points {
        mix(&p.x.to_le_bytes());
        mix(&p.y.to_le_bytes());
        mix(&p.pressure.to_le_bytes());
    }
    mix(&width.to_le_bytes());
    mix(&color.to_le_bytes());
    h
}

// ---- v6 encoder (Neoism ink → device strokes) ---------------------------
//
// The inverse of the v6 decoder below: write strokes as `SceneLineItem`
// blocks so handwriting/highlights drawn in Neoism can be pushed back onto
// the tablet. Points must already be in **page coordinates** (the caller
// maps content→page). NOTE: this emits the stroke blocks; the device also
// expects scaffolding blocks (AuthorIds/MigrationInfo/PageInfo/SceneTree) —
// added when validating against a live device. Round-trip tested against
// `parse_v6` here.

fn push_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// Tag byte: `index << 4 | type` (matches the decoder's `tag()`).
fn push_tag(out: &mut Vec<u8>, index: u8, ty: u8) {
    out.push((index << 4) | (ty & 0x0F));
}

/// CrdtId: author byte + varint sequence.
fn push_cid(out: &mut Vec<u8>, author: u8, seq: u64) {
    out.push(author);
    push_varint(out, seq);
}

fn encode_line_value(s: &Stroke) -> Vec<u8> {
    let mut v = Vec::new();
    v.push(0x03); // leading marker byte (as the device writes)
    push_tag(&mut v, 1, 0x4); // tool
    v.extend_from_slice(&17u32.to_le_bytes()); // 17 = Fineliner
    push_tag(&mut v, 2, 0x4); // colour
    v.extend_from_slice(&(rgba_to_rm_color(s.color) as u32).to_le_bytes());
    push_tag(&mut v, 3, 0x8); // thickness_scale (f64)
    v.extend_from_slice(&(s.width as f64).to_le_bytes());
    push_tag(&mut v, 4, 0x4); // starting_length (f32 bits)
    v.extend_from_slice(&0f32.to_le_bytes());

    // points subblock (v2 layout: x f32, y f32, speed u16, width u16, dir u8, pres u8)
    let half_w = crate::PAGE_WIDTH * 0.5;
    let half_h = crate::PAGE_HEIGHT * 0.5;
    let mut pts = Vec::new();
    for p in &s.points {
        pts.extend_from_slice(&(p.x - half_w).to_le_bytes());
        pts.extend_from_slice(&(p.y - half_h).to_le_bytes());
        pts.extend_from_slice(&0u16.to_le_bytes()); // speed
        pts.extend_from_slice(&0u16.to_le_bytes()); // width
        pts.push(0); // direction
        pts.push((p.pressure.clamp(0.0, 1.0) * 255.0) as u8);
    }
    push_tag(&mut v, 5, 0xC); // Length4 subblock
    v.extend_from_slice(&(pts.len() as u32).to_le_bytes());
    v.extend_from_slice(&pts);
    v
}

fn encode_line_block(s: &Stroke, item_seq: u64) -> Vec<u8> {
    let mut b = Vec::new();
    push_tag(&mut b, 1, 0xF);
    push_cid(&mut b, 0, 0); // parent
    push_tag(&mut b, 2, 0xF);
    push_cid(&mut b, 1, item_seq); // item id (unique)
    push_tag(&mut b, 3, 0xF);
    push_cid(&mut b, 0, 0); // left
    push_tag(&mut b, 4, 0xF);
    push_cid(&mut b, 0, 0); // right
    push_tag(&mut b, 5, 0x4);
    b.extend_from_slice(&0u32.to_le_bytes()); // deleted_length
    let value = encode_line_value(s);
    push_tag(&mut b, 6, 0xC); // value subblock
    b.extend_from_slice(&(value.len() as u32).to_le_bytes());
    b.extend_from_slice(&value);
    b
}

/// Encode strokes (in page coordinates) as a v6 `.rm` page.
pub fn encode_rm_v6(strokes: &[Stroke]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut header = b"reMarkable .lines file, version=6".to_vec();
    header.resize(HEADER_LEN, b' ');
    out.extend_from_slice(&header);
    for (i, s) in strokes.iter().enumerate() {
        let block = encode_line_block(s, i as u64 + 1);
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
        out.push(0); // unknown
        out.push(2); // min_version
        out.push(2); // current_version (v2 points)
        out.push(V6_LINE_BLOCK); // 0x05
        out.extend_from_slice(&block);
    }
    out
}

// ---- v6 (block / tagged format, reMarkable software 3.0+) ---------------
//
// A v6 file is a flat stream of blocks after the 43-byte header. Each block:
//   len: u32 | unknown: u8 | min_ver: u8 | current_ver: u8 | type: u8 | body[len]
// Strokes live in `SceneLineItem` blocks (type 0x05). Inside a block, fields
// are tag-prefixed: a varint tag where `index = tag >> 4`, `type = tag & 0xF`
// (ID=0xF as u8+varint, Byte4=0x4, Byte8=0x8 double, Byte1=0x1, Length4=0xC
// length-prefixed subblock). The Line value is subblock index 6; its points
// are subblock index 5. Point coords are page-centered. All verified against
// a real firmware-3.x notebook.

const V6_LINE_BLOCK: u8 = 0x05;

struct V6<'a> {
    d: &'a [u8],
    p: usize,
    end: usize,
}

impl<'a> V6<'a> {
    fn new(d: &'a [u8], p: usize, end: usize) -> Self {
        Self { d, p, end }
    }
    fn left(&self) -> usize {
        self.end.saturating_sub(self.p)
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.d.get(self.p).filter(|_| self.p < self.end)?;
        self.p += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        if self.p + 2 > self.end {
            return None;
        }
        let v = u16::from_le_bytes(self.d[self.p..self.p + 2].try_into().unwrap());
        self.p += 2;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        if self.p + 4 > self.end {
            return None;
        }
        let v = u32::from_le_bytes(self.d[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        Some(v)
    }
    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    fn f64(&mut self) -> Option<f64> {
        if self.p + 8 > self.end {
            return None;
        }
        let v = f64::from_le_bytes(self.d[self.p..self.p + 8].try_into().unwrap());
        self.p += 8;
        Some(v)
    }
    fn varint(&mut self) -> Option<u64> {
        let mut r = 0u64;
        let mut shift = 0;
        loop {
            let x = self.u8()?;
            r |= ((x & 0x7f) as u64) << shift;
            if x & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                break;
            }
        }
        Some(r)
    }
    /// `(index, type)` from a tag varint.
    fn tag(&mut self) -> Option<(u8, u8)> {
        let t = self.varint()?;
        Some(((t >> 4) as u8, (t & 0xF) as u8))
    }
    /// A CrdtId: author byte + varint sequence.
    fn cid(&mut self) -> Option<(u8, u64)> {
        Some((self.u8()?, self.varint()?))
    }
}

fn parse_v6(bytes: &[u8]) -> Vec<Stroke> {
    let mut strokes = Vec::new();
    let mut p = HEADER_LEN;
    while p + 8 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        let current_ver = bytes[p + 6];
        let block_type = bytes[p + 7];
        let body = p + 8;
        let end = match body.checked_add(len) {
            Some(e) if e <= bytes.len() => e,
            _ => break, // truncated; salvage what we have
        };
        if block_type == V6_LINE_BLOCK {
            if let Some(stroke) = parse_line_block(bytes, body, end, current_ver) {
                strokes.push(stroke);
            }
        }
        p = end;
    }
    strokes
}

/// SceneLineItem: base tagged fields, then the Line value at subblock 6.
fn parse_line_block(
    d: &[u8],
    body: usize,
    end: usize,
    current_ver: u8,
) -> Option<Stroke> {
    let mut r = V6::new(d, body, end);
    let mut item_id = (0u8, 0u64);
    while r.p < end {
        let (idx, ty) = r.tag()?;
        match ty {
            0xF => {
                let id = r.cid()?;
                if idx == 2 {
                    item_id = id; // stable per-stroke id
                }
            }
            0x4 => {
                r.u32()?;
            }
            0x8 => {
                r.f64()?;
            }
            0x1 => {
                r.u8()?;
            }
            0xC => {
                let len = r.u32()? as usize;
                let sub = r.p;
                let sube = (sub + len).min(end);
                r.p = sube;
                if idx == 6 {
                    return parse_line_value(d, sub, sube, current_ver, item_id);
                }
            }
            _ => return None, // unexpected tag in base — skip this line
        }
    }
    None // empty/deleted line (no value)
}

/// The Line value: tool(1)/color(2)/thickness(3)/start(4)/points(5)…
fn parse_line_value(
    d: &[u8],
    start: usize,
    end: usize,
    current_ver: u8,
    item_id: (u8, u64),
) -> Option<Stroke> {
    let mut r = V6::new(d, start, end);
    let mut color = 0u32;
    let mut thickness = 1.0f64;
    let mut points: Option<(usize, usize)> = None;
    while r.p < end {
        let (idx, ty) = r.tag()?;
        match ty {
            0x4 => {
                let v = r.u32()?;
                if idx == 2 {
                    color = v;
                }
            }
            0x8 => {
                let v = r.f64()?;
                if idx == 3 {
                    thickness = v;
                }
            }
            0xF => {
                r.cid()?;
            }
            0x1 => {
                r.u8()?;
            }
            0xC => {
                let len = r.u32()? as usize;
                let s = r.p;
                let e = (s + len).min(end);
                r.p = e;
                if idx == 5 {
                    points = Some((s, e));
                }
            }
            // A lone leading marker byte (index 0) uses a non-standard tag
            // type; it carries no value, so just continue past it.
            _ => {}
        }
    }
    let (ps, pe) = points?;
    let pts = decode_v6_points(d, ps, pe, current_ver);
    if pts.is_empty() {
        return None;
    }
    let id = ((item_id.0 as u64) << 56) ^ item_id.1;
    Some(Stroke {
        id,
        points: pts,
        width: thickness as f32,
        color: rm_color_to_rgba(color as i32),
        anchor: None,
        page: None,
    })
}

fn decode_v6_points(
    d: &[u8],
    start: usize,
    end: usize,
    current_ver: u8,
) -> Vec<StrokePoint> {
    let half_w = crate::PAGE_WIDTH * 0.5;
    let half_h = crate::PAGE_HEIGHT * 0.5;
    let mut out = Vec::new();
    let mut r = V6::new(d, start, end);
    if current_ver >= 2 {
        // v2: x f32, y f32, speed u16, width u16, direction u8, pressure u8
        while r.left() >= 14 {
            let x = r.f32().unwrap();
            let y = r.f32().unwrap();
            let _speed = r.u16().unwrap();
            let _width = r.u16().unwrap();
            let _dir = r.u8().unwrap();
            let pressure = r.u8().unwrap();
            out.push(StrokePoint {
                x: x + half_w,
                y: y + half_h,
                pressure: pressure as f32 / 255.0,
            });
        }
    } else {
        // v1: x, y, speed, direction, width, pressure — all f32
        while r.left() >= 24 {
            let x = r.f32().unwrap();
            let y = r.f32().unwrap();
            let _speed = r.f32().unwrap();
            let _dir = r.f32().unwrap();
            let _width = r.f32().unwrap();
            let pressure = r.f32().unwrap();
            out.push(StrokePoint {
                x: x + half_w,
                y: y + half_h,
                pressure: pressure.clamp(0.0, 1.0),
            });
        }
    }
    out
}

fn rm_color_to_rgba(code: i32) -> Color {
    match code {
        1 => Color([128, 128, 128, 255]), // grey
        2 => Color([255, 255, 255, 255]), // white
        3 => Color([0, 0, 255, 255]),     // blue
        4 => Color([255, 0, 0, 255]),     // red
        5 => Color([0, 150, 0, 255]),     // green
        6 => Color([255, 255, 0, 255]),   // yellow
        7 => Color([255, 105, 180, 255]), // pink (highlighter)
        _ => Color([0, 0, 0, 255]),       // black (0) / unknown
    }
}

fn rgba_to_rm_color(c: Color) -> i32 {
    let [r, g, b, _] = c.0;
    let lum = (r as u32 + g as u32 + b as u32) / 3;
    if lum > 200 {
        2
    } else if lum > 80 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rm_v5_roundtrips() {
        let strokes = vec![Stroke::new(
            0,
            vec![
                StrokePoint {
                    x: 1.0,
                    y: 2.0,
                    pressure: 0.5,
                },
                StrokePoint {
                    x: 3.0,
                    y: 4.0,
                    pressure: 0.8,
                },
            ],
            2.5,
            Color::BLACK,
        )];
        let bytes = encode_rm_v5(&strokes);
        assert_eq!(detect_version(&bytes), Some(RmVersion::V5));

        let parsed = parse_rm(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].points.len(), 2);
        assert!((parsed[0].points[0].x - 1.0).abs() < 1e-6);
        assert!((parsed[0].points[1].pressure - 0.8).abs() < 1e-6);
        assert_eq!(parsed[0].color, Color::BLACK);
        assert!((parsed[0].width - 2.5).abs() < 1e-6);
    }

    #[test]
    fn pull_is_idempotent_via_stable_ids() {
        let strokes = vec![Stroke::new(
            0,
            vec![StrokePoint {
                x: 7.0,
                y: 9.0,
                pressure: 1.0,
            }],
            1.0,
            Color::BLACK,
        )];
        let bytes = encode_rm_v5(&strokes);
        let a = parse_rm(&bytes).unwrap();
        let b = parse_rm(&bytes).unwrap();
        assert_eq!(a[0].id, b[0].id, "same page must yield same stroke id");
    }

    #[test]
    fn v6_header_parses_empty_when_no_blocks() {
        let mut h = b"reMarkable .lines file, version=6".to_vec();
        h.resize(HEADER_LEN, b' ');
        assert_eq!(detect_version(&h), Some(RmVersion::V6));
        assert_eq!(parse_rm(&h).unwrap().len(), 0);
    }

    #[test]
    fn v6_encode_roundtrips_through_decoder() {
        // Neoism ink → v6 .rm → our decoder → same strokes (page coords).
        let strokes = vec![Stroke::new(
            0,
            vec![
                StrokePoint {
                    x: 709.0,
                    y: 763.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 720.0,
                    y: 770.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 730.0,
                    y: 780.0,
                    pressure: 1.0,
                },
            ],
            2.0,
            Color::BLACK,
        )];
        let bytes = encode_rm_v6(&strokes);
        assert_eq!(detect_version(&bytes), Some(RmVersion::V6));
        let parsed = parse_rm(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].points.len(), 3);
        assert!((parsed[0].points[0].x - 709.0).abs() < 1.0);
        assert!((parsed[0].points[2].y - 780.0).abs() < 1.0);
        assert!((parsed[0].width - 2.0).abs() < 1e-3);
        assert_eq!(parsed[0].color, Color::BLACK);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            detect_version(b"not a remarkable file at all!!!!!!!!!!!!!!"),
            None
        );
        assert!(matches!(parse_rm(b"too short"), Err(RmError::BadHeader)));
    }
}
