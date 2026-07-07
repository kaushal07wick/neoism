// DOM event -> `neoism-ui::UiEvent` JSON.
//
// The wasm chrome runs `serde_json::from_str::<UiEvent>(...)` against
// these payloads, so every shape here must match the serde
// representation of the Rust types in `neoism-ui/src/event.rs`.
//
// Encoding notes (externally-tagged enums by default):
//   * Unit variants serialize as bare strings:
//       `UiEvent::PointerLeave`        -> "PointerLeave"
//       `KeyState::Pressed`            -> "Pressed"
//       `NamedKey::Enter`              -> "Enter"
//       `WheelMode::Pixel`             -> "Pixel"
//       `PointerButton::Left`          -> "Left"
//       `LogicalKey::Unidentified`     -> "Unidentified"
//       `CompositionEvent::Start`/`End`-> "Start"/"End"
//   * Newtype variants wrap their inner value:
//       `NamedKey::Function(3)`        -> {"Function": 3}
//       `PointerButton::Other(5)`      -> {"Other": 5}
//       `LogicalKey::Character("a")`   -> {"Character": "a"}
//       `LogicalKey::Named(...)`       -> {"Named": <NamedKey>}
//       `UiEvent::Text("hi")`          -> {"Text": "hi"}
//       `UiEvent::Composition(...)`    -> {"Composition": <CompositionEvent>}
//       `UiEvent::Focus(true)`         -> {"Focus": true}
//       `UiEvent::Key(...)`            -> {"Key": <KeyDescriptor>}
//   * Struct variants emit a struct-shaped payload:
//       `UiEvent::PointerMove { ... }` -> {"PointerMove": {"x":..., ...}}
//   * Tuple structs with one field serialize as the inner value:
//       `PhysicalKey(u32)`             -> 42  (plain integer)
//   * `bitflags::bitflags!` with `serde` (no `#[serde(transparent)]`)
//     serializes as a human-readable string with `|`-joined names,
//     e.g. `Modifiers::SHIFT | Modifiers::CTRL` -> "SHIFT | CTRL".
//     Empty flags serialize as the empty string "".

// ------------------------------------------------------------------
// Modifier helpers
// ------------------------------------------------------------------

/** Bitset constants matching `neoism-ui::event::Modifiers` (u8). */
export const ModifierBits = {
  SHIFT: 1 << 0,
  CTRL: 1 << 1,
  ALT: 1 << 2,
  META: 1 << 3,
} as const;

export type ModifierBitset = number;

/** Pack a DOM event's modifier state into a u8 bitset. */
export function packModifiers(e: {
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): ModifierBitset {
  let bits = 0;
  if (e.shiftKey) bits |= ModifierBits.SHIFT;
  if (e.ctrlKey) bits |= ModifierBits.CTRL;
  if (e.altKey) bits |= ModifierBits.ALT;
  if (e.metaKey) bits |= ModifierBits.META;
  return bits;
}

/**
 * Render a modifier bitset as the `|`-separated string form that
 * `bitflags` produces in human-readable serde formats. Empty flags
 * serialize as the empty string.
 */
export function modifiersToWire(bits: ModifierBitset): string {
  const parts: string[] = [];
  if (bits & ModifierBits.SHIFT) parts.push("SHIFT");
  if (bits & ModifierBits.CTRL) parts.push("CTRL");
  if (bits & ModifierBits.ALT) parts.push("ALT");
  if (bits & ModifierBits.META) parts.push("META");
  return parts.join(" | ");
}

// ------------------------------------------------------------------
// UiEvent JSON shapes
// ------------------------------------------------------------------

/** Modifiers as the bitflags-serialized string. */
export type ModifiersJson = string;

export type NamedKeyJson =
  | "Enter"
  | "Tab"
  | "Escape"
  | "Backspace"
  | "ArrowUp"
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight"
  | "Home"
  | "End"
  | "PageUp"
  | "PageDown"
  | "Delete"
  | "Insert"
  | "Space"
  | { Function: number };

export type LogicalKeyJson =
  | { Named: NamedKeyJson }
  | { Character: string }
  | "Unidentified";

export type KeyStateJson = "Pressed" | "Released";

/** `PhysicalKey(u32)` is a one-field tuple struct -> serializes as u32. */
export type PhysicalKeyJson = number;

export interface KeyDescriptorJson {
  physical: PhysicalKeyJson;
  logical: LogicalKeyJson;
  state: KeyStateJson;
  modifiers: ModifiersJson;
  repeat: boolean;
}

export type PointerButtonJson =
  | "Left"
  | "Right"
  | "Middle"
  | "Back"
  | "Forward"
  | { Other: number };

export type WheelModeJson = "Pixel" | "Line" | "Page";

export type CompositionEventJson =
  | "Start"
  | { Update: { preedit: string; cursor: number } }
  | { Commit: string }
  | "End";

export type UiEventJson =
  | { Key: KeyDescriptorJson }
  | { Text: string }
  | { Composition: CompositionEventJson }
  | {
      PointerMove: { x: number; y: number; modifiers: ModifiersJson };
    }
  | {
      PointerDown: {
        button: PointerButtonJson;
        x: number;
        y: number;
        modifiers: ModifiersJson;
        click_count: number;
      };
    }
  | {
      PointerUp: {
        button: PointerButtonJson;
        x: number;
        y: number;
        modifiers: ModifiersJson;
      };
    }
  | "PointerLeave"
  | {
      Wheel: {
        dx: number;
        dy: number;
        mode: WheelModeJson;
        modifiers: ModifiersJson;
      };
    }
  | { Focus: boolean }
  | { Resize: { w: number; h: number; scale: number } };

// ------------------------------------------------------------------
// Key translation
// ------------------------------------------------------------------

/**
 * Map `KeyboardEvent.key` to `NamedKey` when it names a non-character
 * key the chrome cares about. Returns `null` for printable characters
 * (the caller emits `LogicalKey::Character` for those).
 */
function namedKeyFor(key: string): NamedKeyJson | null {
  switch (key) {
    case "Enter":
      return "Enter";
    case "Tab":
      return "Tab";
    case "Escape":
    case "Esc":
      return "Escape";
    case "Backspace":
      return "Backspace";
    case "ArrowUp":
    case "Up":
      return "ArrowUp";
    case "ArrowDown":
    case "Down":
      return "ArrowDown";
    case "ArrowLeft":
    case "Left":
      return "ArrowLeft";
    case "ArrowRight":
    case "Right":
      return "ArrowRight";
    case "Home":
      return "Home";
    case "End":
      return "End";
    case "PageUp":
      return "PageUp";
    case "PageDown":
      return "PageDown";
    case "Delete":
    case "Del":
      return "Delete";
    case "Insert":
      return "Insert";
    case " ":
    case "Space":
      return "Space";
    default:
      break;
  }
  // F1..=F24
  if (key.length >= 2 && key[0] === "F") {
    const n = Number(key.slice(1));
    if (Number.isInteger(n) && n >= 1 && n <= 24) {
      return { Function: n };
    }
  }
  return null;
}

export function fromKeyPressEvent(args: {
  key: string;
  code?: string;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
  repeat?: boolean;
}): UiEventJson {
  const modifiers = packModifiers({
    shiftKey: args.shiftKey === true,
    ctrlKey: args.ctrlKey === true,
    altKey: args.altKey === true,
    metaKey: args.metaKey === true,
  });
  const named = namedKeyFor(args.key);
  return {
    Key: {
      physical: hashPhysicalKey(args.code ?? args.key),
      logical: named === null ? { Character: args.key } : { Named: named },
      state: "Pressed",
      modifiers: modifiersToWire(modifiers),
      repeat: args.repeat === true,
    },
  };
}

/**
 * Translate `KeyboardEvent.code` into the `PhysicalKey(u32)` payload.
 * Web hosts hash the code string into a u32 — we use FNV-1a so the
 * value is deterministic and stable across reloads.
 */
export function hashPhysicalKey(code: string): number {
  let hash = 0x811c9dc5; // FNV offset basis
  for (let i = 0; i < code.length; i += 1) {
    hash ^= code.charCodeAt(i);
    // FNV prime 0x01000193, kept inside u32 range
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

function logicalKeyFor(e: KeyboardEvent): LogicalKeyJson {
  const named = namedKeyFor(e.key);
  if (named !== null) {
    return { Named: named };
  }
  // Single character / grapheme — emit as `Character(SmolStr)`.
  if (e.key.length > 0 && e.key !== "Dead" && e.key !== "Unidentified") {
    return { Character: e.key };
  }
  return "Unidentified";
}

export function fromKeyboardEvent(e: KeyboardEvent): UiEventJson {
  const descriptor: KeyDescriptorJson = {
    physical: hashPhysicalKey(e.code),
    logical: logicalKeyFor(e),
    state: e.type === "keyup" ? "Released" : "Pressed",
    modifiers: modifiersToWire(packModifiers(e)),
    repeat: e.repeat,
  };
  return { Key: descriptor };
}

// ------------------------------------------------------------------
// Pointer translation
// ------------------------------------------------------------------

function pointerButtonFor(button: number): PointerButtonJson {
  switch (button) {
    case 0:
      return "Left";
    case 1:
      return "Middle";
    case 2:
      return "Right";
    case 3:
      return "Back";
    case 4:
      return "Forward";
    default:
      return { Other: Math.max(0, Math.min(0xffff, button)) };
  }
}

export interface PointerCoords {
  x: number;
  y: number;
}

/**
 * Resolve a pointer's coordinates relative to a target element if
 * provided. The chrome wants client coordinates inside the canvas
 * surface, not viewport-relative ones.
 */
export function pointerCoords(
  e: MouseEvent | PointerEvent,
  target?: Element | null,
): PointerCoords {
  if (target) {
    const rect = target.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }
  return { x: e.clientX, y: e.clientY };
}

export function fromPointerMoveEvent(
  e: PointerEvent | MouseEvent,
  target?: Element | null,
): UiEventJson {
  const { x, y } = pointerCoords(e, target);
  return {
    PointerMove: {
      x,
      y,
      modifiers: modifiersToWire(packModifiers(e)),
    },
  };
}

export function fromPointerDownEvent(
  e: PointerEvent | MouseEvent,
  clickCount: number,
  target?: Element | null,
): UiEventJson {
  const { x, y } = pointerCoords(e, target);
  return {
    PointerDown: {
      button: pointerButtonFor(e.button),
      x,
      y,
      modifiers: modifiersToWire(packModifiers(e)),
      click_count: Math.max(1, Math.min(255, Math.trunc(clickCount))),
    },
  };
}

export function fromPointerUpEvent(
  e: PointerEvent | MouseEvent,
  target?: Element | null,
): UiEventJson {
  const { x, y } = pointerCoords(e, target);
  return {
    PointerUp: {
      button: pointerButtonFor(e.button),
      x,
      y,
      modifiers: modifiersToWire(packModifiers(e)),
    },
  };
}

export function pointerLeaveEvent(): UiEventJson {
  return "PointerLeave";
}

// ------------------------------------------------------------------
// Wheel translation
// ------------------------------------------------------------------

function wheelModeFor(deltaMode: number): WheelModeJson {
  // DOM constants: 0 = pixel, 1 = line, 2 = page
  switch (deltaMode) {
    case 1:
      return "Line";
    case 2:
      return "Page";
    case 0:
    default:
      return "Pixel";
  }
}

export interface WheelEventOptions {
  invertX?: boolean;
}

export function fromWheelEvent(
  e: WheelEvent,
  options: WheelEventOptions = {},
): UiEventJson {
  return {
    Wheel: {
      dx: options.invertX ? -e.deltaX : e.deltaX,
      dy: e.deltaY,
      mode: wheelModeFor(e.deltaMode),
      modifiers: modifiersToWire(packModifiers(e)),
    },
  };
}

// ------------------------------------------------------------------
// Composition / text / focus / resize
// ------------------------------------------------------------------

export function fromTextEvent(text: string): UiEventJson {
  return { Text: text };
}

export function fromCompositionStart(): UiEventJson {
  return { Composition: "Start" };
}

export function fromCompositionUpdate(
  preedit: string,
  cursor: number,
): UiEventJson {
  return {
    Composition: {
      Update: {
        preedit,
        cursor: Math.max(0, Math.trunc(cursor)),
      },
    },
  };
}

export function fromCompositionCommit(text: string): UiEventJson {
  return { Composition: { Commit: text } };
}

export function fromCompositionEnd(): UiEventJson {
  return { Composition: "End" };
}

export function fromFocusEvent(focused: boolean): UiEventJson {
  return { Focus: focused };
}

export function fromResizeEvent(args: {
  w: number;
  h: number;
  scale: number;
}): UiEventJson {
  return {
    Resize: {
      w: Math.max(0, Math.trunc(args.w)),
      h: Math.max(0, Math.trunc(args.h)),
      scale: args.scale,
    },
  };
}
