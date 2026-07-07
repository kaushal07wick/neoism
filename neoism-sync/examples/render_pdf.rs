//! Render markdown to a reMarkable bundle for inspection.
//! `cargo run -p neoism-sync --example render_pdf -- out/`

use neoism_sync::{markdown_to_pdf, pdf_document_bundle, stable_uuid};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&out).unwrap();
    let md = "# Neoism ⇄ reMarkable\n\nThis page was rendered by Neoism and \
              pushed to the tablet to write on.\n\n## How it works\n\nMarkdown \
              becomes a page-sized PDF; handwriting comes back as CRDT ink.";
    let rendered = markdown_to_pdf(md);
    let bundle = pdf_document_bundle(
        &stable_uuid("demo"),
        "Neoism Demo",
        "",
        &rendered.bytes,
        rendered.page_count,
    );
    for (name, bytes) in &bundle.files {
        let path = format!("{out}/{name}");
        std::fs::write(&path, bytes).unwrap();
        println!("wrote {path} ({} bytes)", bytes.len());
    }
    println!(
        "pages: {}  layout lines: {}",
        rendered.page_count,
        rendered.layout.len()
    );
}
