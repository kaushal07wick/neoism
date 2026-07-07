---
sidebar_position: 3
title: Tools And Permissions
---

# Tools And Permissions

Neoism agents work through explicit tools rather than unrestricted access to the machine. The tool runtime can execute searches, reads, shell commands, patches, notes operations, task tracking, browser/web actions, and other workspace-aware operations.

## Tool Results

Tool results include output and metadata. Some tools attach snapshots or structured metadata so the UI can render diffs, diagnostics, search results, or task state.

Tools should be treated as the agent's only access path. A good agent flow searches and reads first, proposes or applies focused edits second, and runs verification commands last.

## Permissions

Permission rules protect higher-risk actions. A session may allow safe read-only tools while requiring approval for shell commands, writes, external directories, or other sensitive operations.

Permission actions are:

- `allow` - run without asking.
- `ask` - request approval.
- `deny` - block the action.

Runtime permission replies include one-time approval, always-allow approval, and rejection. Pattern rules are matched by permission name and target path/command pattern.

Higher-risk permissions include shell commands, file writes, patch application, external paths, browser/network actions, and provider or MCP actions that leave the local workspace. Keep defaults conservative when a session is connected to an unfamiliar repository.

## Generated Files

Keep generated files ignored. Tool metadata and git-aware snapshots can become huge when unignored scratch directories contain random binaries or millions of files. The workspace ignores `.tmp/` for this reason.

## Good Agent Hygiene

- Keep task prompts scoped.
- Prefer read/search tools before edits.
- Approve commands only when you understand the action.
- Ignore generated directories such as `.tmp/`, `target/`, `node_modules/`, and build outputs.

## MCP

Agent config supports MCP entries for local and remote servers. Local entries define a command, optional args/environment, enabled state, and timeout. Remote entries define a URL, optional headers, OAuth settings, enabled state, and timeout.

MCP tools should be documented like first-party tools: what they can read, what they can write, what network or account access they require, and which permission rule should guard them.