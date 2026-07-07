pub use neoism_workspace_index::*;

pub mod tags_view {
    //! Compat shim: tags-view panel now lives in `neoism_ui::panels::tags_view`.
    pub use neoism_ui::panels::tags_view::{NeoismTagsPane, TagsViewAction};
}

pub mod extensions {
    //! Compat shim: extensions panel lives in `neoism_ui::panels::extensions_page`.
    pub use neoism_ui::panels::extensions_page::{
        ExtensionEntry, ExtensionStatus, NeoismExtensionsPane,
    };
}
