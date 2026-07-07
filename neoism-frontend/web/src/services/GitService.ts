// JS-side bridge for `neoism-ui::services::GitService`.
//
// Mirrors the Rust trait surface but speaks the richer wire protocol
// from `neoism-protocol/src/git.rs` (status entries, diff hunks, log
// commits). The Rust `GitService::status` returns a coarse
// `GitStatus { branch, dirty }`; the daemon currently sends a list of
// entries — the wasm chrome can derive the coarse summary on its side.

import type { ProtocolClient } from "../workspace/ProtocolClient";
import type {
  CommitSummary,
  DiffHunk,
  GitClientMessage,
  GitServerMessage,
  GitStatusEntry,
} from "../workspace/types";

export type { CommitSummary, DiffHunk, GitStatusEntry };

export interface GitService {
  status(): Promise<GitStatusEntry[]>;
  diff(path?: string | null): Promise<DiffHunk[]>;
  log(maxCount?: number | null): Promise<CommitSummary[]>;
}

function expectVariant<K extends string>(
  reply: GitServerMessage,
  tag: K,
): Extract<GitServerMessage, Record<K, unknown>> {
  if (tag in reply) {
    return reply as Extract<GitServerMessage, Record<K, unknown>>;
  }
  if ("Error" in reply) {
    throw new Error(`git service error: ${reply.Error.message}`);
  }
  throw new Error(`unexpected reply variant for ${tag}`);
}

export class DaemonGitService implements GitService {
  constructor(private readonly client: ProtocolClient) {}

  async status(): Promise<GitStatusEntry[]> {
    const msg: GitClientMessage = "Status";
    const reply = await this.client.requestGit(msg);
    return expectVariant(reply, "Status").Status.entries;
  }

  async diff(path: string | null = null): Promise<DiffHunk[]> {
    const msg: GitClientMessage = { Diff: { path } };
    const reply = await this.client.requestGit(msg);
    return expectVariant(reply, "Diff").Diff.hunks;
  }

  async log(maxCount: number | null = null): Promise<CommitSummary[]> {
    const msg: GitClientMessage = { Log: { max_count: maxCount } };
    const reply = await this.client.requestGit(msg);
    return expectVariant(reply, "Log").Log.commits;
  }
}
