//! Quick validator: parse a real `.rm` page and summarize it.
//! `cargo run -p neoism-sync --example dump_rm -- path/to/page.rm`

use neoism_sync::remarkable;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_rm <file.rm>");
    let bytes = std::fs::read(&path).expect("read file");
    println!("file: {path}  ({} bytes)", bytes.len());
    println!("version: {:?}", remarkable::detect_version(&bytes));

    match remarkable::parse_rm(&bytes) {
        Ok(strokes) => {
            let points: usize = strokes.iter().map(|s| s.points.len()).sum();
            println!("strokes: {}  points: {}", strokes.len(), points);
            let (mut minx, mut miny, mut maxx, mut maxy) =
                (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for s in &strokes {
                for p in &s.points {
                    minx = minx.min(p.x);
                    miny = miny.min(p.y);
                    maxx = maxx.max(p.x);
                    maxy = maxy.max(p.y);
                }
            }
            if points > 0 {
                println!("bounds: x[{minx:.0}..{maxx:.0}] y[{miny:.0}..{maxy:.0}]  (page is 1404x1872)");
            }
            if let Some(s) = strokes.iter().find(|s| !s.points.is_empty()) {
                println!(
                    "sample stroke: {} pts, width {:.1}, color {:?}, p0 {:?}",
                    s.points.len(),
                    s.width,
                    s.color,
                    s.points[0]
                );
            }
        }
        Err(e) => println!("parse error: {e}"),
    }
}
