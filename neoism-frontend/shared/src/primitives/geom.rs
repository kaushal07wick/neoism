//! Pure geometry helpers shared across chrome panels.
//!
//! Every helper is a pure free function with no panel-specific deps —
//! state stays in the panel.

#[inline]
pub fn snap_to_device_px(value: f32, scale_factor: f32) -> f32 {
    if scale_factor <= 0.0 {
        value
    } else {
        (value * scale_factor).round() / scale_factor
    }
}

pub fn rects_intersect(a: [f32; 4], b: [f32; 4]) -> bool {
    let (ax1, ay1, ax2, ay2) = (a[0], a[1], a[0] + a[2], a[1] + a[3]);
    let (bx1, by1, bx2, by2) = (b[0], b[1], b[0] + b[2], b[1] + b[3]);
    ax1 < bx2 && ax2 > bx1 && ay1 < by2 && ay2 > by1
}

#[allow(dead_code)]
pub fn intersect_rect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

pub fn edge_row_radii(
    y: f32,
    h: f32,
    clip_top: f32,
    clip_bottom: f32,
    radius: f32,
) -> [f32; 4] {
    let top = if y <= clip_top + 0.5 { radius } else { 0.0 };
    let bottom = if y + h >= clip_bottom - 0.5 {
        radius
    } else {
        0.0
    };
    [top, top, bottom, bottom]
}

pub fn edge_left_row_radii(
    y: f32,
    h: f32,
    clip_top: f32,
    clip_bottom: f32,
    radius: f32,
) -> [f32; 4] {
    let top = if y <= clip_top + 0.5 { radius } else { 0.0 };
    let bottom = if y + h >= clip_bottom - 0.5 {
        radius
    } else {
        0.0
    };
    [top, 0.0, 0.0, bottom]
}
