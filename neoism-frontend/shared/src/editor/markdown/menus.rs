use super::types::{MarkdownBlockTemplate, MarkdownWikiLinkKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownBlockMenuEntry {
    pub label: &'static str,
    pub hint: &'static str,
    pub preview: &'static str,
    pub template: MarkdownBlockTemplate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownLinkCompletionMenuMeta {
    pub hint: &'static str,
    pub preview: &'static str,
}

pub const MARKDOWN_BLOCK_MENU_ENTRIES: &[MarkdownBlockMenuEntry] = &[
    MarkdownBlockMenuEntry {
        label: "Task",
        hint: "task",
        preview: "\u{2610}",
        template: MarkdownBlockTemplate::TaskList,
    },
    MarkdownBlockMenuEntry {
        label: "Text",
        hint: "text",
        preview: "Aa",
        template: MarkdownBlockTemplate::Paragraph,
    },
    MarkdownBlockMenuEntry {
        label: "Link Note",
        hint: "link note",
        preview: "\u{f15c}",
        template: MarkdownBlockTemplate::WikiLink,
    },
    MarkdownBlockMenuEntry {
        label: "Page Link",
        hint: "page link",
        preview: "\u{f0c1}",
        template: MarkdownBlockTemplate::CodeLink,
    },
    MarkdownBlockMenuEntry {
        label: "Heading 1",
        hint: "h1",
        preview: "H1",
        template: MarkdownBlockTemplate::Heading1,
    },
    MarkdownBlockMenuEntry {
        label: "Heading 2",
        hint: "h2",
        preview: "H2",
        template: MarkdownBlockTemplate::Heading2,
    },
    MarkdownBlockMenuEntry {
        label: "Heading 3",
        hint: "h3",
        preview: "H3",
        template: MarkdownBlockTemplate::Heading3,
    },
    MarkdownBlockMenuEntry {
        label: "Bullet List",
        hint: "bullet",
        preview: "\u{f0ca}",
        template: MarkdownBlockTemplate::BulletList,
    },
    MarkdownBlockMenuEntry {
        label: "Quote",
        hint: "quote",
        preview: "\u{f10d}",
        template: MarkdownBlockTemplate::Quote,
    },
    MarkdownBlockMenuEntry {
        label: "Code Block",
        hint: "code",
        preview: "\u{f121}",
        template: MarkdownBlockTemplate::CodeBlock,
    },
    MarkdownBlockMenuEntry {
        label: "Table",
        hint: "table",
        preview: "\u{f0ce}",
        template: MarkdownBlockTemplate::Table,
    },
    MarkdownBlockMenuEntry {
        label: "Divider",
        hint: "divider",
        preview: "\u{2014}",
        template: MarkdownBlockTemplate::Divider,
    },
];

pub fn markdown_block_menu_entries() -> &'static [MarkdownBlockMenuEntry] {
    MARKDOWN_BLOCK_MENU_ENTRIES
}

pub fn markdown_block_template_opens_link_completion(
    template: MarkdownBlockTemplate,
) -> bool {
    matches!(
        template,
        MarkdownBlockTemplate::WikiLink | MarkdownBlockTemplate::CodeLink
    )
}

pub fn markdown_link_completion_menu_title(kind: MarkdownWikiLinkKind) -> &'static str {
    match kind {
        MarkdownWikiLinkKind::CodeRef => "Link page",
        MarkdownWikiLinkKind::Heading => "Link heading",
        MarkdownWikiLinkKind::Note => "Link note",
    }
}

pub fn markdown_link_completion_menu_meta(
    kind: MarkdownWikiLinkKind,
    creating: bool,
) -> MarkdownLinkCompletionMenuMeta {
    if creating {
        return MarkdownLinkCompletionMenuMeta {
            hint: "Create",
            preview: "+",
        };
    }

    match kind {
        MarkdownWikiLinkKind::CodeRef => MarkdownLinkCompletionMenuMeta {
            hint: "Enter",
            preview: "\u{f0c1}",
        },
        MarkdownWikiLinkKind::Heading => MarkdownLinkCompletionMenuMeta {
            hint: "Enter",
            preview: "#",
        },
        MarkdownWikiLinkKind::Note => MarkdownLinkCompletionMenuMeta {
            hint: "Enter",
            preview: "\u{f15c}",
        },
    }
}
