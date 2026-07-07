# The Neoism Agent

Neoism treats agents as workspace participants, not chat windows. The agent isn't a widget — it's its own local server with a full tool runtime, sub-agent orchestration, an LSP client, and persistent memory.

Open it with **Alt + A**, or prefer a third-party CLI with `:claude`, `:codex`, or `:opencode`.

## Runs as a server

`neoism-agent` is a standalone binary on loopback. The app supervises it automatically, and every surface — desktop pane, browser, phone — is a client on the same server.

- **Sessions outlive windows.** Close mid-response, reopen later, the timeline is intact.
- **One session, every device.** Drive from the desktop pane, pick up from the web UI or a phone (see [[Multiplayer]]).
- **The workspace is the context.** Real files, real shell, the same search engines the app uses.

## The tool runtime

- **Edits** with before/after snapshots (these power checkpoints and revert), plus an automatic post-edit diagnostics report.
- **LSP** the model can query — hover, definitions, references, symbols, diagnostics (the same engine as [[Editor/Languages and LSP|Languages & LSP]]).
- **Search, shell, web, and notes** tools, with background jobs so long builds don't block the conversation.
- **Sub-agents** via the `task` tool — real child sessions that run in parallel, including external agents (Claude Code, Codex, OpenCode) over ACP.

## Safety & memory

- **Permissions**: `allow` / `deny` / `ask` rules per tool; `ask` shows an approval card.
- **Checkpoints**: `/undo` and `/redo` at message granularity — including rolling files back — as a full undo *tree*.
- **Compaction**: long sessions summarize before hitting the context limit; `/compact` triggers it.
- **Memory**: a Markdown memory vault the agent recalls across sessions; external MCP servers plug in too.

## Slash commands

```text
/model /think /agent /sub-agent /skill /sessions /queue
/mcp /permissions /permit /answer /reject
/compact /undo /redo /goal /abort /new
```

## Providers

The full models.dev catalog with per-model capability handling: Anthropic (API key or OAuth), OpenAI (incl. the Responses API), GitHub Copilot auth, the opencode gateway, a Claude Code subscription bridge, and custom providers.
