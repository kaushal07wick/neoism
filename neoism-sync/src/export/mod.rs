//! The push half: turn a note's markdown into something the reMarkable
//! can display + annotate — a page-sized PDF ([`pdf`]) wrapped in a
//! xochitl document bundle ([`xochitl`]). The matching ink comes back via
//! the bridge and is anchored using the PDF [`LayoutItem`](pdf::LayoutItem)
//! map.

pub mod pdf;
pub mod xochitl;

pub use pdf::{markdown_to_pdf, LayoutItem, RenderedPdf};
pub use xochitl::{folder_bundle, pdf_document_bundle, stable_uuid, DocBundle};
