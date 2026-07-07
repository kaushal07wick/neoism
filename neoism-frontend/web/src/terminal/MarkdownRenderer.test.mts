import { test } from "node:test";
import assert from "node:assert/strict";

import { isMarkdownPath, parseMarkdownBlocks } from "./MarkdownRenderer.ts";

test("detects markdown file extensions used by shared chrome", () => {
  assert.equal(isMarkdownPath("README.md"), true);
  assert.equal(isMarkdownPath("docs/note.markdown"), true);
  assert.equal(isMarkdownPath("component.mdx"), true);
  assert.equal(isMarkdownPath("src/main.ts"), false);
});

test("parses shared markdown block shapes for web rendering", () => {
  const blocks = parseMarkdownBlocks(`---
title: skip me
---
# Title

Intro with **bold** and \`code\`.

- [x] done
- [ ] todo

> quoted

\`\`\`ts
const x = 1;
\`\`\`

---`);

  assert.deepEqual(blocks, [
    { kind: "heading", level: 1, text: "Title" },
    { kind: "paragraph", text: "Intro with **bold** and `code`." },
    { kind: "task", checked: true, text: "done" },
    { kind: "task", checked: false, text: "todo" },
    { kind: "quote", text: "quoted" },
    { kind: "code", lang: "ts", code: "const x = 1;" },
    { kind: "divider" },
  ]);
});