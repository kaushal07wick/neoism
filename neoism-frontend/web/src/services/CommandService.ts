// JS-side bridge for `neoism-ui::services::CommandService`.
//
// `CommandService::run` dispatches a chrome-level slash command
// (`/git status`, `/files reload`, etc.). There is no dedicated
// wire surface for it yet, so the default implementation routes
// commands through a local registry that the host (web shell or
// wasm chrome) populates. Unknown commands resolve to
// `CommandError::Unknown` on the Rust side; we model that as the
// promise resolving to an `UnknownCommand` tagged result rather
// than rejecting so the chrome can render a friendly hint.

import type { ProtocolClient } from "../workspace/ProtocolClient";

export type CommandResult =
  | { ok: true }
  | { ok: false; kind: "unknown" | "denied" | "io"; detail?: string };

export interface CommandService {
  run(command: string): Promise<CommandResult>;
  register(name: string, handler: CommandHandler): void;
}

export type CommandHandler = (
  rest: string,
) => Promise<CommandResult> | CommandResult;

/**
 * Default command service: parses the leading token as the command
 * name and looks it up in an in-process registry. If the chrome
 * later wants to round-trip to the daemon (e.g. `/daemon reload`),
 * the matching handler can call `client` directly.
 */
export class DaemonCommandService implements CommandService {
  private readonly handlers = new Map<string, CommandHandler>();

  // Reserved for handlers that need to talk to the daemon.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  constructor(private readonly client: ProtocolClient) {}

  register(name: string, handler: CommandHandler): void {
    this.handlers.set(name, handler);
  }

  async run(command: string): Promise<CommandResult> {
    const trimmed = command.trim();
    if (trimmed.length === 0) {
      return { ok: false, kind: "unknown", detail: "empty command" };
    }
    const spaceIdx = trimmed.indexOf(" ");
    const head = spaceIdx === -1 ? trimmed : trimmed.slice(0, spaceIdx);
    const rest = spaceIdx === -1 ? "" : trimmed.slice(spaceIdx + 1);
    const handler = this.handlers.get(head);
    if (!handler) {
      return { ok: false, kind: "unknown", detail: head };
    }
    try {
      return await handler(rest);
    } catch (err) {
      return {
        ok: false,
        kind: "io",
        detail: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Read access to the underlying client for handlers that need it. */
  protocolClient(): ProtocolClient {
    return this.client;
  }
}
