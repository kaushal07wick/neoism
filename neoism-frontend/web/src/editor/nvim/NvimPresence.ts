import type { CrdtPeerPresence, CrdtPresenceColor } from "../../workspace/types";

export interface NvimPresencePoint {
  row: number;
  col: number;
}

export interface NvimPresenceSelection {
  anchor: NvimPresencePoint;
  head: NvimPresencePoint;
  start: NvimPresencePoint;
  end: NvimPresencePoint;
}

export interface NvimPresenceCue {
  peerId: string;
  label: string;
  color: CrdtPresenceColor;
  colorCss: string;
  cursor: NvimPresencePoint;
  selection: NvimPresenceSelection | null;
}

export interface NvimPresenceSegment {
  row: number;
  startCol: number;
  endCol: number;
}

const PRESENCE_PALETTE: CrdtPresenceColor[] = [
  { r: 0x2f, g: 0x80, b: 0xed },
  { r: 0x27, g: 0xae, b: 0x60 },
  { r: 0xeb, g: 0x57, b: 0x57 },
  { r: 0xf2, g: 0xc9, b: 0x4c },
  { r: 0xbb, g: 0x6b, b: 0xd9 },
  { r: 0x56, g: 0xcc, b: 0xf2 },
  { r: 0xf2, g: 0x99, b: 0x4a },
  { r: 0x21, g: 0x92, b: 0x6b },
  { r: 0x9b, g: 0x51, b: 0xe0 },
  { r: 0x00, g: 0xac, b: 0xd7 },
  { r: 0xd6, g: 0x5d, b: 0x0e },
  { r: 0x6f, g: 0x7d, b: 0xff },
];

export function stablePresenceColor(peerId: string): CrdtPresenceColor {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(peerId)) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x1000000001b3n);
  }
  return PRESENCE_PALETTE[Number(hash % BigInt(PRESENCE_PALETTE.length))];
}

export function colorCss(color: CrdtPresenceColor): string {
  return `rgb(${clampByte(color.r)}, ${clampByte(color.g)}, ${clampByte(color.b)})`;
}

export function presenceCueForGrid(
  presence: CrdtPeerPresence,
  width: number,
  height: number,
): NvimPresenceCue {
  const cursor = clampPoint(
    { row: presence.cursor.line, col: presence.cursor.column },
    width,
    height,
  );
  const selection = presence.selection
    ? selectionCue(presence.selection.anchor, presence.selection.head, width, height)
    : null;
  return {
    peerId: presence.peer_id,
    label: presenceLabel(presence),
    color: presence.color,
    colorCss: colorCss(presence.color),
    cursor,
    selection,
  };
}

export function presenceCuesForGrid(
  peers: CrdtPeerPresence[],
  bufferId: string | null,
  width: number,
  height: number,
  localPeerId: string | null = null,
): NvimPresenceCue[] {
  if (!bufferId) return [];
  return peers
    .filter((presence) => presence.buffer_id === bufferId)
    .filter((presence) => presence.peer_id !== localPeerId)
    .map((presence) => presenceCueForGrid(presence, width, height));
}

export function selectionSegments(
  selection: NvimPresenceSelection,
  width: number,
): NvimPresenceSegment[] {
  const maxWidth = Math.max(1, Math.trunc(width));
  const segments: NvimPresenceSegment[] = [];
  for (let row = selection.start.row; row <= selection.end.row; row += 1) {
    const startCol = row === selection.start.row ? selection.start.col : 0;
    const endCol = row === selection.end.row ? selection.end.col : maxWidth - 1;
    if (endCol < startCol) continue;
    segments.push({ row, startCol, endCol });
  }
  return segments;
}

function selectionCue(
  anchorRaw: { line: number; column: number },
  headRaw: { line: number; column: number },
  width: number,
  height: number,
): NvimPresenceSelection | null {
  const anchor = clampPoint({ row: anchorRaw.line, col: anchorRaw.column }, width, height);
  const head = clampPoint({ row: headRaw.line, col: headRaw.column }, width, height);
  if (anchor.row === head.row && anchor.col === head.col) return null;
  const [start, end] =
    anchor.row < head.row || (anchor.row === head.row && anchor.col <= head.col)
      ? [anchor, head]
      : [head, anchor];
  return { anchor, head, start, end };
}

function presenceLabel(presence: CrdtPeerPresence): string {
  const trimmed = presence.display_name.trim();
  return (trimmed.length > 0 ? trimmed : presence.peer_id).slice(0, 32);
}

function clampPoint(point: NvimPresencePoint, width: number, height: number): NvimPresencePoint {
  return {
    row: clampInt(point.row, 0, Math.max(0, Math.trunc(height) - 1)),
    col: clampInt(point.col, 0, Math.max(0, Math.trunc(width) - 1)),
  };
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

function clampByte(value: number): number {
  return clampInt(value, 0, 255);
}
