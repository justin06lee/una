/**
 * A hotkey binding as the keys printed on the keyboard: on a Mac "fn" with
 * its 🌐, "⌘ command", "space"; elsewhere a PC's "ctrl", "super", "alt".
 * Lowercase like the keycaps themselves. Native bindings carry the
 * platform's own keycodes (macOS virtual keycodes, X11 keycodes on Linux).
 */

export const IS_MAC = navigator.userAgent.includes("Mac");

export interface Cap {
  /** Single centred legend for ordinary keys ("A", "F5", "space"). */
  label?: string;
  /** Modifier keys carry two legends: this one top right ("⌘", "fn")… */
  symbol?: string;
  /** …and a word bottom left ("command"); fn has the globe there instead. */
  word?: string;
  globe?: boolean;
  /** Which of a left/right pair the binding is tied to. */
  side?: "left" | "right";
  wide?: boolean;
}

const FN: Cap = { symbol: "fn", globe: true };
const CONTROL: Cap = { symbol: "⌃", word: "control" };
const OPTION: Cap = { symbol: "⌥", word: "option" };
const COMMAND: Cap = { symbol: "⌘", word: "command" };
const SHIFT: Cap = { symbol: "⇧", word: "shift" };

const PC_CTRL: Cap = { label: "ctrl" };
const PC_ALT: Cap = { label: "alt" };
const PC_SUPER: Cap = { label: "super" };
const PC_SHIFT: Cap = { label: "shift" };

/** macOS virtual keycodes with a legend of their own. */
const MAC_BY_KEYCODE: Record<number, Cap> = {
  63: FN,
  179: FN, // the 🌐 half of the same key
  54: { ...COMMAND, side: "right" },
  55: { ...COMMAND, side: "left" },
  56: { ...SHIFT, side: "left" },
  60: { ...SHIFT, side: "right" },
  58: { ...OPTION, side: "left" },
  61: { ...OPTION, side: "right" },
  59: { ...CONTROL, side: "left" },
  62: { ...CONTROL, side: "right" },
  57: { label: "caps lock", wide: true },
  49: { label: "space", wide: true },
  36: { label: "return", wide: true },
  48: { label: "tab" },
  51: { label: "delete" },
  117: { label: "⌦" },
  53: { label: "esc" },
  123: { label: "←" },
  124: { label: "→" },
  125: { label: "↓" },
  126: { label: "↑" },
};

/** X11 keycodes (evdev + 8) with a legend of their own. */
const X11_BY_KEYCODE: Record<number, Cap> = {
  37: { ...PC_CTRL, side: "left" },
  105: { ...PC_CTRL, side: "right" },
  64: { ...PC_ALT, side: "left" },
  108: { ...PC_ALT, side: "right" },
  133: { ...PC_SUPER, side: "left" },
  134: { ...PC_SUPER, side: "right" },
  50: { ...PC_SHIFT, side: "left" },
  62: { ...PC_SHIFT, side: "right" },
  66: { label: "caps lock", wide: true },
  135: { label: "menu" },
  65: { label: "space", wide: true },
  36: { label: "enter", wide: true },
  23: { label: "tab" },
  22: { label: "backspace" },
  119: { label: "delete" },
  9: { label: "esc" },
  113: { label: "←" },
  114: { label: "→" },
  116: { label: "↓" },
  111: { label: "↑" },
};

const BY_KEYCODE = IS_MAC ? MAC_BY_KEYCODE : X11_BY_KEYCODE;

/** Combo tokens (the global-shortcut plugin's names). */
const MAC_BY_TOKEN: Record<string, Cap> = {
  Ctrl: CONTROL,
  Control: CONTROL,
  Alt: OPTION,
  Option: OPTION,
  Shift: SHIFT,
  Super: COMMAND,
  Cmd: COMMAND,
  Command: COMMAND,
  Meta: COMMAND,
  CmdOrCtrl: COMMAND,
  Space: { label: "space", wide: true },
  Enter: { label: "return", wide: true },
  Tab: { label: "tab" },
  Backspace: { label: "delete" },
  Delete: { label: "⌦" },
  Escape: { label: "esc" },
  ArrowLeft: { label: "←" },
  ArrowRight: { label: "→" },
  ArrowDown: { label: "↓" },
  ArrowUp: { label: "↑" },
};

const PC_BY_TOKEN: Record<string, Cap> = {
  ...MAC_BY_TOKEN,
  Ctrl: PC_CTRL,
  Control: PC_CTRL,
  Alt: PC_ALT,
  Option: PC_ALT,
  Shift: PC_SHIFT,
  Super: PC_SUPER,
  Cmd: PC_SUPER,
  Command: PC_SUPER,
  Meta: PC_SUPER,
  CmdOrCtrl: PC_CTRL,
  Enter: { label: "enter", wide: true },
  Backspace: { label: "backspace" },
  Delete: { label: "delete" },
};

const BY_TOKEN = IS_MAC ? MAC_BY_TOKEN : PC_BY_TOKEN;

type RowKey =
  | { keycode: number; name: string; cap: Cap; grow: number }
  | { space: true; grow: number };

/**
 * The bottom row of the keyboard, left to right, as one-click hotkeys: left
 * and right keys are separate, so either side can be bound while its twin
 * stays an ordinary modifier (⌘ and ⌥ on a Mac, Ctrl, Super and Alt on a
 * PC). Names are what the recorder stores for the same key; `grow` keeps a
 * real keyboard's proportions.
 */
const MAC_BOTTOM_ROW: RowKey[] = [
  { keycode: 63, name: "Fn", cap: FN, grow: 1 },
  { keycode: 59, name: "Left ⌃", cap: CONTROL, grow: 1.3 },
  { keycode: 58, name: "Left ⌥", cap: OPTION, grow: 1.3 },
  { keycode: 55, name: "Left ⌘", cap: COMMAND, grow: 1.55 },
  { space: true, grow: 2.2 },
  { keycode: 54, name: "Right ⌘", cap: COMMAND, grow: 1.55 },
  { keycode: 61, name: "Right ⌥", cap: OPTION, grow: 1.3 },
];

const PC_BOTTOM_ROW: RowKey[] = [
  { keycode: 37, name: "Left Ctrl", cap: PC_CTRL, grow: 1.3 },
  { keycode: 133, name: "Left Super", cap: PC_SUPER, grow: 1.2 },
  { keycode: 64, name: "Left Alt", cap: PC_ALT, grow: 1.2 },
  { space: true, grow: 2.6 },
  { keycode: 108, name: "Right Alt", cap: PC_ALT, grow: 1.2 },
  { keycode: 105, name: "Right Ctrl", cap: PC_CTRL, grow: 1.3 },
];

export const BOTTOM_ROW = IS_MAC ? MAC_BOTTOM_ROW : PC_BOTTOM_ROW;

/** The modifier keycodes: held alone, they don't reach other apps' typing. */
export const MODIFIER_KEYCODES = IS_MAC
  ? [54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 179]
  : [37, 105, 64, 108, 133, 134, 50, 62, 66, 92];
export const FN_KEYCODES = IS_MAC ? [63, 179] : [];

/** "native:<keycode>:<Name>" → its parts, or null for a combo. */
export function parseNative(b: string): { keycode: number; name: string } | null {
  if (!b.startsWith("native:")) return null;
  const rest = b.slice("native:".length);
  const i = rest.indexOf(":");
  const code = Number(i >= 0 ? rest.slice(0, i) : rest);
  const name = i >= 0 ? rest.slice(i + 1) : "";
  return { keycode: code, name: name || `Key ${code}` };
}

/** The keycaps to draw for a binding: one for a single key, several for a combo. */
export function capsFor(binding: string): Cap[] {
  const native = parseNative(binding);
  if (native) return [BY_KEYCODE[native.keycode] ?? { label: native.name }];
  return binding.split("+").map((t) => BY_TOKEN[t] ?? { label: t });
}

/** How to write a binding in a sentence: "fn", "right ⌘", "⌃⌥ space". */
export function spoken(binding: string): string {
  const caps = capsFor(binding);
  if (caps.length === 1) {
    const [c] = caps;
    const legend = c.symbol ?? c.label ?? "";
    return c.side ? `${c.side} ${legend}` : legend;
  }
  // A PC names a combo key by key: "ctrl+alt+space".
  if (!IS_MAC) return caps.map((c) => c.label ?? c.symbol).join("+");
  // A combo reads the way macOS menus print one: modifiers run together.
  const mods = caps.filter((c) => c.symbol).map((c) => c.symbol).join("");
  const keys = caps.filter((c) => !c.symbol).map((c) => c.label).join(" ");
  return [mods, keys].filter(Boolean).join(" ");
}
