import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { IS_MAC } from "../useWindowShortcut";

/**
 * A key combination in the form it is stored and exported in. `code` is the physical key
 * (`KeyboardEvent.code`), so a keyboard layout or a held Shift does not move it. The modifiers are
 * platform-neutral: `primary` is ⌘ on an Apple keyboard and Ctrl elsewhere, `other` the remaining one
 * of the two (Ctrl on an Apple keyboard, the Windows key elsewhere), so a default means the same
 * thing on every platform and a document exported on one can be applied on another.
 */
export interface Binding {
  code: string;
  primary?: boolean;
  shift?: boolean;
  alt?: boolean;
  other?: boolean;
}

/** The modifiers in the order a combination is written in: ⌃⌥⇧⌘. */
const MODIFIERS = ["other", "alt", "shift", "primary"] as const;

/** Codes of the keys that are modifiers themselves: a press of one alone is no combination. */
const MODIFIER_CODE = /^(Shift|Control|Alt|Meta|OS)(Left|Right)$|^(CapsLock|Fn|FnLock)$/;

export function isModifierCode(code: string): boolean {
  return MODIFIER_CODE.test(code);
}

/** The key codes (`KeyboardEvent.code`) a binding can have: the ones the app can record, show and
 * hand the native menu as an accelerator. A modifier is none of them. */
const KNOWN_CODE =
  /^(Key[A-Z]|Digit\d|F([1-9]|1\d|2[0-4])|Minus|Equal|BracketLeft|BracketRight|Semicolon|Quote|Backquote|Backslash|Comma|Period|Slash|Tab|Enter|Space|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(Up|Down|Left|Right)|Escape|ContextMenu)$/;

export function isKnownCode(code: string): boolean {
  return KNOWN_CODE.test(code);
}

/** The binding a key press is, with the modifiers held. */
export function bindingOf(event: Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">): Binding {
  const primary = IS_MAC ? event.metaKey : event.ctrlKey;
  const other = IS_MAC ? event.ctrlKey : event.metaKey;
  return normalize({ code: event.code, primary, other, shift: event.shiftKey, alt: event.altKey });
}

/** `binding` with every modifier either `true` or left out, so equal combinations are equal objects. */
function normalize(binding: Binding): Binding {
  const result: Binding = { code: binding.code };
  for (const modifier of MODIFIERS) if (binding[modifier]) result[modifier] = true;
  return result;
}

export function bindingsEqual(a: Binding, b: Binding): boolean {
  return a.code === b.code && MODIFIERS.every((modifier) => !!a[modifier] === !!b[modifier]);
}

/** Whether `event` is exactly `binding`: its key, and no modifier more or less. */
export function matches(binding: Binding, event: KeyboardEvent): boolean {
  return bindingsEqual(bindingOf(event), binding);
}

/** The binding a stored or imported value names, or `undefined` for anything that is not one: a code the app does not know, a modifier's among them. */
export function parseBinding(value: unknown): Binding | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const record = value as Record<string, unknown>;
  if (typeof record.code !== "string" || !isKnownCode(record.code)) return undefined;
  for (const modifier of MODIFIERS) {
    if (record[modifier] !== undefined && typeof record[modifier] !== "boolean") return undefined;
  }
  return normalize(record as unknown as Binding);
}

/** The key names HeroUI's `Kbd.Abbr` draws as a symbol. */
const SYMBOL_KEYS: Record<string, "tab" | "enter" | "space" | "delete" | "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown"> = {
  Tab: "tab",
  Enter: "enter",
  Space: "space",
  Backspace: "delete",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
};

const PUNCTUATION: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
};

/** The text of a key that has no symbol: `KeyF` is F, `Digit1` is 1, `Comma` is a comma. */
function keyText(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return PUNCTUATION[code] ?? code;
}

export interface BindingPart {
  /** A modifier or a key `Kbd.Abbr` has a symbol for. */
  abbr?: "command" | "shift" | "ctrl" | "option" | "win" | "alt" | NonNullable<(typeof SYMBOL_KEYS)[string]>;
  /** The key's own text, where it has no symbol. */
  text?: string;
}

function modifierPart(modifier: (typeof MODIFIERS)[number]): BindingPart {
  switch (modifier) {
    case "shift": return { abbr: "shift" };
    case "alt": return { abbr: IS_MAC ? "option" : "alt" };
    case "primary": return { abbr: IS_MAC ? "command" : "ctrl" };
    case "other": return { abbr: IS_MAC ? "ctrl" : "win" };
  }
}

/** What a combination is drawn from, in reading order: its modifiers, then its key. */
export function bindingParts(binding: Binding): BindingPart[] {
  const parts: BindingPart[] = MODIFIERS.filter((modifier) => binding[modifier]).map(modifierPart);
  const symbol = SYMBOL_KEYS[binding.code];
  parts.push(symbol ? { abbr: symbol } : { text: keyText(binding.code) });
  return parts;
}

/** What a screen reader says for each `Kbd.Abbr` key: macOS says Control for its ⌃, and Ctrl is the key of that name elsewhere. */
const SPOKEN_NAMES = {
  command: "settings.shortcuts.key.command",
  shift: "settings.shortcuts.key.shift",
  ctrl: IS_MAC ? "settings.shortcuts.key.control" : "settings.shortcuts.key.ctrl",
  option: "settings.shortcuts.key.option",
  win: "settings.shortcuts.key.windows",
  alt: "settings.shortcuts.key.alt",
  tab: "settings.shortcuts.key.tab",
  enter: "settings.shortcuts.key.enter",
  space: "settings.shortcuts.key.space",
  delete: "settings.shortcuts.key.backspace",
  up: "settings.shortcuts.key.up",
  down: "settings.shortcuts.key.down",
  left: "settings.shortcuts.key.left",
  right: "settings.shortcuts.key.right",
  home: "settings.shortcuts.key.home",
  end: "settings.shortcuts.key.end",
  pageup: "settings.shortcuts.key.pageUp",
  pagedown: "settings.shortcuts.key.pageDown",
} as const satisfies Record<NonNullable<BindingPart["abbr"]>, PlainMessageKey>;

/** The combination as a screen reader should say it: `Shift+Command+F`, in the language of `t`. */
export function describeBinding(binding: Binding, t: Translate): string {
  return bindingParts(binding).map((part) => (part.abbr ? t(SPOKEN_NAMES[part.abbr]) : part.text)).join("+");
}

/**
 * The combination as a Tauri menu accelerator (`CmdOrCtrl+Shift+Comma`): the modifiers and the
 * key's code, which the menu's parser takes by its W3C name.
 */
export function toAccelerator(binding: Binding): string {
  const modifiers = [
    binding.primary && "CmdOrCtrl",
    binding.other && (IS_MAC ? "Ctrl" : "Super"),
    binding.alt && "Alt",
    binding.shift && "Shift",
  ].filter((modifier) => typeof modifier === "string");
  return [...modifiers, binding.code].join("+");
}

/** A binding as the modifier flags a key event of it carries, for a document that cannot know the platform. */
export interface EventKey {
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export function eventKey(binding: Binding): EventKey {
  const { primary = false, other = false, shift = false, alt = false } = binding;
  return {
    code: binding.code,
    metaKey: IS_MAC ? primary : other,
    ctrlKey: IS_MAC ? other : primary,
    shiftKey: shift,
    altKey: alt,
  };
}
