use std::collections::HashMap;

pub type HitRect = (String, [f32; 4]);
pub type DiffScrollRect = (String, [f32; 4], f32);

pub fn rect_contains(rect: [f32; 4], x: f32, y: f32) -> bool {
    x >= rect[0] && x <= rect[0] + rect[2] && y >= rect[1] && y <= rect[1] + rect[3]
}

pub fn register_hit_rect(rects: &mut Vec<HitRect>, id: String, rect: [f32; 4]) {
    if !id.is_empty() {
        rects.push((id, rect));
    }
}

pub fn hit_rect_target(rects: &[HitRect], x: f32, y: f32) -> Option<(String, [f32; 4])> {
    rects
        .iter()
        .rev()
        .find(|(_, rect)| rect_contains(*rect, x, y))
        .map(|(target, rect)| (target.clone(), *rect))
}

pub fn register_diff_scroll_rect(
    rects: &mut Vec<DiffScrollRect>,
    key: String,
    rect: [f32; 4],
    max_scroll: f32,
) {
    if !key.is_empty() && max_scroll > 1.0 {
        rects.push((key, rect, max_scroll));
    }
}

pub fn diff_scroll_offset(
    offsets: &mut HashMap<String, f32>,
    key: &str,
    max_scroll: f32,
) -> f32 {
    if max_scroll <= 1.0 {
        offsets.remove(key);
        return 0.0;
    }
    let offset = offsets.entry(key.to_string()).or_insert(0.0);
    *offset = (*offset).clamp(0.0, max_scroll);
    *offset
}

pub fn scroll_diff_at(
    rects: &[DiffScrollRect],
    offsets: &mut HashMap<String, f32>,
    x: f32,
    y: f32,
    delta_pixels: f32,
) -> Option<bool> {
    let (key, _, max_scroll) = rects
        .iter()
        .rev()
        .find(|(_, rect, _)| rect_contains(*rect, x, y))
        .cloned()?;
    let offset = offsets.entry(key).or_insert(0.0);
    let next = (*offset + delta_pixels).clamp(0.0, max_scroll);
    if (next - *offset).abs() < f32::EPSILON {
        return Some(false);
    }
    *offset = next;
    Some(true)
}

pub fn update_hover_target(current: &mut Option<String>, next: Option<String>) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_rect_target_prefers_latest_registered_rect() {
        let mut rects = Vec::new();
        register_hit_rect(&mut rects, "old".to_string(), [0.0, 0.0, 20.0, 20.0]);
        register_hit_rect(&mut rects, "new".to_string(), [10.0, 10.0, 20.0, 20.0]);
        register_hit_rect(&mut rects, String::new(), [0.0, 0.0, 100.0, 100.0]);

        assert_eq!(
            hit_rect_target(&rects, 12.0, 12.0),
            Some(("new".to_string(), [10.0, 10.0, 20.0, 20.0]))
        );
        assert_eq!(
            hit_rect_target(&rects, 5.0, 5.0),
            Some(("old".to_string(), [0.0, 0.0, 20.0, 20.0]))
        );
        assert_eq!(hit_rect_target(&rects, 40.0, 40.0), None);
    }

    #[test]
    fn diff_scroll_offsets_clamp_and_prune_inactive_scroll_regions() {
        let mut offsets =
            HashMap::from([("diff".to_string(), 20.0), ("tiny".to_string(), 3.0)]);

        assert_eq!(diff_scroll_offset(&mut offsets, "diff", 10.0), 10.0);
        assert_eq!(diff_scroll_offset(&mut offsets, "tiny", 1.0), 0.0);
        assert!(!offsets.contains_key("tiny"));
    }

    #[test]
    fn diff_scroll_at_uses_topmost_hit_rect_and_reports_edge_hits() {
        let rects = vec![
            ("lower".to_string(), [0.0, 0.0, 30.0, 30.0], 100.0),
            ("upper".to_string(), [10.0, 10.0, 30.0, 30.0], 25.0),
        ];
        let mut offsets = HashMap::new();

        assert_eq!(
            scroll_diff_at(&rects, &mut offsets, 12.0, 12.0, 10.0),
            Some(true)
        );
        assert_eq!(offsets.get("upper"), Some(&10.0));
        assert_eq!(
            scroll_diff_at(&rects, &mut offsets, 12.0, 12.0, 30.0),
            Some(true)
        );
        assert_eq!(offsets.get("upper"), Some(&25.0));
        assert_eq!(
            scroll_diff_at(&rects, &mut offsets, 12.0, 12.0, 1.0),
            Some(false)
        );
        assert_eq!(
            scroll_diff_at(&rects, &mut offsets, 100.0, 100.0, 1.0),
            None
        );
    }

    #[test]
    fn update_hover_target_only_reports_real_changes() {
        let mut hover = None;
        assert!(update_hover_target(&mut hover, Some("a".to_string())));
        assert!(!update_hover_target(&mut hover, Some("a".to_string())));
        assert!(update_hover_target(&mut hover, Some("b".to_string())));
        assert!(update_hover_target(&mut hover, None));
        assert!(!update_hover_target(&mut hover, None));
    }
}
