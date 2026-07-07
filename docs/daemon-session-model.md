# Daemon session model: workspace-session vs PTY-session ids

Status: implemented (Wave 1C). Scope: `neoism-workspace-daemon/src/workspace.rs`
and `neoism-workspace-daemon/src/sessions.rs`. Companion to the
"Work From Anywhere" plan (locked architecture decision #3:
**no live PTY migration**).

## Two kinds of "session"

The daemon tracks sessions in two independent registries, both living on
`AppState` (see `server.rs`):

| | Workspace session (a *tab*) | PTY session (a *shell*) |
|---|---|---|
| Owner | `WorkspaceManager` (`workspace.rs`) | `SessionRegistry` (`sessions.rs`) |
| Id type | `SessionSummary::id` (UUID string) | PTY `session_id` (UUID string) |
| Lifetime | **Durable** — persisted in the daemon snapshot, survives restarts and host moves | **Ephemeral** — names a live OS process; dies with the shell, the daemon, or a host move |
| Carries | recorded `cwd`, `label`, `workspace_id`, `last_active` | the `PtySession`, output backlog, reader task |

A *tab* is the thing a user sees and reopens; a *shell* is the live
process that happens to back it right now. Historically these were
minted as unrelated UUIDs with **no link between them**, so a roaming or
reconnecting client could not reliably tell which live PTY backed a given
workspace tab. That was the root of the `TODO(wave-cutover)` markers.

## The link

A minimal bidirectional id-map closes the gap, with one half maintained
on each side (no cross-crate / protocol change, no shared mutable
reference between the two registries):

- **Workspace side** — `ManagerInner.session_pty_links:
  HashMap<workspace_session_id, pty_session_id>`, maintained by:
  - `WorkspaceManager::link_pty_session(tab_id, pty_id)` — record/replace
    (no-op if the tab is unknown).
  - `WorkspaceManager::pty_session_for(tab_id) -> Option<pty_id>` —
    resolve a tab to its live shell.
  - `WorkspaceManager::unlink_pty_session(tab_id)` — forget the shell,
    keep the tab.
  - Cleared automatically on `remove_session` and on
    `rehydrate_from_snapshot` (a restored world has no live shells).

- **PTY side** — `SessionEntry.workspace_session_id: Option<String>`,
  maintained by:
  - `SessionRegistry::link_workspace_session(pty_id, tab_id)` — record
    (returns `false` if the PTY is unknown / already exited).
  - `SessionRegistry::workspace_session_for(pty_id) -> Option<tab_id>`.
  - `SessionRegistry::pty_for_workspace_session(tab_id) -> Option<pty_id>`
    (reverse lookup).
  - Dropped automatically when the PTY entry is removed (close / exit /
    read error), because the link rides on the entry.

The link is established at bridge time. The clearest case is
`bind_editor_surface` in `workspace.rs`: when web binds an editor surface
using a PTY id it already holds from the transport layer, the workspace
tab id *is* that PTY id, and we record a self-link so `pty_session_for`
resolves it without callers special-casing "id == pty id".

The two halves are kept consistent by whoever holds both registries (the
`/session` PTY socket task on `AppState`). The maps are deliberately
**not** persisted — PTY ids are meaningless across a restart.

## Respawn-in-cwd policy

PTYs never migrate. Whenever a tab's directory changes or the daemon's
home host moves:

1. The **logical tab is preserved** (durable registry entry + recorded
   `cwd`).
2. The old shell is abandoned and its PTY link dropped.
3. On next attach a **fresh shell is spawned in the recorded `cwd`**
   (`SessionRegistry::create` already takes a `cwd`), and a new link is
   recorded via `link_pty_session` / `link_workspace_session`.
4. **Agents resume** from serialized state (`resume_prompt_queues`),
   independent of the shell respawn.

This is exactly how `SetCwd` behaves: `update_session_cwd` writes the new
`cwd` to the durable tab, then `unlink_pty_session` drops the now-stale
PTY link so the next attach respawns in the new directory instead of
trying to `cd` a live process. The dispatcher only owns
`&WorkspaceManager`, so it does not kill the old shell itself — clearing
the link is the correct in-scope hand-off to the PTY-socket owner, which
sees "no link" and respawns.

## Resolved `TODO(wave-cutover)` markers

- `workspace.rs` `SetCwd` handler — **closed.** Replaced the
  "book-keeping only" TODO with the respawn-in-cwd implementation
  (update cwd + drop stale link) and a comment stating the policy.
- The `workspace.rs` module header note about PTY/nvim retarget being
  "deliberately TODO'd / SessionRegistry per-socket" — **closed.**
  Replaced with the id-model + policy description above. (The registry
  is in fact shared on `AppState`, not per-socket; the stale comment was
  the source of the confusion.)

Markers left open (out of Wave 1C scope): the three `agent.rs`
`TODO(wave-cutover)` items about agent-server route shapes — these are
about the agent transport, not the session-id link, and belong to a
different wave.
