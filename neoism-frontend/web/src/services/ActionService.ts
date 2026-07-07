// JS-side bridge for `neoism_ui::action_policy`.
//
// The shared `action_policy` module owns the canonical keyboard /
// mouse binding tables (`Action`, `MouseAction`, `SearchAction`,
// `ViAction`, `Binding`, `BindingMode`, `Program`, defaults). It's
// exposed to the wasm bridge via three top-level wasm-bindgen exports
// (`default_key_bindings_json`, `default_mouse_bindings_json`,
// `lookup_action` / `lookup_mouse_action`).
//
// This service wraps those exports so the rest of the web frontend can
// route palette / keyboard shortcuts through shared policy first and
// only fall back to the hand-rolled TS table when no shared binding
// matches. That way the bytecode in `bindings/defaults.rs` is the
// single source of truth for both desktop and web.

/** Mods bag accepted by `lookup_action`. Extra keys are ignored. */
export interface ActionMods {
  shift?: boolean;
  control?: boolean;
  alt?: boolean;
  super?: boolean;
}

/** Mode bag accepted by `lookup_action`. Maps to `BindingMode` bits.
 *  Omitted keys default to `false`. Most web call sites only need to
 *  set `vi` and `search` — terminal mode bits (`app_cursor`,
 *  `app_keypad`, `alt_screen`, `disambiguate_keys`, `all_keys_as_esc`)
 *  are populated from the active `Terminal::mode()` snapshot. */
export interface ActionMode {
  app_cursor?: boolean;
  app_keypad?: boolean;
  alt_screen?: boolean;
  vi?: boolean;
  search?: boolean;
  disambiguate_keys?: boolean;
  all_keys_as_esc?: boolean;
}

/** Discriminated union mirror of `action_policy::Action<()>`. The
 *  `kind` field is `snake_case`; variant-carrying types ship their
 *  payload alongside. Mirrors the JSON shape emitted by the wasm
 *  bridge's `ActionDto`. */
export type Action =
  | { kind: "esc"; sequence: string }
  | { kind: "run"; program: string; args: string[] }
  | { kind: "scroll"; delta: number }
  | { kind: "hint" }
  | { kind: "vi_motion"; motion: string }
  | { kind: "vi"; action: string }
  | { kind: "mouse"; action: string }
  | { kind: "paste" }
  | { kind: "copy" }
  | { kind: "copy_selection" }
  | { kind: "paste_selection" }
  | { kind: "increase_font_size" }
  | { kind: "decrease_font_size" }
  | { kind: "reset_font_size" }
  | { kind: "scroll_page_up" }
  | { kind: "scroll_page_down" }
  | { kind: "scroll_half_page_up" }
  | { kind: "scroll_half_page_down" }
  | { kind: "scroll_to_top" }
  | { kind: "scroll_to_bottom" }
  | { kind: "clear_history" }
  | { kind: "hide" }
  | { kind: "hide_other_applications" }
  | { kind: "minimize" }
  | { kind: "quit" }
  | { kind: "clear_log_notice" }
  | { kind: "spawn_new_instance" }
  | { kind: "window_create_new" }
  | { kind: "config_editor" }
  | { kind: "tab_create_new" }
  | { kind: "workspace_terminal_tab_create_new" }
  | { kind: "move_current_tab_to_prev" }
  | { kind: "move_current_tab_to_next" }
  | { kind: "move_active_buffer_tab_to_prev" }
  | { kind: "move_active_buffer_tab_to_next" }
  | { kind: "select_next_tab" }
  | { kind: "select_prev_tab" }
  | { kind: "select_next_buffer_tab" }
  | { kind: "select_prev_buffer_tab" }
  | { kind: "tab_close_current" }
  | { kind: "close_current_split_or_tab" }
  | { kind: "tab_close_unfocused" }
  | { kind: "toggle_fullscreen" }
  | { kind: "toggle_maximized" }
  | { kind: "toggle_simple_fullscreen" }
  | { kind: "clear_selection" }
  | { kind: "toggle_vi_mode" }
  | { kind: "toggle_appearance_theme" }
  | { kind: "select_tab"; index: number }
  | { kind: "select_last_tab" }
  | { kind: "search"; action: string }
  | { kind: "search_forward" }
  | { kind: "search_backward" }
  | { kind: "split_right" }
  | { kind: "split_down" }
  | { kind: "select_next_split" }
  | { kind: "select_prev_split" }
  | { kind: "select_next_split_or_tab" }
  | { kind: "select_prev_split_or_tab" }
  | { kind: "move_divider_up" }
  | { kind: "move_divider_down" }
  | { kind: "move_divider_left" }
  | { kind: "move_divider_right" }
  | { kind: "open_command_palette" }
  | { kind: "toggle_file_tree" }
  | { kind: "toggle_git_diff_panel" }
  | { kind: "receive_char" }
  | { kind: "none" };

/** Trigger shape on a keyboard binding row. */
export type BindingTrigger =
  | { kind: "character"; value: string }
  | { kind: "named"; name: string };

export interface KeyBinding {
  trigger: BindingTrigger;
  mods: Required<ActionMods>;
  mode: Required<ActionMode>;
  notmode: Required<ActionMode>;
  action: Action;
}

export interface MouseBinding {
  button: "left" | "middle" | "right" | "back" | "forward" | "other";
  mods: Required<ActionMods>;
  mode: Required<ActionMode>;
  notmode: Required<ActionMode>;
  action: Action;
}

/** Minimal facet of the wasm module the service depends on. The full
 *  module surface is exported by `neoism-terminal-wasm`; we only need
 *  the four action-policy entry points here. */
export interface WasmActionModule {
  default_key_bindings_json?(): unknown;
  default_mouse_bindings_json?(): unknown;
  parse_action_name?(name: string): unknown;
  lookup_action?(key: string, mods: ActionMods, mode: ActionMode): unknown;
  lookup_mouse_action?(
    button: string,
    mods: ActionMods,
    mode: ActionMode,
  ): unknown;
}

/** Bridge for `neoism_ui::action_policy`. Lazy: when the wasm module
 *  is unavailable (stub adapter, bundle not built yet), every method
 *  returns `null` / `[]` so callers can fall back to their own
 *  hand-rolled tables. */
export class ActionService {
  private cachedKeyBindings: KeyBinding[] | null = null;
  private cachedMouseBindings: MouseBinding[] | null = null;

  constructor(private readonly wasm: WasmActionModule | null) {}

  /** Default keyboard binding table from shared policy. Returns `[]`
   *  when the wasm module is unavailable. */
  defaultKeyBindings(): KeyBinding[] {
    if (this.cachedKeyBindings) return this.cachedKeyBindings;
    if (!this.wasm?.default_key_bindings_json) return [];
    const raw = this.wasm.default_key_bindings_json();
    const bindings = Array.isArray(raw)
      ? raw.filter(isKeyBinding)
      : [];
    this.cachedKeyBindings = bindings;
    return bindings;
  }

  /** Default mouse binding table from shared policy. */
  defaultMouseBindings(): MouseBinding[] {
    if (this.cachedMouseBindings) return this.cachedMouseBindings;
    if (!this.wasm?.default_mouse_bindings_json) return [];
    const raw = this.wasm.default_mouse_bindings_json();
    const bindings = Array.isArray(raw)
      ? raw.filter(isMouseBinding)
      : [];
    this.cachedMouseBindings = bindings;
    return bindings;
  }

  /** Parse a config-style action name (`"opencommandpalette"`,
   *  `"scroll(1)"`, etc.). Returns `{ kind: "none" }` when unknown or
   *  when the wasm module is unavailable. */
  parseAction(name: string): Action {
    if (!this.wasm?.parse_action_name) return { kind: "none" };
    const raw = this.wasm.parse_action_name(name);
    return isAction(raw) ? raw : { kind: "none" };
  }

  /** Look up the action bound to a key chord. Returns `null` when no
   *  shared binding matches (so the caller can fall through to its
   *  own table) or when the wasm module is unavailable. */
  lookupAction(key: string, mods: ActionMods, mode: ActionMode): Action | null {
    if (!this.wasm?.lookup_action) return null;
    const raw = this.wasm.lookup_action(key, mods, mode);
    return isAction(raw) ? raw : null;
  }

  /** Look up the action bound to a mouse chord. */
  lookupMouseAction(
    button: MouseBinding["button"],
    mods: ActionMods,
    mode: ActionMode,
  ): Action | null {
    if (!this.wasm?.lookup_mouse_action) return null;
    const raw = this.wasm.lookup_mouse_action(button, mods, mode);
    return isAction(raw) ? raw : null;
  }

  /** True when the underlying wasm module exposes the action-policy
   *  surface. Useful for `if (actionService.isAvailable()) { … }`
   *  fast-paths that want to skip a `lookupAction` call when they
   *  know it will return `null`. */
  isAvailable(): boolean {
    return Boolean(
      this.wasm?.lookup_action && this.wasm?.default_key_bindings_json,
    );
  }
}

/** Convert a DOM `KeyboardEvent` into the config-form key name expected
 *  by `lookup_action`. Returns `null` when the event doesn't carry a
 *  key the shared policy table can match (e.g. dead keys, IME
 *  composition, modifier-only presses).
 *
 *  Examples:
 *  - `key === "p"`        → `"p"`
 *  - `key === "P"`        → `"p"` (case-folded so the lookup matches
 *                           the cross-platform table's lowercase keys)
 *  - `key === "Escape"`   → `"escape"`
 *  - `key === "PageUp"`   → `"pageup"`
 *  - `key === "ArrowUp"`  → `"up"`
 *  - `key === "Enter"`    → `"enter"`
 *  - `code === "Equal"`   → `"="` for the `=` / `+` zoom binding
 */
export function configKeyNameFromEvent(event: KeyboardEvent): string | null {
  const k = event.key;
  if (!k || k === "Unidentified" || k === "Dead") return null;
  // Single printable character — case-fold to match the table's
  // lowercase entries.
  if (k.length === 1) {
    return k.toLowerCase();
  }
  // Map browser key names onto the shared config vocabulary.
  switch (k) {
    case "Escape":
      return "escape";
    case "Enter":
      return "enter";
    case "Tab":
      return "tab";
    case "Backspace":
      return "backspace";
    case "Delete":
      return "delete";
    case "Insert":
      return "insert";
    case "Home":
      return "home";
    case "End":
      return "end";
    case "PageUp":
      return "pageup";
    case "PageDown":
      return "pagedown";
    case "ArrowUp":
      return "up";
    case "ArrowDown":
      return "down";
    case "ArrowLeft":
      return "left";
    case "ArrowRight":
      return "right";
    case " ":
      return "space";
    case "F1":
    case "F2":
    case "F3":
    case "F4":
    case "F5":
    case "F6":
    case "F7":
    case "F8":
    case "F9":
    case "F10":
    case "F11":
    case "F12":
      return k.toLowerCase();
    default:
      return null;
  }
}

/** Project a DOM `KeyboardEvent` onto an `ActionMods` bag. */
export function modsFromEvent(event: KeyboardEvent): ActionMods {
  return {
    shift: event.shiftKey,
    control: event.ctrlKey,
    alt: event.altKey,
    super: event.metaKey,
  };
}

function isAction(value: unknown): value is Action {
  if (!value || typeof value !== "object") return false;
  const rec = value as Record<string, unknown>;
  return typeof rec.kind === "string";
}

function isKeyBinding(value: unknown): value is KeyBinding {
  if (!value || typeof value !== "object") return false;
  const rec = value as Record<string, unknown>;
  return (
    isAction(rec.action) &&
    typeof rec.trigger === "object" &&
    rec.trigger !== null &&
    typeof (rec.trigger as Record<string, unknown>).kind === "string"
  );
}

function isMouseBinding(value: unknown): value is MouseBinding {
  if (!value || typeof value !== "object") return false;
  const rec = value as Record<string, unknown>;
  return typeof rec.button === "string" && isAction(rec.action);
}
