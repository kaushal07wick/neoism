import { test } from "node:test";
import assert from "node:assert/strict";

import { NvimGridModel } from "./NvimGridModel.ts";

test("applies grid updates, cursor moves, mode, and default colors", () => {
  const model = new NvimGridModel();

  model.ingest({
    DefaultColors: {
      surface_id: "1",
      rgb_fg: 0xeeeeee,
      rgb_bg: 0x101010,
      rgb_sp: 0xff00ff,
    },
  });
  model.ingest({
    GridUpdate: {
      surface_id: "1",
      grid_id: 7,
      width: 4,
      height: 2,
      cells: [
        { row: 0, col: 0, ch: "H", fg: 0xffffff, bg: 0x101010, attrs: 1 },
        { row: 0, col: 1, ch: "i", fg: 0xffffff, bg: 0x101010, attrs: 0 },
      ],
      cursor: { row: 0, col: 2 },
      mode: "insert",
    },
  });

  const snapshot = model.snapshotForSurface("1");
  assert.ok(snapshot);
  assert.equal(snapshot.gridId, 7);
  assert.equal(snapshot.width, 4);
  assert.equal(snapshot.height, 2);
  assert.equal(snapshot.cells[0].ch, "H");
  assert.equal(snapshot.cells[0].attrs, 1);
  assert.deepEqual(snapshot.cursor, [0, 2]);
  assert.equal(snapshot.mode, "insert");
  assert.equal(snapshot.default_bg, 0x101010);

  model.ingest({ CursorGoto: { surface_id: "1", grid_id: 7, row: 1, col: 3 } });
  assert.deepEqual(model.snapshotForSurface("1")?.cursor, [1, 3]);
});

test("keeps independent snapshots for multiple surfaces", () => {
  const model = new NvimGridModel();

  model.ingest({
    GridUpdate: {
      surface_id: "1",
      grid_id: 1,
      width: 2,
      height: 1,
      cells: [{ row: 0, col: 0, ch: "A", fg: 1, bg: 0, attrs: 0 }],
      cursor: null,
      mode: null,
    },
  });
  model.ingest({
    GridUpdate: {
      surface_id: "2",
      grid_id: 1,
      width: 2,
      height: 1,
      cells: [{ row: 0, col: 0, ch: "B", fg: 2, bg: 0, attrs: 0 }],
      cursor: null,
      mode: null,
    },
  });

  assert.equal(model.snapshotForSurface("1")?.cells[0].ch, "A");
  assert.equal(model.snapshotForSurface("2")?.cells[0].ch, "B");
  assert.deepEqual(model.surfaceIds().sort(), ["1", "2"]);
});

test("scrolls a rectangular region and blanks exposed cells", () => {
  const model = new NvimGridModel();
  model.ingest({
    GridUpdate: {
      surface_id: "1",
      grid_id: 1,
      width: 3,
      height: 3,
      cells: Array.from({ length: 9 }, (_, idx) => ({
        row: Math.floor(idx / 3),
        col: idx % 3,
        ch: String(idx),
        fg: 0xffffff,
        bg: 0,
        attrs: 0,
      })),
      cursor: null,
      mode: null,
    },
  });

  model.ingest({
    GridScroll: {
      surface_id: "1",
      grid_id: 1,
      top: 0,
      bot: 3,
      left: 0,
      right: 3,
      rows: 1,
      cols: 0,
    },
  });

  const chars = model.snapshotForSurface("1")?.cells.map((cell) => cell.ch);
  assert.deepEqual(chars, ["3", "4", "5", "6", "7", "8", " ", " ", " "]);
});

test("clears an existing grid snapshot to blank cells", () => {
  const model = new NvimGridModel();
  model.ingest({
    DefaultColors: {
      surface_id: "1",
      rgb_fg: 0xeeeeee,
      rgb_bg: 0x101010,
      rgb_sp: 0xff00ff,
    },
  });
  model.ingest({
    GridUpdate: {
      surface_id: "1",
      grid_id: 1,
      width: 2,
      height: 2,
      cells: [
        { row: 0, col: 0, ch: "A", fg: 0xffffff, bg: 0, attrs: 0 },
        { row: 1, col: 1, ch: "B", fg: 0xffffff, bg: 0, attrs: 0 },
      ],
      cursor: { row: 1, col: 1 },
      mode: null,
    },
  });

  model.ingest({ GridClear: { surface_id: "1", grid_id: 1 } });

  const snapshot = model.snapshotForSurface("1");
  assert.deepEqual(snapshot?.cells.map((cell) => cell.ch), [" ", " ", " ", " "]);
  assert.equal(snapshot?.cells[0].fg, 0xeeeeee);
  assert.equal(snapshot?.cells[0].bg, 0x101010);
  assert.deepEqual(snapshot?.cursor, [1, 1]);
});

test("tracks highlight definitions, mouse mode, and popup menu state", () => {
  const model = new NvimGridModel();
  model.ingest({ GridResize: { surface_id: "1", grid_id: 1, width: 10, height: 4 } });
  model.ingest({
    HighlightDefined: {
      surface_id: "1",
      hl_id: 4,
      attrs: {
        fg: 0xffffff,
        bg: 0,
        sp: null,
        bold: true,
        italic: false,
        underline: false,
        undercurl: false,
        strikethrough: false,
        reverse: false,
      },
    },
  });
  model.ingest({ MouseMode: { surface_id: "1", enabled: true } });
  model.ingest({
    PopupMenu: {
      surface_id: "1",
      items: [{ word: "write", kind: "f", menu: "[cmd]", info: "" }],
      selected: 0,
      anchor: { row: 1, col: 2 },
      grid_id: 1,
    },
  });

  const snapshot = model.snapshotForSurface("1");
  assert.equal(snapshot?.mouseEnabled, true);
  assert.equal(snapshot?.popupMenu?.items[0].word, "write");

  model.ingest({ PopupHide: { surface_id: "1" } });
  assert.equal(model.snapshotForSurface("1")?.popupMenu, null);
});

test("maps crdt presence snapshots onto the matching opened buffer", () => {
  const model = new NvimGridModel();
  model.ingest({ GridResize: { surface_id: "1", grid_id: 1, width: 10, height: 4 } });
  model.ingest({
    BufferOpened: {
      surface_id: "1",
      path: "file:///a.md",
      line_count: 10,
    },
  });
  model.ingestPresence(
    {
      PresenceSnapshot: {
        buffer_id: "file:///a.md",
        peers: [
          {
            buffer_id: "file:///a.md",
            peer_id: "peer-a",
            display_name: "Ada",
            color: { r: 47, g: 128, b: 237 },
            cursor: { line: 1, column: 3, offset: null },
            selection: {
              anchor: { line: 1, column: 1, offset: null },
              head: { line: 1, column: 4, offset: null },
            },
            updated_at_ms: 100,
          },
          {
            buffer_id: "file:///a.md",
            peer_id: "local",
            display_name: "Local",
            color: { r: 1, g: 2, b: 3 },
            cursor: { line: 0, column: 0, offset: null },
            selection: null,
            updated_at_ms: 100,
          },
        ],
      },
    },
    "local",
  );

  const snapshot = model.snapshotForSurface("1");
  assert.equal(snapshot?.bufferId, "file:///a.md");
  assert.equal(snapshot?.presence.length, 1);
  assert.equal(snapshot?.presence[0].label, "Ada");
  assert.deepEqual(snapshot?.presence[0].cursor, { row: 1, col: 3 });
  assert.deepEqual(snapshot?.presence[0].selection?.end, { row: 1, col: 4 });
});

test("applies incremental crdt presence upserts and removals", () => {
  const model = new NvimGridModel();
  model.ingest({ GridResize: { surface_id: "1", grid_id: 1, width: 5, height: 3 } });
  model.ingest({
    BufferOpened: {
      surface_id: "1",
      path: "file:///a.md",
      line_count: 10,
    },
  });

  model.ingestPresence({
    Presence: {
      update: {
        Upsert: {
          buffer_id: "file:///a.md",
          peer_id: "peer-a",
          display_name: "Ada",
          color: { r: 47, g: 128, b: 237 },
          cursor: { line: 2, column: 9, offset: null },
          selection: null,
          updated_at_ms: 100,
        },
      },
    },
  });
  assert.deepEqual(model.snapshotForSurface("1")?.presence[0].cursor, {
    row: 2,
    col: 4,
  });

  model.ingestPresence({
    Presence: {
      update: {
        Remove: {
          buffer_id: "file:///a.md",
          peer_id: "peer-a",
        },
      },
    },
  });
  assert.equal(model.snapshotForSurface("1")?.presence.length, 0);
});
