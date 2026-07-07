import type {
  CrdtPeerPresence,
  CrdtServerMessage,
  EditorServerMessage,
  GridCell,
  HighlightAttrs,
  PopupMenuItem,
} from "../../workspace/types";
import { presenceCuesForGrid, type NvimPresenceCue } from "./NvimPresence";
import { presenceBufferIdForPath } from "../../presence/presence";

export interface NvimGridCellSnapshot {
  ch: string;
  fg: number;
  bg: number;
  attrs: number;
}

export interface NvimGridCursorSnapshot {
  row: number;
  col: number;
  visible: boolean;
}

export interface NvimPopupMenuSnapshot {
  items: PopupMenuItem[];
  selected: number | null;
  anchor: { row: number; col: number };
  gridId: number;
}

export interface NvimGridSnapshot {
  surfaceId: string | null;
  bufferId: string | null;
  gridId: number;
  topline: number;
  curline: number | null;
  curcol: number | null;
  width: number;
  height: number;
  cells: NvimGridCellSnapshot[];
  cursor: [number, number] | null;
  cursorState: NvimGridCursorSnapshot | null;
  default_fg: number;
  default_bg: number;
  default_sp: number;
  mode: string | null;
  mouseEnabled: boolean;
  popupMenu: NvimPopupMenuSnapshot | null;
  presence: NvimPresenceCue[];
  error: string | null;
}

interface SurfaceState {
  surfaceId: string | null;
  bufferId: string | null;
  gridId: number;
  /** win_viewport topline — converts buffer-coordinate presence
   *  lines into grid rows for this surface's current scroll. */
  topline: number;
  botline: number;
  /** Gutter width in cells (line numbers etc.). */
  textoff: number;
  /** Buffer-coordinate caret (curline/curcol) — what the presence
   *  plane publishes for this surface. */
  curline: number | null;
  curcol: number | null;
  width: number;
  height: number;
  cells: NvimGridCellSnapshot[];
  cursor: NvimGridCursorSnapshot | null;
  defaultFg: number;
  defaultBg: number;
  defaultSp: number;
  mode: string | null;
  mouseEnabled: boolean;
  popupMenu: NvimPopupMenuSnapshot | null;
  error: string | null;
  highlights: Map<number, HighlightAttrs>;
}

const PRIMARY_SURFACE_KEY = "__primary__";
const DEFAULT_FG = 0xe8e8e8;
const DEFAULT_BG = 0x000000;
const DEFAULT_SP = 0xe8e8e8;

export class NvimGridModel {
  private readonly surfaces = new Map<string, SurfaceState>();
  private readonly presenceByBuffer = new Map<string, Map<string, CrdtPeerPresence>>();
  private localPresencePeerId: string | null = null;
  private activeSurfaceKey: string = PRIMARY_SURFACE_KEY;

  ingest(message: EditorServerMessage): void {
    if (typeof message === "object" && "Batch" in message) {
      const event = message.Batch;
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      for (const child of event.messages) {
        this.ingest(child);
      }
      return;
    }

    if ("PopupHide" in message) {
      const event = message.PopupHide;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.popupMenu = null;
      return;
    }

    if ("GridResize" in message) {
      const event = message.GridResize;
      const surface = this.surfaceFor(event.surface_id ?? null);
      this.resize(surface, event.grid_id, event.width, event.height);
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("GridUpdate" in message) {
      const event = message.GridUpdate;
      const surface = this.surfaceFor(event.surface_id ?? null);
      this.ensureSize(surface, event.grid_id, event.width, event.height);
      for (const cell of event.cells) {
        this.putCell(surface, cell);
      }
      if (event.cursor) {
        this.setCursor(surface, event.cursor.row, event.cursor.col);
      }
      if (event.mode !== null && event.mode !== undefined) {
        surface.mode = event.mode;
      }
      surface.error = null;
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("GridClear" in message) {
      const event = message.GridClear;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.gridId = event.grid_id;
      this.clear(surface);
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("GridScroll" in message) {
      const event = message.GridScroll;
      const surface = this.surfaceFor(event.surface_id ?? null);
      this.scroll(surface, event.top, event.bot, event.left, event.right, event.rows, event.cols);
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("CursorGoto" in message) {
      const event = message.CursorGoto;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.gridId = event.grid_id;
      this.setCursor(surface, event.row, event.col);
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("WinViewport" in message) {
      const event = message.WinViewport;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.topline = Number(event.topline ?? 0);
      surface.botline = Number(event.botline ?? 0);
      if (event.textoff) surface.textoff = Number(event.textoff);
      // Older daemons omit curline/curcol (serde default 0 is
      // indistinguishable from line 0 — accept; presence lands at top
      // until the daemon updates).
      surface.curline = Number(event.curline ?? 0);
      surface.curcol = Number(event.curcol ?? 0);
      return;
    }

    if ("HighlightDefined" in message) {
      const event = message.HighlightDefined;
      this.surfaceFor(event.surface_id ?? null).highlights.set(event.hl_id, event.attrs);
      return;
    }

    if ("DefaultColors" in message) {
      const event = message.DefaultColors;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.defaultFg = event.rgb_fg;
      surface.defaultBg = event.rgb_bg;
      surface.defaultSp = event.rgb_sp;
      return;
    }

    if ("PopupMenu" in message) {
      const event = message.PopupMenu;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.popupMenu = {
        items: event.items,
        selected: event.selected ?? null,
        anchor: { row: event.anchor.row, col: event.anchor.col },
        gridId: event.grid_id,
      };
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("PopupMenuSelect" in message) {
      const event = message.PopupMenuSelect;
      const surface = this.surfaceFor(event.surface_id ?? null);
      if (surface.popupMenu) {
        surface.popupMenu = {
          ...surface.popupMenu,
          selected: event.selected ?? null,
        };
      }
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("MouseMode" in message) {
      const event = message.MouseMode;
      this.surfaceFor(event.surface_id ?? null).mouseEnabled = event.enabled;
      return;
    }

    if ("ModeChange" in message) {
      const event = message.ModeChange;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.mode = event.mode;
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("BufferOpened" in message) {
      const event = message.BufferOpened;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.bufferId = event.path;
      this.activeSurfaceKey = surfaceKey(event.surface_id ?? null);
      return;
    }

    if ("Closed" in message) {
      const event = message.Closed;
      const surface = this.surfaceFor(event.surface_id ?? null);
      surface.error = event.reason ?? "Editor closed";
      return;
    }

    if ("Error" in message) {
      const event = message.Error;
      this.surfaceFor(event.surface_id ?? null).error = event.message;
    }
  }

  ingestPresence(message: CrdtServerMessage, localPeerId: string | null = null): void {
    this.localPresencePeerId = localPeerId;
    if ("PresenceSnapshot" in message) {
      const event = message.PresenceSnapshot;
      const peers = new Map<string, CrdtPeerPresence>();
      for (const peer of event.peers) {
        peers.set(peer.peer_id, peer);
      }
      this.presenceByBuffer.set(event.buffer_id, peers);
      return;
    }

    if ("Presence" in message) {
      const update = message.Presence.update;
      if ("Upsert" in update) {
        const presence = update.Upsert;
        const peers = this.presenceByBuffer.get(presence.buffer_id) ?? new Map();
        peers.set(presence.peer_id, presence);
        this.presenceByBuffer.set(presence.buffer_id, peers);
        return;
      }
      if ("Remove" in update) {
        const { buffer_id, peer_id } = update.Remove;
        const peers = this.presenceByBuffer.get(buffer_id);
        peers?.delete(peer_id);
        if (peers && peers.size === 0) {
          this.presenceByBuffer.delete(buffer_id);
        }
      }
    }
  }

  setActiveSurface(surfaceId: string | null): void {
    this.activeSurfaceKey = surfaceKey(surfaceId);
    this.surfaceFor(surfaceId);
  }

  activeSnapshot(): NvimGridSnapshot | null {
    return this.snapshotForKey(this.activeSurfaceKey) ?? this.firstSnapshot();
  }

  snapshotForSurface(surfaceId: string | null): NvimGridSnapshot | null {
    return this.snapshotForKey(surfaceKey(surfaceId));
  }

  surfaceIds(): string[] {
    return Array.from(this.surfaces.values())
      .map((surface) => surface.surfaceId)
      .filter((id): id is string => typeof id === "string" && id.length > 0);
  }

  hasAnySnapshot(): boolean {
    return this.firstSnapshot() !== null;
  }

  private firstSnapshot(): NvimGridSnapshot | null {
    for (const key of this.surfaces.keys()) {
      const snapshot = this.snapshotForKey(key);
      if (snapshot) return snapshot;
    }
    return null;
  }

  private snapshotForKey(key: string): NvimGridSnapshot | null {
    const surface = this.surfaces.get(key);
    if (!surface) return null;
    // Presence channels are keyed by the daemon's canonical buffer id
    // (`file://<abs-path>`); nvim's BufferOpened path may be a bare
    // absolute path, so normalize before the lookup.
    const presenceBufferId = surface.bufferId
      ? presenceBufferIdForPath(surface.bufferId)
      : null;
    const syntheticErrorGrid =
      surface.error && (surface.width <= 0 || surface.height <= 0);
    if (!syntheticErrorGrid && (surface.width <= 0 || surface.height <= 0)) {
      return null;
    }
    const width = syntheticErrorGrid ? 1 : surface.width;
    const height = syntheticErrorGrid ? 1 : surface.height;
    const cells = syntheticErrorGrid ? [this.blankCell(surface)] : surface.cells;
    const cursor =
      !syntheticErrorGrid && surface.cursor && surface.cursor.visible
        ? ([surface.cursor.row, surface.cursor.col] as [number, number])
        : null;
    return {
      surfaceId: surface.surfaceId,
      bufferId: surface.bufferId,
      gridId: surface.gridId,
      topline: surface.topline,
      curline: surface.curline,
      curcol: surface.curcol,
      width,
      height,
      cells: cells.map((cell) => ({ ...cell })),
      cursor,
      cursorState: surface.cursor ? { ...surface.cursor } : null,
      default_fg: surface.defaultFg,
      default_bg: surface.defaultBg,
      default_sp: surface.defaultSp,
      mode: surface.mode,
      mouseEnabled: surface.mouseEnabled,
      popupMenu: surface.popupMenu
        ? {
            items: surface.popupMenu.items.map((item) => ({ ...item })),
            selected: surface.popupMenu.selected,
            anchor: { ...surface.popupMenu.anchor },
            gridId: surface.popupMenu.gridId,
          }
        : null,
      presence: presenceCuesForGrid(
        Array.from(
          this.presenceByBuffer.get(presenceBufferId ?? "")?.values() ?? [],
        )
          // Presence lines are BUFFER coordinates; this grid renders
          // rows relative to win_viewport.topline. Shift, and drop
          // peers scrolled out of this surface's view.
          .filter(
            (peer) =>
              peer.cursor.line >= surface.topline &&
              (surface.botline <= surface.topline ||
                peer.cursor.line < surface.botline),
          )
          .map((peer) => ({
            ...peer,
            cursor: {
              ...peer.cursor,
              line: peer.cursor.line - surface.topline,
              column: peer.cursor.column + surface.textoff,
            },
          })),
        presenceBufferId,
        width,
        height,
        this.localPresencePeerId,
      ),
      error: surface.error,
    };
  }

  private surfaceFor(surfaceId: string | null): SurfaceState {
    const key = surfaceKey(surfaceId);
    const existing = this.surfaces.get(key);
    if (existing) return existing;
    const surface: SurfaceState = {
      surfaceId,
      bufferId: null,
      gridId: 0,
      topline: 0,
      botline: 0,
      textoff: 0,
      curline: null,
      curcol: null,
      width: 0,
      height: 0,
      cells: [],
      cursor: null,
      defaultFg: DEFAULT_FG,
      defaultBg: DEFAULT_BG,
      defaultSp: DEFAULT_SP,
      mode: null,
      mouseEnabled: false,
      popupMenu: null,
      error: null,
      highlights: new Map(),
    };
    this.surfaces.set(key, surface);
    return surface;
  }

  private ensureSize(surface: SurfaceState, gridId: number, width: number, height: number): void {
    if (surface.width === width && surface.height === height && surface.gridId === gridId) {
      return;
    }
    this.resize(surface, gridId, width, height);
  }

  private resize(surface: SurfaceState, gridId: number, width: number, height: number): void {
    surface.gridId = gridId;
    surface.width = Math.max(0, Math.trunc(width));
    surface.height = Math.max(0, Math.trunc(height));
    surface.cells = Array.from(
      { length: surface.width * surface.height },
      () => this.blankCell(surface),
    );
    if (surface.cursor) {
      this.setCursor(surface, surface.cursor.row, surface.cursor.col);
    }
  }

  private putCell(surface: SurfaceState, cell: GridCell): void {
    const row = Math.trunc(cell.row);
    const col = Math.trunc(cell.col);
    if (row < 0 || col < 0 || row >= surface.height || col >= surface.width) return;
    surface.cells[row * surface.width + col] = {
      ch: cell.ch.length > 0 ? cell.ch : " ",
      fg: cell.fg || surface.defaultFg,
      bg: cell.bg,
      attrs: cell.attrs,
    };
  }

  private setCursor(surface: SurfaceState, row: number, col: number): void {
    if (surface.width <= 0 || surface.height <= 0) {
      surface.cursor = {
        row: Math.max(0, Math.trunc(row)),
        col: Math.max(0, Math.trunc(col)),
        visible: true,
      };
      return;
    }
    surface.cursor = {
      row: clampInt(row, 0, surface.height - 1),
      col: clampInt(col, 0, surface.width - 1),
      visible: true,
    };
  }

  private scroll(
    surface: SurfaceState,
    top: number,
    bot: number,
    left: number,
    right: number,
    rows: number,
    cols: number,
  ): void {
    if (surface.width <= 0 || surface.height <= 0) return;
    const t = clampInt(top, 0, surface.height);
    const b = clampInt(bot, t, surface.height);
    const l = clampInt(left, 0, surface.width);
    const r = clampInt(right, l, surface.width);
    if (t >= b || l >= r) return;

    const previous = surface.cells.map((cell) => ({ ...cell }));
    for (let row = t; row < b; row += 1) {
      for (let col = l; col < r; col += 1) {
        const srcRow = row + Math.trunc(rows);
        const srcCol = col + Math.trunc(cols);
        const dstIndex = row * surface.width + col;
        if (srcRow >= t && srcRow < b && srcCol >= l && srcCol < r) {
          surface.cells[dstIndex] = { ...previous[srcRow * surface.width + srcCol] };
        } else {
          surface.cells[dstIndex] = this.blankCell(surface);
        }
      }
    }
  }

  private clear(surface: SurfaceState): void {
    if (surface.width <= 0 || surface.height <= 0) return;
    surface.cells = Array.from(
      { length: surface.width * surface.height },
      () => this.blankCell(surface),
    );
  }

  private blankCell(surface: SurfaceState): NvimGridCellSnapshot {
    return {
      ch: " ",
      fg: surface.defaultFg,
      bg: surface.defaultBg,
      attrs: 0,
    };
  }
}

export function surfaceKey(surfaceId: string | null): string {
  return surfaceId && surfaceId.length > 0 ? surfaceId : PRIMARY_SURFACE_KEY;
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
