//! Shape-creation gesture: a draft started on pointer-down, sized on
//! drag, and committed on release.
//!
//! The draft holds enough to both *preview* the shape mid-gesture
//! (rendered as a transient [`Shape`]) and *commit* it into the scene.
//! Drag-to-create tools (rect/ellipse/line/arrow) track two points;
//! the pen accumulates a path. Text placement is handled in `text.rs`.

use super::pane::{DrawPane, Tool};
use super::scene::{ArrowHead, Shape, ShapeId, ShapeKind, Style, Vec2};

/// Minimum drag extent (world units) below which a draft is discarded
/// as an accidental click rather than committed as a zero-size shape.
const MIN_EXTENT: f32 = 2.0;

/// An in-progress shape being drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub tool: Tool,
    pub start: Vec2,
    pub current: Vec2,
    /// Accumulated path for the pen tool.
    pub points: Vec<Vec2>,
}

impl Draft {
    /// The shape this draft currently represents, with the given id and
    /// style. `None` if the tool doesn't produce a draw-drag shape.
    fn to_shape(&self, id: ShapeId, mut style: Style) -> Option<Shape> {
        style.seed = seed_for(id);
        // Highlighter: a wide, translucent marker in the chosen colour.
        if self.tool == Tool::Highlighter {
            style.width = style.width.max(2.0) * 8.0;
            style.opacity = 0.5;
            style.roughness = 0.0;
        }
        let kind = match self.tool {
            Tool::Rect => {
                let (x, y, w, h) = rect_xywh(self.start, self.current);
                ShapeKind::Rect {
                    x,
                    y,
                    w,
                    h,
                    corner: 0.0,
                }
            }
            Tool::Ellipse => {
                let (x, y, w, h) = rect_xywh(self.start, self.current);
                ShapeKind::Ellipse { x, y, w, h }
            }
            Tool::Line => ShapeKind::Line {
                points: vec![self.start, self.current],
            },
            Tool::Arrow => ShapeKind::Arrow {
                points: vec![self.start, self.current],
                head: ArrowHead::Triangle,
            },
            Tool::Pen | Tool::Highlighter => {
                if self.points.len() < 2 {
                    return None;
                }
                ShapeKind::Freehand {
                    points: self.points.clone(),
                }
            }
            _ => return None,
        };
        Some(Shape { id, kind, style })
    }

    /// Whether the draft has grown past the accidental-click threshold.
    fn is_committable(&self) -> bool {
        match self.tool {
            Tool::Pen | Tool::Highlighter => self.points.len() >= 2,
            _ => {
                let dx = (self.current.x - self.start.x).abs();
                let dy = (self.current.y - self.start.y).abs();
                dx >= MIN_EXTENT || dy >= MIN_EXTENT
            }
        }
    }
}

fn rect_xywh(a: Vec2, b: Vec2) -> (f32, f32, f32, f32) {
    (
        a.x.min(b.x),
        a.y.min(b.y),
        (a.x - b.x).abs(),
        (a.y - b.y).abs(),
    )
}

/// Deterministic rough-pass seed derived from a shape id.
fn seed_for(id: ShapeId) -> u32 {
    (id.0 as u32)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(0x9E37_79B1)
}

impl DrawPane {
    /// Whether the active tool creates shapes by dragging.
    pub fn is_creation_tool(&self) -> bool {
        matches!(
            self.tool,
            Tool::Rect
                | Tool::Ellipse
                | Tool::Line
                | Tool::Arrow
                | Tool::Pen
                | Tool::Highlighter
        )
    }

    /// Begin a creation gesture at `world` (pointer-down).
    pub fn begin_draft(&mut self, world: Vec2) {
        if !self.is_creation_tool() {
            return;
        }
        self.draft = Some(Draft {
            tool: self.tool,
            start: world,
            current: world,
            points: vec![world],
        });
    }

    /// Update the active draft as the pointer moves.
    pub fn update_draft(&mut self, world: Vec2) {
        if let Some(draft) = self.draft.as_mut() {
            draft.current = world;
            if matches!(draft.tool, Tool::Pen | Tool::Highlighter) {
                // Skip near-duplicate samples to keep paths lean.
                let push = draft
                    .points
                    .last()
                    .map(|p| (p.x - world.x).abs() > 1.0 || (p.y - world.y).abs() > 1.0)
                    .unwrap_or(true);
                if push {
                    draft.points.push(world);
                }
            }
        }
    }

    /// A transient shape for rendering the in-progress draft.
    pub fn draft_preview(&self) -> Option<Shape> {
        let draft = self.draft.as_ref()?;
        draft.to_shape(ShapeId(u64::MAX), self.style_defaults.clone())
    }

    /// Commit the active draft into the scene. Returns the new shape's id,
    /// or `None` if the draft was too small / invalid. The new shape is NOT
    /// auto-selected — drawing then leaves nothing selected, so you can keep
    /// drawing or click a shape to select it deliberately.
    pub fn commit_draft(&mut self) -> Option<ShapeId> {
        let draft = self.draft.take()?;
        if !draft.is_committable() {
            return None;
        }
        let id = self.scene.next_id();
        let shape = draft.to_shape(id, self.style_defaults.clone())?;
        self.checkpoint();
        self.scene.shapes.push(shape);
        self.selection.clear();
        self.dirty = true;
        // Stay on the current tool so repeated shapes can be drawn
        // without re-picking it each time (press `V` to select/move).
        Some(id)
    }

    /// Abandon the active draft (e.g. Escape).
    pub fn cancel_draft(&mut self) {
        self.draft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pane(tool: Tool) -> DrawPane {
        let mut p = DrawPane::new(PathBuf::from("t.neodraw"));
        p.tool = tool;
        p
    }

    #[test]
    fn rect_draft_commits_to_bounds() {
        let mut p = pane(Tool::Rect);
        p.begin_draft(Vec2::new(10.0, 10.0));
        p.update_draft(Vec2::new(60.0, 40.0));
        let id = p.commit_draft().expect("commits");
        assert_eq!(p.scene.shapes.len(), 1);
        assert_eq!(p.scene.shapes[0].id, id);
        assert_eq!(p.scene.shapes[0].bounds().xywh(), [10.0, 10.0, 50.0, 30.0]);
        assert!(p.selection.is_empty(), "new shape is not auto-selected");
        assert_eq!(p.tool, Tool::Rect, "stays on the tool for repeat drawing");
        assert!(p.draft.is_none());
    }

    #[test]
    fn dragging_up_left_normalizes_origin() {
        let mut p = pane(Tool::Ellipse);
        p.begin_draft(Vec2::new(100.0, 100.0));
        p.update_draft(Vec2::new(40.0, 30.0));
        p.commit_draft().unwrap();
        assert_eq!(p.scene.shapes[0].bounds().xywh(), [40.0, 30.0, 60.0, 70.0]);
    }

    #[test]
    fn tiny_drag_is_discarded() {
        let mut p = pane(Tool::Rect);
        p.begin_draft(Vec2::new(10.0, 10.0));
        p.update_draft(Vec2::new(10.5, 10.5));
        assert!(p.commit_draft().is_none());
        assert!(p.scene.shapes.is_empty());
    }

    #[test]
    fn pen_accumulates_and_dedupes_points() {
        let mut p = pane(Tool::Pen);
        p.begin_draft(Vec2::new(0.0, 0.0));
        p.update_draft(Vec2::new(0.2, 0.2)); // too close, skipped
        p.update_draft(Vec2::new(10.0, 5.0));
        p.update_draft(Vec2::new(20.0, 0.0));
        let preview = p.draft_preview().unwrap();
        match preview.kind {
            ShapeKind::Freehand { points } => assert_eq!(points.len(), 3),
            _ => panic!("expected freehand"),
        }
        let id = p.commit_draft().unwrap();
        assert!(matches!(p.scene.shapes[0].kind, ShapeKind::Freehand { .. }));
        assert!(p.selection.is_empty(), "new shape is not auto-selected");
    }

    #[test]
    fn arrow_draft_has_two_points_and_head() {
        let mut p = pane(Tool::Arrow);
        p.begin_draft(Vec2::new(0.0, 0.0));
        p.update_draft(Vec2::new(50.0, 80.0));
        p.commit_draft().unwrap();
        match &p.scene.shapes[0].kind {
            ShapeKind::Arrow { points, head } => {
                assert_eq!(points.len(), 2);
                assert_eq!(*head, ArrowHead::Triangle);
            }
            _ => panic!("expected arrow"),
        }
    }

    #[test]
    fn select_tool_makes_no_draft() {
        let mut p = pane(Tool::Select);
        p.begin_draft(Vec2::new(0.0, 0.0));
        assert!(p.draft.is_none());
    }

    #[test]
    fn committed_seed_is_deterministic_and_nonzero() {
        let mut p = pane(Tool::Rect);
        p.begin_draft(Vec2::new(0.0, 0.0));
        p.update_draft(Vec2::new(50.0, 50.0));
        let id = p.commit_draft().unwrap();
        assert_eq!(p.scene.shapes[0].style.seed, seed_for(id));
        assert_ne!(p.scene.shapes[0].style.seed, 0);
    }
}
