//! Mirror of the desktop "Share with reMarkable" handler, runnable from
//! the CLI to validate the multi-note push path.
//! `cargo run -p neoism-sync --example share_dir -- <vault-dir> <out-dir>`

use neoism_sync::{folder_bundle, markdown_to_pdf, pdf_document_bundle, stable_uuid};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let vault_dir =
        PathBuf::from(args.next().expect("usage: share_dir <vault-dir> <out-dir>"));
    let out = args.next().unwrap_or_else(|| ".".into());
    let vault_name = vault_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Vault".into());
    std::fs::create_dir_all(&out).unwrap();

    let mut md = Vec::new();
    let mut stack = vec![vault_dir];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
                md.push(p);
            }
        }
    }

    let folder = folder_bundle(
        &stable_uuid(&format!("vault:{vault_name}")),
        &vault_name,
        "",
    );
    let mut files = folder.files.clone();
    for path in &md {
        let src = std::fs::read_to_string(path).unwrap_or_default();
        let title = path
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("Note")
            .to_string();
        let pdf = markdown_to_pdf(&src);
        let note = pdf_document_bundle(
            &stable_uuid(&format!("note:{}", path.display())),
            &title,
            &folder.uuid,
            &pdf.bytes,
            pdf.page_count,
        );
        files.extend(note.files);
    }
    for (name, bytes) in &files {
        std::fs::write(format!("{out}/{name}"), bytes).unwrap();
    }
    println!(
        "vault '{vault_name}': {} notes -> {} files in {out}",
        md.len(),
        files.len()
    );
}
