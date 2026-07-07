//! `RemarkableSync` — the desktop-side controller that wires the
//! `neoism-sync` engine into the app.
//!
//! It owns the [`BridgeServer`] (Neoism's end of the live link), keeps a
//! [`NoteDoc`] per shared device document, and exposes the two verbs the
//! UI needs: **pull** (drain the agent, merge ink, hand back a renderable
//! overlay [`Scene`]) and **push** (render a note's markdown to a
//! reMarkable bundle and scp it over). The heavy lifting all lives in
//! `neoism-sync`; this is just the glue.

use std::collections::HashMap;
use std::process::Command;

use neoism_sync::{
    markdown_to_pdf, pdf_document_bundle, stable_uuid, BridgeMsg, BridgeServer,
    DocBundle, NoteDoc, PAGE_HEIGHT,
};

use crate::ink_interop::scene_from_strokes;
use neoism_ui::editor::neodraw::Scene;

/// Default TCP port the on-device agent dials (`--connect host:47800`).
pub const DEFAULT_BRIDGE_PORT: u16 = 47800;

pub struct RemarkableSync {
    server: Option<BridgeServer>,
    /// device document-uuid → the note CRDT receiving its ink.
    notes: HashMap<String, NoteDoc>,
}

impl Default for RemarkableSync {
    fn default() -> Self {
        Self::new()
    }
}

impl RemarkableSync {
    pub fn new() -> Self {
        Self {
            server: None,
            notes: HashMap::new(),
        }
    }

    /// Start listening for the agent (call when the reMarkable extension is
    /// enabled).
    pub fn listen(&mut self, port: u16) -> std::io::Result<()> {
        self.server = Some(BridgeServer::bind(("0.0.0.0", port))?);
        Ok(())
    }

    pub fn is_listening(&self) -> bool {
        self.server.is_some()
    }

    /// Drain the agent and merge any incoming ink. Returns the device
    /// document-ids whose overlay changed, so the caller can refresh just
    /// those. Non-blocking — safe to call every frame.
    pub fn poll(&mut self) -> Vec<String> {
        let mut changed = Vec::new();
        let Some(server) = self.server.as_mut() else {
            return changed;
        };
        for msg in server.poll() {
            if let BridgeMsg::PageInk { page_id, .. } = &msg {
                let doc_id = page_id.split('/').next().unwrap_or_default().to_string();
                let note = self.notes.entry(doc_id.clone()).or_default();
                if note.apply_bridge(&msg).is_ok() && !changed.contains(&doc_id) {
                    changed.push(doc_id);
                }
            }
        }
        changed
    }

    /// The current ink overlay for a device document as a neodraw scene,
    /// with multi-page notebooks stacked vertically (page N offset by
    /// N×page-height). Ready to hand to the neodraw renderer.
    pub fn overlay_scene(&self, doc_id: &str) -> Option<Scene> {
        let note = self.notes.get(doc_id)?;
        let mut strokes = note.strokes();

        // Give each page a vertical slot in sorted page order.
        let mut page_ids: Vec<String> =
            strokes.iter().filter_map(|s| s.page.clone()).collect();
        page_ids.sort();
        page_ids.dedup();
        let slot: HashMap<&str, usize> = page_ids
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();

        for s in &mut strokes {
            if let Some(page) = s.page.clone() {
                let offset = *slot.get(page.as_str()).unwrap_or(&0) as f32 * PAGE_HEIGHT;
                for pt in &mut s.points {
                    pt.y += offset;
                }
            }
        }
        Some(scene_from_strokes(&strokes))
    }

    /// Render a note's markdown into a reMarkable document bundle (PDF +
    /// metadata), placed in the folder `parent_uuid` (empty = top level).
    pub fn build_bundle(
        &self,
        title: &str,
        markdown: &str,
        parent_uuid: &str,
    ) -> DocBundle {
        let pdf = markdown_to_pdf(markdown);
        let uuid = stable_uuid(&format!("note:{parent_uuid}/{title}"));
        pdf_document_bundle(&uuid, title, parent_uuid, &pdf.bytes, pdf.page_count)
    }

    /// Push a bundle to the device over SSH (writes the files into
    /// xochitl's store, then restarts it so it appears). Uses the system
    /// `scp`/`ssh`. Returns the device uuid. Run off the UI thread — it
    /// blocks on the network.
    pub fn push_bundle(&self, bundle: &DocBundle, host: &str) -> std::io::Result<String> {
        let staging = std::env::temp_dir().join(format!("neoism-rm-{}", bundle.uuid));
        std::fs::create_dir_all(&staging)?;
        for (name, bytes) in &bundle.files {
            std::fs::write(staging.join(name), bytes)?;
        }
        let dest = format!("{host}:.local/share/remarkable/xochitl/");
        let ok = Command::new("scp")
            .arg("-r")
            .arg(format!("{}/.", staging.display()))
            .arg(&dest)
            .status()?
            .success();
        let _ = std::fs::remove_dir_all(&staging);
        if !ok {
            return Err(std::io::Error::other("scp to reMarkable failed"));
        }
        // Best-effort: make xochitl pick up the new document.
        let _ = Command::new("ssh")
            .arg(host)
            .arg("systemctl restart xochitl")
            .status();
        Ok(bundle.uuid.clone())
    }
}
