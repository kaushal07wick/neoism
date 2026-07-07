//! Lossless-ish conversion between CRDT [`Stroke`]s and neodraw shapes.
//!
//! Pulled reMarkable handwriting arrives as `Stroke`s; turning them into a
//! neodraw [`Scene`] lets the existing renderer draw them for free (with
//! the hand-drawn look). Going the other way, a neodraw freehand drawing
//! becomes `Stroke`s we can store in the CRDT and push to the device.
//!
//! The one lossy axis is per-point pressure: neodraw freehand has none, so
//! `Stroke → Shape` drops it and `Shape → Stroke` fills `1.0`. Width and
//! colour round-trip exactly.

use neoism_sync::{Color as SyncColor, Stroke, StrokePoint};

use neoism_ui::editor::neodraw::{
    Color as NeoColor, Scene, Shape, ShapeId, ShapeKind, Style, Vec2, SCENE_VERSION,
};

fn neo_color(c: SyncColor) -> NeoColor {
    let [r, g, b, a] = c.0;
    NeoColor::rgba(r, g, b, a)
}

fn sync_color(c: NeoColor) -> SyncColor {
    SyncColor([c.r, c.g, c.b, c.a])
}

/// One CRDT stroke → a neodraw freehand shape (renderable).
pub fn shape_from_stroke(stroke: &Stroke) -> Shape {
    Shape {
        id: ShapeId(stroke.id),
        kind: ShapeKind::Freehand {
            points: stroke.points.iter().map(|p| Vec2::new(p.x, p.y)).collect(),
        },
        style: Style {
            stroke: neo_color(stroke.color),
            width: stroke.width,
            ..Style::default()
        },
    }
}

/// A neodraw freehand shape → one CRDT stroke. `None` for non-freehand
/// shapes (rects, text, …), which the ink layer doesn't carry.
pub fn stroke_from_shape(shape: &Shape) -> Option<Stroke> {
    match &shape.kind {
        ShapeKind::Freehand { points } => Some(Stroke {
            id: shape.id.0,
            points: points
                .iter()
                .map(|v| StrokePoint {
                    x: v.x,
                    y: v.y,
                    pressure: 1.0,
                })
                .collect(),
            width: shape.style.width,
            color: sync_color(shape.style.stroke),
            anchor: None,
            page: None,
        }),
        _ => None,
    }
}

/// A whole ink layer → a renderable neodraw scene.
pub fn scene_from_strokes(strokes: &[Stroke]) -> Scene {
    Scene {
        version: SCENE_VERSION,
        shapes: strokes.iter().map(shape_from_stroke).collect(),
    }
}

/// Every freehand shape in a scene → CRDT strokes (non-freehand ignored).
pub fn strokes_from_scene(scene: &Scene) -> Vec<Stroke> {
    scene.shapes.iter().filter_map(stroke_from_shape).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_shape_roundtrip_keeps_geometry_width_colour() {
        let stroke = Stroke::new(
            42,
            vec![
                StrokePoint {
                    x: 1.0,
                    y: 2.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 9.0,
                    y: 8.0,
                    pressure: 1.0,
                },
            ],
            3.0,
            SyncColor([10, 20, 30, 255]),
        );
        let back = stroke_from_shape(&shape_from_stroke(&stroke)).unwrap();
        assert_eq!(back.id, stroke.id);
        assert_eq!(back.points.len(), 2);
        assert!((back.points[1].x - 9.0).abs() < 1e-6);
        assert!((back.width - 3.0).abs() < 1e-6);
        assert_eq!(back.color, stroke.color);
    }

    #[test]
    fn scene_roundtrip_filters_to_freehand() {
        let strokes = vec![Stroke::new(
            1,
            vec![StrokePoint {
                x: 0.0,
                y: 0.0,
                pressure: 1.0,
            }],
            1.0,
            SyncColor::BLACK,
        )];
        let scene = scene_from_strokes(&strokes);
        assert_eq!(scene.shapes.len(), 1);
        assert_eq!(strokes_from_scene(&scene).len(), 1);
    }
}
