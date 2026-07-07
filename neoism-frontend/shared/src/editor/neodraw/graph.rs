//! Turn a note-link graph into a neodraw [`Scene`] via a force-directed
//! layout, so the Obsidian-style "graph view" reuses the whole sketch
//! canvas (pan / zoom / movement) instead of a bespoke renderer.
//!
//! Input is deliberately generic — node labels + index edges — so the
//! desktop adapts `NoteGraphSummary` to it and this stays pure/testable.

use super::scene::{Color, Scene, Shape, ShapeId, ShapeKind, Style, Vec2};

/// Node colour (filled disc) and its label.
const NODE_FILL: Color = Color::rgb(0x41, 0x84, 0xff);
const NODE_STROKE: Color = Color::rgb(0xbf, 0xd6, 0xff);
const EDGE_COLOR: Color = Color::rgba(0x9a, 0xa4, 0xb2, 0x88);
const LABEL_COLOR: Color = Color::rgb(0xe6, 0xed, 0xf3);

/// Build a graph [`Scene`] from `labels` and undirected `edges`
/// (index pairs into `labels`). Isolated nodes are included.
pub fn graph_scene(labels: &[String], edges: &[(usize, usize)]) -> Scene {
    let n = labels.len();
    if n == 0 {
        return Scene::empty();
    }

    let mut degree = vec![0usize; n];
    for &(a, b) in edges {
        if a < n {
            degree[a] += 1;
        }
        if b < n {
            degree[b] += 1;
        }
    }

    let (xs, ys) = layout(n, edges);

    let mut shapes = Vec::with_capacity(n * 2 + edges.len());
    let mut next_id = 1u64;
    let mut id = || {
        let v = next_id;
        next_id += 1;
        ShapeId(v)
    };

    // Edges first (drawn under nodes).
    for &(a, b) in edges {
        if a >= n || b >= n || a == b {
            continue;
        }
        shapes.push(Shape {
            id: id(),
            kind: ShapeKind::Line {
                points: vec![Vec2::new(xs[a], ys[a]), Vec2::new(xs[b], ys[b])],
            },
            style: Style {
                stroke: EDGE_COLOR,
                width: 1.5,
                roughness: 0.0,
                ..Style::default()
            },
        });
    }

    // Nodes + labels.
    for i in 0..n {
        let r = node_radius(degree[i]);
        shapes.push(Shape {
            id: id(),
            kind: ShapeKind::Ellipse {
                x: xs[i] - r,
                y: ys[i] - r,
                w: r * 2.0,
                h: r * 2.0,
            },
            style: Style {
                stroke: NODE_STROKE,
                fill: Some(NODE_FILL),
                width: 1.5,
                roughness: 0.0,
                ..Style::default()
            },
        });
        shapes.push(Shape {
            id: id(),
            kind: ShapeKind::Text {
                x: xs[i] + r + 4.0,
                y: ys[i] - 9.0,
                content: labels[i].clone(),
                size: 15.0,
            },
            style: Style {
                stroke: LABEL_COLOR,
                roughness: 0.0,
                ..Style::default()
            },
        });
    }

    Scene {
        version: super::scene::SCENE_VERSION,
        shapes,
    }
}

fn node_radius(degree: usize) -> f32 {
    (10.0 + (degree as f32).sqrt() * 5.0).min(46.0)
}

/// Fruchterman–Reingold force-directed layout. Deterministic (seeded
/// initial ring) so re-running gives a stable picture.
fn layout(n: usize, edges: &[(usize, usize)]) -> (Vec<f32>, Vec<f32>) {
    use std::f32::consts::TAU;
    let mut x = vec![0.0f32; n];
    let mut y = vec![0.0f32; n];
    // Seed on a ring with a little deterministic jitter so symmetric
    // graphs don't collapse onto a line.
    for i in 0..n {
        let t = i as f32 / n as f32 * TAU;
        let jitter = (hash01(i as u32) - 0.5) * 60.0;
        x[i] = t.cos() * 360.0 + jitter;
        y[i] = t.sin() * 360.0 - jitter;
    }
    if n == 1 {
        return (vec![0.0], vec![0.0]);
    }

    let area = 1600.0 * 1100.0;
    let k = (area / n as f32).sqrt();
    let iters = 300;
    let mut dx = vec![0.0f32; n];
    let mut dy = vec![0.0f32; n];

    for it in 0..iters {
        let temp = (1.0 - it as f32 / iters as f32) * (k * 0.5) + 0.5;
        for v in dx.iter_mut() {
            *v = 0.0;
        }
        for v in dy.iter_mut() {
            *v = 0.0;
        }
        // Repulsion between every pair.
        for i in 0..n {
            for j in (i + 1)..n {
                let ddx = x[i] - x[j];
                let ddy = y[i] - y[j];
                let dist = (ddx * ddx + ddy * ddy).sqrt().max(0.01);
                let f = k * k / dist;
                let (ux, uy) = (ddx / dist, ddy / dist);
                dx[i] += ux * f;
                dy[i] += uy * f;
                dx[j] -= ux * f;
                dy[j] -= uy * f;
            }
        }
        // Attraction along edges.
        for &(a, b) in edges {
            if a >= n || b >= n || a == b {
                continue;
            }
            let ddx = x[a] - x[b];
            let ddy = y[a] - y[b];
            let dist = (ddx * ddx + ddy * ddy).sqrt().max(0.01);
            let f = dist * dist / k;
            let (ux, uy) = (ddx / dist, ddy / dist);
            dx[a] -= ux * f;
            dy[a] -= uy * f;
            dx[b] += ux * f;
            dy[b] += uy * f;
        }
        // Apply, capped by the cooling temperature.
        for i in 0..n {
            let dl = (dx[i] * dx[i] + dy[i] * dy[i]).sqrt().max(0.01);
            let capped = dl.min(temp);
            x[i] += dx[i] / dl * capped;
            y[i] += dy[i] / dl * capped;
        }
    }
    (x, y)
}

fn hash01(seed: u32) -> f32 {
    let mut z = seed.wrapping_mul(0x9E37_79B1).wrapping_add(0x7F4A_7C15);
    z ^= z >> 16;
    z = z.wrapping_mul(0x85EB_CA6B);
    z ^= z >> 13;
    (z as f32) / (u32::MAX as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_is_empty_scene() {
        assert!(graph_scene(&[], &[]).shapes.is_empty());
    }

    #[test]
    fn builds_nodes_labels_and_edges() {
        let labels = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let edges = vec![(0, 1), (1, 2)];
        let scene = graph_scene(&labels, &edges);
        let ellipses = scene
            .shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::Ellipse { .. }))
            .count();
        let texts = scene
            .shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::Text { .. }))
            .count();
        let lines = scene
            .shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::Line { .. }))
            .count();
        assert_eq!(ellipses, 3);
        assert_eq!(texts, 3);
        assert_eq!(lines, 2);
    }

    #[test]
    fn layout_positions_are_finite() {
        let labels: Vec<String> = (0..12).map(|i| format!("n{i}")).collect();
        let edges = vec![(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (0, 6), (6, 7)];
        let (x, y) = layout(labels.len(), &edges);
        assert!(x.iter().chain(y.iter()).all(|v| v.is_finite()));
        // Connected nodes shouldn't all pile onto one point.
        let spread = x.iter().cloned().fold(f32::MIN, f32::max)
            - x.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 50.0, "layout should spread nodes out");
    }
}
