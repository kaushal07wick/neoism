import { test } from "node:test";
import assert from "node:assert/strict";

import {
  colorCss,
  presenceCuesForGrid,
  selectionSegments,
  stablePresenceColor,
} from "./NvimPresence.ts";
import type { CrdtPeerPresence } from "../../workspace/types.ts";

function peer(overrides: Partial<CrdtPeerPresence> = {}): CrdtPeerPresence {
  return {
    buffer_id: "file:///a.md",
    peer_id: "peer-a",
    display_name: " Ada ",
    color: { r: 47, g: 128, b: 237 },
    cursor: { line: 2, column: 4, offset: null },
    selection: null,
    updated_at_ms: 100,
    ...overrides,
  };
}

test("presence cues filter buffer and local peer, then clean labels", () => {
  const cues = presenceCuesForGrid(
    [
      peer(),
      peer({ peer_id: "local", display_name: "Local" }),
      peer({ buffer_id: "file:///other.md", peer_id: "other" }),
    ],
    "file:///a.md",
    80,
    24,
    "local",
  );

  assert.equal(cues.length, 1);
  assert.equal(cues[0].peerId, "peer-a");
  assert.equal(cues[0].label, "Ada");
  assert.equal(cues[0].colorCss, "rgb(47, 128, 237)");
});

test("presence cues clamp cursor and normalize reversed selections", () => {
  const cues = presenceCuesForGrid(
    [
      peer({
        display_name: "",
        cursor: { line: 99, column: 99, offset: null },
        selection: {
          anchor: { line: 5, column: 6, offset: null },
          head: { line: 1, column: 2, offset: null },
        },
      }),
    ],
    "file:///a.md",
    5,
    3,
  );

  assert.equal(cues[0].label, "peer-a");
  assert.deepEqual(cues[0].cursor, { row: 2, col: 4 });
  assert.deepEqual(cues[0].selection?.start, { row: 1, col: 2 });
  assert.deepEqual(cues[0].selection?.end, { row: 2, col: 4 });
});

test("selection segments expand multi-line ranges by row", () => {
  const cue = presenceCuesForGrid(
    [
      peer({
        selection: {
          anchor: { line: 1, column: 3, offset: null },
          head: { line: 3, column: 2, offset: null },
        },
      }),
    ],
    "file:///a.md",
    8,
    5,
  )[0];

  assert.ok(cue.selection);
  assert.deepEqual(selectionSegments(cue.selection, 8), [
    { row: 1, startCol: 3, endCol: 7 },
    { row: 2, startCol: 0, endCol: 7 },
    { row: 3, startCol: 0, endCol: 2 },
  ]);
});

test("stable color and css conversion are deterministic", () => {
  assert.deepEqual(stablePresenceColor("peer-a"), stablePresenceColor("peer-a"));
  assert.deepEqual(stablePresenceColor("peer-a"), { r: 0x21, g: 0x92, b: 0x6b });
  assert.notDeepEqual(stablePresenceColor("peer-a"), stablePresenceColor("peer-b"));
  assert.equal(colorCss({ r: -1, g: 260, b: 16 }), "rgb(0, 255, 16)");
});
