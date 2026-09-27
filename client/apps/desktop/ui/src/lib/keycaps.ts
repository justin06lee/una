/**
 * A hotkey binding as the keys printed on a Mac keyboard: "fn" with its 🌐,
 * "⌘ command", "space". Lowercase like the keycaps themselves.
 */

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

/** macOS virtual keycodes with a legend of their own. */
const BY_KEYCODE: Record<number, Cap> = {
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

/** Combo tokens (the global-shortcut plugin's names). */
const BY_TOKEN: Record<string, Cap> = {
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

/** The modifier keycodes: held alone, they don't reach other apps' typing. */
export const MODIFIER_KEYCODES = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 179];
export const FN_KEYCODES = [63, 179];

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
    return c.side ? `${c.side} ${c.symbol}` : (c.label ?? c.symbol ?? "");
  }
  // A combo reads the way macOS menus print one: modifiers run together.
  const mods = caps.filter((c) => c.symbol).map((c) => c.symbol).join("");
  const keys = caps.filter((c) => !c.symbol).map((c) => c.label).join(" ");
  return [mods, keys].filter(Boolean).join(" ");
}
