import { test } from "node:test";
import assert from "node:assert/strict";

import { RemotePresenceStore } from "./RemotePresenceStore.ts";
import { presenceBufferIdForPath } from "./presence.ts";
import type {
  CrdtPeerPresence,
  CrdtServerMessage,
} from "../workspace/types.ts";

// Mirrors the shared Rust store tests in
// `neoism-frontend/shared/src/editor/crdt/remote_presence.rs`.

function wirePresence(
  bufferId: string,
  peerId: string,
  line: number,
  at: number,
): CrdtPeerPresence {
  return {
    buffer_id: bufferId,
    peer_id: peerId,
    display_name: peerId.toUpperCase(),
    color: { r: 1, g: 2, b: 3 },
    cursor: { line, column: 4, offset: null },
    selection: null,
    updated_at_ms: at,
  };
}

function upsert(presence: CrdtPeerPresence): CrdtServerMessage {
  return { Presence: { update: { Upsert: presence } } };
}

test("store tracks upserts per buffer and exposes cursors", () => {
  const store = new RemotePresenceStore();
  assert.ok(store.applyServerMessage(upsert(wirePresence("buf-a", "alice", 3, 10))));
  assert.ok(store.applyServerMessage(upsert(wirePresence("buf-b", "bob", 7, 11))));

  const inA = store.cursorsFor("buf-a");
  assert.equal(inA.length, 1);
  assert.equal(inA[0].peer_id, "alice");
  assert.equal(inA[0].cursor.line, 3);
  assert.ok(store.hasRemoteCursors("buf-b"));
  assert.ok(!store.hasRemoteCursors("buf-missing"));
});

test("store dedupes identical upserts for cheap redraw gating", () => {
  const store = new RemotePresenceStore();
  const presence = wirePresence("buf-a", "alice", 3, 10);
  assert.ok(store.applyServerMessage(upsert(presence)));
  assert.ok(
    !store.applyServerMessage(upsert({ ...presence })),
    "identical re-publish must not report a change",
  );
  assert.ok(store.applyServerMessage(upsert(wirePresence("buf-a", "alice", 4, 12))));
});

test("store filters local peer and applies removes", () => {
  const store = new RemotePresenceStore();
  store.setLocalPeerId("me");
  assert.ok(
    !store.applyServerMessage(upsert(wirePresence("buf-a", "me", 1, 1))),
    "defensive echo filter: own peer id never lands in the store",
  );
  store.applyServerMessage(upsert(wirePresence("buf-a", "alice", 2, 2)));

  assert.ok(
    store.applyServerMessage({
      Presence: { update: { Remove: { buffer_id: "buf-a", peer_id: "alice" } } },
    }),
  );
  assert.ok(!store.hasRemoteCursors("buf-a"));
  assert.ok(
    !store.applyServerMessage({
      Presence: { update: { Remove: { buffer_id: "buf-a", peer_id: "alice" } } },
    }),
    "removing an unknown peer is a no-change",
  );
});

test("store snapshot replaces buffer state", () => {
  const store = new RemotePresenceStore();
  store.setLocalPeerId("me");
  store.applyServerMessage(upsert(wirePresence("buf-a", "stale", 9, 1)));

  assert.ok(
    store.applyServerMessage({
      PresenceSnapshot: {
        buffer_id: "buf-a",
        peers: [
          wirePresence("buf-a", "alice", 1, 5),
          wirePresence("buf-a", "me", 0, 5),
        ],
      },
    }),
  );

  const peers = store.cursorsFor("buf-a");
  assert.equal(peers.length, 1, "snapshot replaces + filters local peer");
  assert.equal(peers[0].peer_id, "alice");
});

test("store prunes stale entries by ttl", () => {
  const store = new RemotePresenceStore();
  store.applyServerMessage(upsert(wirePresence("buf-a", "old", 1, 100)));
  store.applyServerMessage(upsert(wirePresence("buf-a", "fresh", 2, 950)));

  assert.ok(store.pruneStale(1_000, 500));
  const peers = store.cursorsFor("buf-a");
  assert.equal(peers.length, 1);
  assert.equal(peers[0].peer_id, "fresh");
  assert.ok(!store.pruneStale(1_001, 500));
});

test("non-presence messages do not disturb the store", () => {
  const store = new RemotePresenceStore();
  store.applyServerMessage(upsert(wirePresence("buf-a", "alice", 1, 1)));
  assert.ok(
    !store.applyServerMessage({
      Error: { buffer_id: null, message: "nope" },
    }),
  );
  assert.ok(store.hasRemoteCursors("buf-a"));
});

test("buffer id scheme matches the daemon's file scheme", () => {
  assert.equal(
    presenceBufferIdForPath("/work/notes/a.md"),
    "file:///work/notes/a.md",
  );
  assert.equal(
    presenceBufferIdForPath("file:///work/notes/a.md"),
    "file:///work/notes/a.md",
    "already-canonical ids pass through untouched",
  );
  assert.equal(
    presenceBufferIdForPath("notes/a.md", "/work"),
    "file:///work/notes/a.md",
    "workspace-relative paths resolve against the workspace root",
  );
  assert.equal(
    presenceBufferIdForPath("./notes/a.md", "/work/"),
    "file:///work/notes/a.md",
  );
});
