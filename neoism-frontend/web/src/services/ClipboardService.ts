// JS-side bridge for `neoism-ui::services::ClipboardService`.
//
// Unlike files/git, the clipboard lives in the *browser*, not the
// daemon — the daemon has no view of the user's selection buffer.
// We route through the async `navigator.clipboard` API and surface a
// synchronous-feeling shape that matches the trait.
//
// `read()` returns `null` instead of `undefined`/`""` when permission
// is denied or the API is unavailable so callers can distinguish
// "no clipboard" from "empty clipboard". The wasm chrome maps this
// to `Option<String>::None` for `ClipboardService::read`.

import type { ProtocolClient } from "../workspace/ProtocolClient";
import type { ClipboardPayload, WorkspaceServerMessage } from "../workspace/types";

export interface ClipboardService {
  read(): Promise<string | null>;
  write(text: string): Promise<void>;
  ingestServerMessage?(msg: WorkspaceServerMessage): void;
}

export class BrowserClipboardService implements ClipboardService {
  private cachedPayload: ClipboardPayload | null = null;

  constructor(private readonly client?: ProtocolClient) {}

  async read(): Promise<string | null> {
    this.client?.sendWorkspace("LoadClipboard");
    if (typeof navigator === "undefined" || !navigator.clipboard) {
      return this.cachedPayload?.text ?? null;
    }
    try {
      const text = await navigator.clipboard.readText();
      if (text.length > 0) {
        const payload = textClipboardPayload(text);
        this.cachedPayload = payload;
        this.client?.sendWorkspace({ StoreClipboard: { payload } });
        return text;
      }
      return this.cachedPayload?.text ?? text;
    } catch {
      return this.cachedPayload?.text ?? null;
    }
  }

  async write(text: string): Promise<void> {
    const payload = textClipboardPayload(text);
    this.cachedPayload = payload;
    this.client?.sendWorkspace({ StoreClipboard: { payload } });
    if (typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Best-effort: the trait return is unit, and we don't want to
      // throw if the user revoked permission mid-session.
    }
  }

  ingestServerMessage(msg: WorkspaceServerMessage): void {
    if ("ClipboardPayload" in msg) {
      this.cachedPayload = msg.ClipboardPayload.payload;
    }
  }
}

/**
 * Synchronous fallback that mirrors the trait shape exactly. Used by
 * callers that can't await (e.g. some legacy synchronous wasm host
 * paths). The string state lives in memory only.
 */
export class InMemoryClipboardService implements ClipboardService {
  private buffer: string | null = null;

  async read(): Promise<string | null> {
    return this.buffer;
  }

  async write(text: string): Promise<void> {
    this.buffer = text;
  }
}

function textClipboardPayload(text: string): ClipboardPayload {
  return {
    mime_type: "text/plain",
    text,
    bytes: Array.from(new TextEncoder().encode(text)),
    filename: null,
  };
}
