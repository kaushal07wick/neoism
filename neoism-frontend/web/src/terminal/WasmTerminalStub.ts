/**
 * Placeholder for the eventual `@neoism/terminal-wasm` export.
 *
 * The real module will ship a `WasmTerminal` class with this same surface:
 * - `new(cols, rows)` constructs a parser/grid pair.
 * - `feed(bytes)` advances the state machine with PTY output bytes.
 * - `resize(cols, rows)` retiles the grid.
 * - `snapshot()` returns a frame-ready view (cells, cursor, dirty regions).
 *
 * Keeping the shape stable here lets `TerminalPanel.ts` swap implementations
 * by changing imports only.
 */

export interface TerminalCursor {
  col: number;
  row: number;
  visible: boolean;
}

export interface TerminalSnapshot {
  cols: number;
  rows: number;
  cursor: TerminalCursor;
  bytesIngested: number;
  lastBytePreview: string;
}

export class WasmTerminalStub {
  private bytesIngested = 0;
  private lastBytePreview = "";
  private cursor: TerminalCursor = { col: 0, row: 0, visible: true };

  constructor(
    private cols: number,
    private rows: number,
  ) {}

  feed(bytes: Uint8Array): void {
    this.bytesIngested += bytes.byteLength;
    const tail = bytes.subarray(Math.max(0, bytes.byteLength - 16));
    this.lastBytePreview = Array.from(tail)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ");
    // Stub cursor advance: every byte moves the column one cell, wrapping.
    for (let i = 0; i < bytes.byteLength; i++) {
      this.cursor.col += 1;
      if (this.cursor.col >= this.cols) {
        this.cursor.col = 0;
        this.cursor.row = Math.min(this.rows - 1, this.cursor.row + 1);
      }
    }
  }

  resize(cols: number, rows: number): void {
    this.cols = Math.max(1, cols);
    this.rows = Math.max(1, rows);
    this.cursor.col = Math.min(this.cursor.col, this.cols - 1);
    this.cursor.row = Math.min(this.cursor.row, this.rows - 1);
  }

  snapshot(): TerminalSnapshot {
    return {
      cols: this.cols,
      rows: this.rows,
      cursor: { ...this.cursor },
      bytesIngested: this.bytesIngested,
      lastBytePreview: this.lastBytePreview,
    };
  }
}
