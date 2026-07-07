//! Build the xochitl document bundle for an annotated PDF.
//!
//! The reMarkable shows a document as a UUID file bundle: `<uuid>.pdf` +
//! `<uuid>.metadata` + `<uuid>.content` + `<uuid>.pagedata`. We render the
//! note's markdown to the PDF (see [`super::pdf`]) and emit the JSON
//! around it so the tablet lists it as an annotatable document inside the
//! Neoism vault folder. Schema modelled on a real `formatVersion 2`
//! notebook off the device; the PDF-document specifics (fileType `pdf`,
//! `redirectionPageMap`) follow the documented structure and should be
//! confirmed against a real imported-PDF sample before trusting writes.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

/// A document ready to drop into the device's xochitl directory: each
/// entry is `(filename, bytes)`.
#[derive(Debug, Clone)]
pub struct DocBundle {
    pub uuid: String,
    pub files: Vec<(String, Vec<u8>)>,
}

/// Build the bundle for `pdf` (rendered markdown) titled `title`, placed
/// inside the folder `parent_uuid` (empty string = top level). `uuid` is
/// the document id — pass a **stable** one (see [`stable_uuid`]) keyed by
/// the note's path so re-sharing overwrites in place instead of creating
/// duplicates.
pub fn pdf_document_bundle(
    uuid: &str,
    title: &str,
    parent_uuid: &str,
    pdf: &[u8],
    page_count: usize,
) -> DocBundle {
    let now = now_ms();
    let pages = page_count.max(1);

    let metadata = json!({
        "deleted": false,
        "lastModified": now.to_string(),
        "lastOpened": now.to_string(),
        "lastOpenedPage": 0,
        "metadatamodified": false,
        "modified": false,
        "parent": parent_uuid,
        "pinned": false,
        "synced": false,
        "type": "DocumentType",
        "version": 1,
        "visibleName": title,
    });

    // Each page must carry an inline `redir` mapping it to its PDF page,
    // or xochitl renders it as a blank notebook page. (Confirmed by
    // inspecting how the device rewrote a pushed doc.)
    let cpages: Vec<_> = (0..pages)
        .map(|i| {
            json!({
                "id": stable_uuid(&format!("{uuid}-page-{i}")),
                "idx": { "timestamp": "1:1", "value": frac_idx(i) },
                "redir": { "timestamp": "1:1", "value": i },
            })
        })
        .collect();

    let content = json!({
        "coverPageNumber": 0,
        "customZoomCenterX": 0,
        "customZoomCenterY": 936,
        "customZoomOrientation": "portrait",
        "customZoomPageHeight": 1872,
        "customZoomPageWidth": 1404,
        "customZoomScale": 1,
        "documentMetadata": {},
        "extraMetadata": {},
        "fileType": "pdf",
        "fontName": "",
        "formatVersion": 2,
        "lineHeight": -1,
        "margins": 125,
        "orientation": "portrait",
        "pageCount": pages,
        "pageTags": [],
        "sizeInBytes": pdf.len().to_string(),
        "tags": [],
        "textAlignment": "justify",
        "textScale": 1,
        "zoomMode": "fitToWidth",
        "cPages": {
            "lastOpened": { "timestamp": "0:0", "value": "" },
            "original": { "timestamp": "0:0", "value": -1 },
            "pages": cpages,
            "uuids": [ { "first": stable_uuid(&format!("{uuid}-author")), "second": 1 } ],
        },
    });

    // One template line per page (PDF pages need none; "Blank" is safe).
    let pagedata = vec!["Blank"; pages].join("\n");

    DocBundle {
        files: vec![
            (format!("{uuid}.pdf"), pdf.to_vec()),
            (
                format!("{uuid}.metadata"),
                serde_json::to_vec_pretty(&metadata).unwrap_or_default(),
            ),
            (
                format!("{uuid}.content"),
                serde_json::to_vec_pretty(&content).unwrap_or_default(),
            ),
            (format!("{uuid}.pagedata"), pagedata.into_bytes()),
        ],
        uuid: uuid.to_string(),
    }
}

/// Build a folder (`CollectionType`) bundle — a "Neoism vault" or any
/// sub-folder. `uuid` should be **stable** (keyed by the folder's path)
/// so re-syncing reuses it; `parent_uuid` empty = top level.
pub fn folder_bundle(uuid: &str, title: &str, parent_uuid: &str) -> DocBundle {
    let now = now_ms();
    let metadata = json!({
        "deleted": false,
        "lastModified": now.to_string(),
        "metadatamodified": false,
        "modified": false,
        "parent": parent_uuid,
        "pinned": false,
        "synced": false,
        "type": "CollectionType",
        "version": 1,
        "visibleName": title,
    });
    let content = json!({ "tags": [] });
    DocBundle {
        files: vec![
            (
                format!("{uuid}.metadata"),
                serde_json::to_vec_pretty(&metadata).unwrap_or_default(),
            ),
            (
                format!("{uuid}.content"),
                serde_json::to_vec_pretty(&content).unwrap_or_default(),
            ),
        ],
        uuid: uuid.to_string(),
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// A fractional-index ordering key, mirroring the device's `"ba","bb",…`.
fn frac_idx(i: usize) -> String {
    format!("b{}", (b'a' + (i as u8 % 26)) as char)
}

/// A **deterministic** v4-shaped UUID derived purely from `seed`. Same
/// seed → same UUID, which is what makes re-syncing idempotent: key it by
/// a note/folder's stable path and re-pushing overwrites in place instead
/// of piling up duplicate documents.
pub fn stable_uuid(seed: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in seed.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }

    let mut bytes = [0u8; 16];
    for (i, slot) in bytes.iter_mut().enumerate() {
        h ^= (i as u64).wrapping_add(1);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        *slot = (h >> 24) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant

    let h = |r: std::ops::Range<usize>| {
        bytes[r]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    format!(
        "{}-{}-{}-{}-{}",
        h(0..4),
        h(4..6),
        h(6..8),
        h(8..10),
        h(10..16)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_has_the_four_files_and_valid_json() {
        let pdf = b"%PDF-1.4 fake";
        let bundle = pdf_document_bundle("note-uuid", "My Note", "folder-uuid", pdf, 3);

        let names: Vec<_> = bundle.files.iter().map(|(n, _)| n.clone()).collect();
        for ext in ["pdf", "metadata", "content", "pagedata"] {
            assert!(names.iter().any(|n| n.ends_with(ext)), "missing .{ext}");
        }

        let content_bytes = &bundle
            .files
            .iter()
            .find(|(n, _)| n.ends_with(".content"))
            .unwrap()
            .1;
        let content: serde_json::Value = serde_json::from_slice(content_bytes).unwrap();
        assert_eq!(content["formatVersion"], 2);
        assert_eq!(content["fileType"], "pdf");
        assert_eq!(content["pageCount"], 3);
        let cpages = content["cPages"]["pages"].as_array().unwrap();
        assert_eq!(cpages.len(), 3);
        // Each page redirects to its PDF page — without this the device
        // shows blank pages.
        assert_eq!(cpages[0]["redir"]["value"], 0);
        assert_eq!(cpages[2]["redir"]["value"], 2);

        let meta_bytes = &bundle
            .files
            .iter()
            .find(|(n, _)| n.ends_with(".metadata"))
            .unwrap()
            .1;
        let meta: serde_json::Value = serde_json::from_slice(meta_bytes).unwrap();
        assert_eq!(meta["visibleName"], "My Note");
        assert_eq!(meta["type"], "DocumentType");
        assert_eq!(meta["parent"], "folder-uuid");
    }

    #[test]
    fn stable_uuids_are_deterministic_and_shaped() {
        // Same seed → same UUID (this is what de-dups re-shares); different
        // seed → different UUID.
        assert_eq!(stable_uuid("vault/note.md"), stable_uuid("vault/note.md"));
        assert_ne!(stable_uuid("a"), stable_uuid("b"));
        let u = stable_uuid("x");
        assert_eq!(u.len(), 36);
        assert_eq!(u.as_bytes()[14], b'4'); // version nibble
    }
}
