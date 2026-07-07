import { test } from "node:test";
import assert from "node:assert/strict";

import {
  caretLayoutForLine,
  type MarkdownPresenceAnchor,
} from "./MarkdownPresenceOverlay.ts";
import { parseMarkdownBlockSpans } from "./MarkdownRenderer.ts";

const anchors: MarkdownPresenceAnchor[] = [
  // heading on source line 0
  { line: 0, endLine: 0, top: 10, height: 30, left: 40 },
  // 4-line paragraph on source lines 2..5
  { line: 2, endLine: 5, top: 60, height: 80, left: 40 },
  // fenced code block on source lines 7..10
  { line: 7, endLine: 10, top: 160, height: 88, left: 40 },
];

test("caret lands inside the block that owns the source line", () => {
  assert.deepEqual(caretLayoutForLine(anchors, 0), {
    top: 10,
    height: 30,
    left: 40,
  });
  // Line 4 is the third of four lines in the paragraph block.
  assert.deepEqual(caretLayoutForLine(anchors, 4), {
    top: 60 + 2 * 20,
    height: 20,
    left: 40,
  });
  // Last code line.
  assert.deepEqual(caretLayoutForLine(anchors, 10), {
    top: 160 + 3 * 22,
    height: 22,
    left: 40,
  });
});

test("caret on a blank separator line snaps to the next block", () => {
  const layout = caretLayoutForLine(anchors, 6);
  assert.ok(layout);
  assert.equal(layout.top, 160);
  assert.equal(layout.left, 40);
});

test("caret past the end of the document draws nothing", () => {
  assert.equal(caretLayoutForLine(anchors, 42), null);
  assert.equal(caretLayoutForLine([], 0), null);
});

test("block spans carry the original source line ranges", () => {
  const spans = parseMarkdownBlockSpans(
    [
      "---", // 0 frontmatter
      "title: skip", // 1
      "---", // 2
      "# Title", // 3
      "", // 4
      "first line", // 5
      "second line", // 6
      "", // 7
      "```ts", // 8
      "const x = 1;", // 9
      "```", // 10
      "", // 11
      "- [ ] todo", // 12
    ].join("\n"),
  );

  assert.deepEqual(
    spans.map((span) => [span.block.kind, span.line, span.endLine]),
    [
      ["heading", 3, 3],
      ["paragraph", 5, 6],
      ["code", 8, 10],
      ["task", 12, 12],
    ],
  );
});

test("unterminated code blocks span to the end of the source", () => {
  const spans = parseMarkdownBlockSpans("intro\n\n```sh\necho hi");
  assert.deepEqual(
    spans.map((span) => [span.block.kind, span.line, span.endLine]),
    [
      ["paragraph", 0, 0],
      ["code", 2, 3],
    ],
  );
});
