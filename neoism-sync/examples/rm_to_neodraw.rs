//! Convert reMarkable `.rm` pages into a neodraw scene Neoism can open.
//! `cargo run -p neoism-sync --example rm_to_neodraw -- out.neodraw p1.rm [p2.rm ...]`
//!
//! Emits the same neodraw JSON the desktop pull writes, so this validates
//! the device-ink → Neoism-drawing path end to end.

use neoism_sync::{remarkable, PAGE_HEIGHT};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.remove(0);

    let mut shapes = Vec::new();
    for (page_idx, rm_path) in args.iter().enumerate() {
        let bytes = std::fs::read(rm_path).expect("read .rm");
        let strokes = match remarkable::parse_rm(&bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skip {rm_path}: {e}");
                continue;
            }
        };
        let offset = page_idx as f32 * PAGE_HEIGHT;
        for s in strokes {
            let pts: Vec<String> = s
                .points
                .iter()
                .map(|p| format!("{{\"x\":{:.1},\"y\":{:.1}}}", p.x, p.y + offset))
                .collect();
            let [r, g, b, _] = s.color.0;
            shapes.push(format!(
                "{{\"id\":{},\"type\":\"freehand\",\"points\":[{}],\"style\":{{\"stroke\":\"#{r:02x}{g:02x}{b:02x}\",\"width\":{:.1}}}}}",
                s.id % 1_000_000_000,
                pts.join(","),
                s.width.max(1.0),
            ));
        }
    }

    let json = format!("{{\"version\":1,\"shapes\":[{}]}}", shapes.join(","));
    std::fs::write(&out, &json).expect("write");
    println!(
        "wrote {} ({} shapes, {} bytes)",
        out,
        shapes.len(),
        json.len()
    );
}
