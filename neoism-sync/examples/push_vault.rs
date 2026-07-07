//! Build a "Neoism" vault folder + a note inside it, ready to scp to the
//! tablet. `cargo run -p neoism-sync --example push_vault -- out/`

use neoism_sync::{folder_bundle, markdown_to_pdf, pdf_document_bundle, stable_uuid};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&out).unwrap();

    let folder = folder_bundle(&stable_uuid("demo-neoism-folder"), "Neoism", "");
    let md = "# Welcome to Neoism on reMarkable\n\nThis note was authored in \
              Neoism and synced to your tablet. Write on it with your pen — \
              your handwriting flows back into Neoism as an editable overlay.\n\n\
              ## How it works\n\nMarkdown becomes this page. Your ink returns \
              as CRDT strokes. The same engine will power live multiplayer \
              editing between devices.\n\nTry scribbling below this line.";
    let pdf = markdown_to_pdf(md);
    let note = pdf_document_bundle(
        &stable_uuid("demo-welcome"),
        "Welcome",
        &folder.uuid,
        &pdf.bytes,
        pdf.page_count,
    );

    for (name, bytes) in folder.files.iter().chain(note.files.iter()) {
        std::fs::write(format!("{out}/{name}"), bytes).unwrap();
    }
    println!("folder_uuid={}", folder.uuid);
    println!("note_uuid={}", note.uuid);
    println!(
        "wrote {} files to {out}",
        folder.files.len() + note.files.len()
    );
}
