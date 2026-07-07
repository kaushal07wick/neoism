// Aggregate bundle of platform services, passed to the wasm chrome.
//
// Mirrors `neoism-ui::services::Services` but with Promise-based
// methods. The chrome host adapts these to the synchronous Rust
// trait shape by maintaining a pending-request table keyed on
// `RequestId` and re-running panels when replies arrive.

import type { ProtocolClient } from "../workspace/ProtocolClient";
import {
  DaemonFilesService,
  type FilesService,
} from "./FilesService";
import { DaemonGitService, type GitService } from "./GitService";
import {
  BrowserClipboardService,
  type ClipboardService,
} from "./ClipboardService";
import {
  DaemonCommandService,
  type CommandService,
} from "./CommandService";

export interface ServiceRegistry {
  files: FilesService;
  git: GitService;
  clipboard: ClipboardService;
  commands: CommandService;
}

/**
 * Build the default registry for the browser frontend. Files and git
 * route through the daemon WebSocket; the clipboard talks to
 * `navigator.clipboard`; commands dispatch through a local registry
 * with the protocol client wired in for handlers that need it.
 */
export function defaultServiceRegistry(
  client: ProtocolClient,
): ServiceRegistry {
  return {
    files: new DaemonFilesService(client),
    git: new DaemonGitService(client),
    clipboard: new BrowserClipboardService(client),
    commands: new DaemonCommandService(client),
  };
}
