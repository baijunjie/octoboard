import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { HostFacts } from "../platform/host";
import type { KbdKey } from "../primitives";

/**
 * A key combination in the form it is stored and exported in. `code` is the physical key
 * (`KeyboardEvent.code`), so a keyboard layout or a held Shift does not move it. The modifiers are
 * platform-neutral: `primary` is ⌘ on an Apple keyboard and Ctrl elsewhere, `other` the remaining one
 * of the two (Ctrl on an Apple keyboard, the Windows key elsewhere), so a default means the same
 * thing on every platform and a document exported on one can be applied on another.
 *
 * `ctrl` is the Control key itself on every keyboard, for a combination whose modifier is Control
 * wherever it runs rather than the platform's own (the console-session switch's Ctrl+Tab, the
 * browsers' tab key on every system). On a host it is the one of `primary` and `other` that is
 * Control there (`onHost`), so it is compared, matched and drawn as that key; a binding holding
 * it holds neither of those two (`parseBinding`). A recording never writes it: a key press is
 * stored in the platform-neutral modifiers.
 */
export interface Binding {
  code: string;
  primary?: boolean;
  shift?: boolean;
  alt?: boolean;
  other?: boolean;
  ctrl?: boolean;
}

/** The modifiers in the order a combination is written in on an Apple keyboard: ⌃⌥⇧⌘. A binding
 * on a host (`onHost`) has no others. */
const MODIFIERS = ["other", "alt", "shift", "primary"] as const;

/** Every modifier a stored binding can hold. */
const STORED_MODIFIERS = [...MODIFIERS, "ctrl"] as const;

/** The order elsewhere, as Windows writes a combination: Win, Ctrl, Alt, Shift (Ctrl+Shift+F, Win+Shift+S). */
const MODIFIERS_ELSEWHERE: readonly (typeof MODIFIERS)[number][] = ["other", "primary", "alt", "shift"];

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

/** What a binding's meaning depends on: which of ⌘ and Ctrl is the primary modifier. */
export type ModifierHost = Pick<HostFacts, "modifier">;

/** The binding a key press is, with the modifiers held. */
export function bindingOf(event: Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">, host: ModifierHost): Binding {
  const apple = host.modifier === "meta";
  const primary = apple ? event.metaKey : event.ctrlKey;
  const other = apple ? event.ctrlKey : event.metaKey;
  return normalize({ code: event.code, primary, other, shift: event.shiftKey, alt: event.altKey });
}

/** `binding` with every modifier either `true` or left out, so equal combinations are equal objects. */
function normalize(binding: Binding): Binding {
  const result: Binding = { code: binding.code };
  for (const modifier of STORED_MODIFIERS) if (binding[modifier]) result[modifier] = true;
  return result;
}

/** Whether two bindings are written the same. Whether they are the same keys on a host is `sameKeys`. */
export function bindingsEqual(a: Binding, b: Binding): boolean {
  return a.code === b.code && STORED_MODIFIERS.every((modifier) => !!a[modifier] === !!b[modifier]);
}

/** `binding` in the modifiers `host` has: its `ctrl` held as `other` on an Apple keyboard, where Control is
 * that one, and as `primary` elsewhere. Every binding is compared, matched and drawn in this form. */
export function onHost(binding: Binding, host: ModifierHost): Binding {
  if (!binding.ctrl) return binding;
  const { ctrl: _ctrl, ...rest } = binding;
  return normalize({ ...rest, [host.modifier === "meta" ? "other" : "primary"]: true });
}

/** Whether two bindings are the same keys on `host`, however each is written. */
export function sameKeys(a: Binding, b: Binding, host: ModifierHost): boolean {
  return bindingsEqual(onHost(a, host), onHost(b, host));
}

/** Whether `event` is exactly `binding`: its key, and no modifier more or less. */
export function matches(binding: Binding, event: KeyboardEvent, host: ModifierHost): boolean {
  return sameKeys(bindingOf(event, host), binding, host);
}

/** The binding a stored or imported value names, or `undefined` for anything that is not one: a code the app does not
 * know, a modifier's among them, or `ctrl` held with `primary` or `other`, one of which is that same Control key on
 * every host. */
export function parseBinding(value: unknown): Binding | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const record = value as Record<string, unknown>;
  if (typeof record.code !== "string" || !isKnownCode(record.code)) return undefined;
  for (const modifier of STORED_MODIFIERS) {
    if (record[modifier] !== undefined && typeof record[modifier] !== "boolean") return undefined;
  }
  if (record.ctrl === true && (record.primary === true || record.other === true)) return undefined;
  return normalize(record as unknown as Binding);
}

/** The key names HeroUI's key caps draw as a symbol (`KbdAbbr`). */
const SYMBOL_KEYS: Record<string, KbdKey> = {
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
  /** A modifier or a key `KbdAbbr` has a symbol for: the key's identity, which a screen reader speaks. */
  abbr?: KbdKey;
  /** The key's own text, where it has no symbol. */
  text?: string;
  /** What to draw for an `abbr` in place of HeroUI's symbol, where that symbol is a macOS one the host's users do not read. */
  display?: string;
}

/** What a key cap says for the keys whose HeroUI symbol is a macOS one: the names the keys carry elsewhere. */
const TEXT_CAPS: Partial<Record<KbdKey, string>> = {
  ctrl: "Ctrl",
  alt: "Alt",
  win: "Win",
  shift: "Shift",
  home: "Home",
  end: "End",
  pageup: "PgUp",
  pagedown: "PgDn",
};

function modifierPart(modifier: (typeof MODIFIERS)[number], apple: boolean): BindingPart {
  switch (modifier) {
    case "shift": return { abbr: "shift" };
    case "alt": return { abbr: apple ? "option" : "alt" };
    case "primary": return { abbr: apple ? "command" : "ctrl" };
    case "other": return { abbr: apple ? "ctrl" : "win" };
  }
}

/** `part` as its host draws it: on an Apple host the symbol HeroUI has, elsewhere the key's name. */
function drawnFor(part: BindingPart, apple: boolean): BindingPart {
  const display = part.abbr && TEXT_CAPS[part.abbr];
  return apple || !display ? part : { ...part, display };
}

/** What a combination is drawn from, in reading order: its modifiers, then its key. */
export function bindingParts(stored: Binding, host: ModifierHost): BindingPart[] {
  const apple = host.modifier === "meta";
  const binding = onHost(stored, host);
  const parts: BindingPart[] = (apple ? MODIFIERS : MODIFIERS_ELSEWHERE).filter((modifier) => binding[modifier]).map((modifier) => modifierPart(modifier, apple));
  const symbol = SYMBOL_KEYS[binding.code];
  parts.push(symbol ? { abbr: symbol } : { text: keyText(binding.code) });
  return parts.map((part) => drawnFor(part, apple));
}

/** What a screen reader says for each `KbdAbbr` key: macOS says Control for its ⌃, and Ctrl is the key of that name elsewhere. */
const SPOKEN_NAMES = {
  command: "settings.shortcuts.key.command",
  shift: "settings.shortcuts.key.shift",
  ctrl: "settings.shortcuts.key.ctrl",
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
export function describeBinding(binding: Binding, t: Translate, host: ModifierHost): string {
  const spoken = (abbr: NonNullable<BindingPart["abbr"]>): PlainMessageKey =>
    abbr === "ctrl" && host.modifier === "meta" ? "settings.shortcuts.key.control" : SPOKEN_NAMES[abbr];
  return bindingParts(binding, host).map((part) => (part.abbr ? t(spoken(part.abbr)) : part.text)).join("+");
}

/**
 * The combination as a Tauri menu accelerator (`CmdOrCtrl+Shift+Comma`): the modifiers and the
 * key's code, which the menu's parser takes by its W3C name.
 */
export function toAccelerator(stored: Binding, host: ModifierHost): string {
  const binding = onHost(stored, host);
  const modifiers = [
    binding.primary && "CmdOrCtrl",
    binding.other && (host.modifier === "meta" ? "Ctrl" : "Super"),
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

export function eventKey(stored: Binding, host: ModifierHost): EventKey {
  const binding = onHost(stored, host);
  const { primary = false, other = false, shift = false, alt = false } = binding;
  const apple = host.modifier === "meta";
  return {
    code: binding.code,
    metaKey: apple ? primary : other,
    ctrlKey: apple ? other : primary,
    shiftKey: shift,
    altKey: alt,
  };
}

/** A combination in the platform-neutral key names that `aria-keyshortcuts` requires. */
export function ariaBinding(stored: Binding, host: ModifierHost): string {
  const binding = onHost(stored, host);
  const apple = host.modifier === "meta";
  const names = { other: apple ? "Control" : "Meta", alt: "Alt", shift: "Shift", primary: apple ? "Meta" : "Control" };
  return [...MODIFIERS.filter((modifier) => binding[modifier]).map((modifier) => names[modifier]), keyText(binding.code)].join("+");
}
