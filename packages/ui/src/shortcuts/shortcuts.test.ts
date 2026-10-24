// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { format, type Translate } from "../i18n/catalog";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { IS_MAC } from "../useWindowShortcut";
import { bindingParts, bindingsEqual, describeBinding, eventKey, matches, parseBinding, toAccelerator, type Binding } from "./binding";
import { defaultBinding, isAvailable, REBINDABLE_COMMANDS } from "./commands";
import { bindingFor, effectiveBindings, matchesCommand, replaceOverrides, restoreAllBindings, restoreBinding, sanitizeOverrides, setBinding, shortcutOverrides } from "./store";
import { validateBinding } from "./validate";

const press = (binding: Binding, extra: KeyboardEventInit = {}) => new KeyboardEvent("keydown", { ...eventKey(binding), ...extra });
const defaults = effectiveBindings({});
const say = (language: "en" | "zh-Hans"): Translate => (key, ...args) => format(language, key, args[0] as Record<string, string | number> | undefined);
const primary = IS_MAC ? { metaKey: true } : { ctrlKey: true };

afterEach(() => restoreAllBindings());

describe("matching", () => {
  it("matches the physical key with exactly the modifiers of the binding", () => {
    const lock: Binding = { code: "KeyL", primary: true, shift: true };
    expect(matches(lock, press(lock))).toBe(true);
    expect(matches(lock, new KeyboardEvent("keydown", { code: "KeyL", shiftKey: true, ...primary, key: "ł" }))).toBe(true);
    expect(matches(lock, press({ code: "KeyL", primary: true }))).toBe(false);
    expect(matches(lock, press({ code: "KeyL", primary: true, shift: true, alt: true }))).toBe(false);
    expect(matches(lock, press({ code: "KeyL", primary: true, shift: true, other: true }))).toBe(false);
    expect(matches(lock, press({ code: "KeyP", primary: true, shift: true }))).toBe(false);
  });

  it("means ⌘ on an Apple keyboard and Ctrl elsewhere by `primary`, and the other key by `other`", () => {
    const key = eventKey({ code: "Tab", other: true });
    expect(key).toMatchObject(IS_MAC ? { ctrlKey: true, metaKey: false } : { ctrlKey: false, metaKey: true });
    expect(eventKey({ code: "KeyA", primary: true })).toMatchObject(IS_MAC ? { metaKey: true, ctrlKey: false } : { ctrlKey: true, metaKey: false });
  });

  it("follows the command's current binding", () => {
    const event = press({ code: "KeyJ", primary: true, shift: true });
    expect(matchesCommand("lock", event)).toBe(false);
    expect(setBinding("lock", { code: "KeyJ", primary: true, shift: true })).toBeUndefined();
    expect(matchesCommand("lock", event)).toBe(true);
    expect(matchesCommand("lock", press(defaultBinding("lock")))).toBe(false);
  });
});

describe("formatting", () => {
  it("draws modifiers in the order ⌃⌥⇧⌘ and then the key", () => {
    const parts = bindingParts({ code: "KeyF", primary: true, shift: true, alt: true, other: true });
    expect(parts).toEqual([
      { abbr: IS_MAC ? "ctrl" : "win" },
      { abbr: IS_MAC ? "option" : "alt" },
      { abbr: "shift" },
      { abbr: IS_MAC ? "command" : "ctrl" },
      { text: "F" },
    ]);
  });

  it("names keys by symbol where HeroUI has one, and by text otherwise", () => {
    expect(bindingParts({ code: "Tab", other: true }).at(-1)).toEqual({ abbr: "tab" });
    expect(bindingParts({ code: "Comma", primary: true }).at(-1)).toEqual({ text: "," });
    expect(bindingParts({ code: "BracketLeft", primary: true }).at(-1)?.text).toBe("[");
    expect(bindingParts({ code: "Digit5", primary: true }).at(-1)?.text).toBe("5");
    expect(bindingParts({ code: "Backslash", primary: true }).at(-1)?.text).toBe("\\");
  });

  it("says a binding in words for a screen reader", () => {
    expect(describeBinding({ code: "KeyF", primary: true, shift: true }, say("en"))).toBe(IS_MAC ? "Shift+Command+F" : "Shift+Ctrl+F");
    expect(describeBinding({ code: "PageUp", primary: true }, say("en"))).toBe(IS_MAC ? "Command+Page Up" : "Ctrl+Page Up");
    expect(describeBinding({ code: "ArrowUp", primary: true }, say("zh-Hans"))).toBe(IS_MAC ? "Command+上箭头" : "Ctrl+上箭头");
  });

  it("writes a Tauri accelerator", () => {
    expect(toAccelerator({ code: "Comma", primary: true })).toBe("CmdOrCtrl+Comma");
    expect(toAccelerator({ code: "KeyK", primary: true, shift: true, alt: true })).toBe("CmdOrCtrl+Alt+Shift+KeyK");
    expect(toAccelerator({ code: "F5", primary: true })).toBe("CmdOrCtrl+F5");
    expect(toAccelerator({ code: "Backslash", primary: true, other: true })).toBe(`CmdOrCtrl+${IS_MAC ? "Ctrl" : "Super"}+Backslash`);
  });
});

describe("parsing", () => {
  it("takes only a code with boolean modifiers, and normalises the rest", () => {
    expect(parseBinding({ code: "KeyA", primary: true, shift: false })).toEqual({ code: "KeyA", primary: true });
    for (const bad of [undefined, null, [], "KeyA", {}, { code: "" }, { code: "KeyA", primary: "yes" }]) expect(parseBinding(bad)).toBeUndefined();
  });

  it("takes only a code the app knows, and a modifier's is none", () => {
    for (const code of ["KeyA", "Digit0", "F1", "F24", "Backslash", "ArrowLeft", "PageDown", "Tab", "Space"]) expect(parseBinding({ code, primary: true })).toEqual({ code, primary: true });
    for (const code of ["<script>", "Nonsense", "F25", "KeyAA", "ShiftLeft", "MetaRight", "CapsLock", "AudioVolumeUp", "Numpad5", "NumpadAdd", "IntlBackslash"]) expect(parseBinding({ code, primary: true })).toBeUndefined();
  });
});

describe("the registry", () => {
  it("holds defaults no two of which are alike, each valid for its own command", () => {
    for (const command of REBINDABLE_COMMANDS) {
      expect(validateBinding(command.id, command.binding, defaults)).toBeUndefined();
      for (const other of REBINDABLE_COMMANDS) if (other !== command) expect(bindingsEqual(other.binding, command.binding)).toBe(false);
    }
  });

  it("offers each command where its client has what it needs", () => {
    const browser = { windowChrome: false, nativeMenu: false };
    const app = { windowChrome: true, nativeMenu: true };
    expect(isAvailable("always", browser)).toBe(true);
    expect(isAvailable("app", browser)).toBe(false);
    expect(isAvailable("nativeMenu", browser)).toBe(false);
    expect(isAvailable("app", app)).toBe(true);
    expect(isAvailable("macApp", app)).toBe(IS_MAC);
  });
});

describe("validation", () => {
  const check = (binding: Binding) => validateBinding("lock", binding, defaults);

  it("accepts a combination with the shortcut modifier that nothing else holds", () => {
    expect(check({ code: "KeyK", primary: true, shift: true })).toBeUndefined();
    expect(check({ code: "Digit1", primary: true, alt: true })).toBeUndefined();
    expect(check(defaultBinding("lock"))).toBeUndefined();
  });

  it("refuses a modifier alone, a key with no shortcut modifier, and the keys that keep their meaning", () => {
    expect(check({ code: "ShiftLeft", shift: true })).toEqual({ reason: "unsupportedKey" });
    expect(check({ code: "AudioVolumeUp", primary: true })).toEqual({ reason: "unsupportedKey" });
    expect(check({ code: "KeyK" })).toEqual({ reason: "needsModifier" });
    expect(check({ code: "KeyK", shift: true, alt: true })).toEqual({ reason: "needsModifier" });
    for (const code of ["Escape", "F6", "ContextMenu"]) expect(check({ code, primary: true })).toEqual({ reason: "reservedKey" });
    expect(check({ code: "Tab", primary: true })).toEqual({ reason: "osReserved" });
  });

  it("reserves the keys that move the caret or edit text, with ⌘ or Ctrl and whatever else is held", () => {
    for (const code of ["ArrowUp", "ArrowLeft", "Home", "End", "PageUp", "PageDown", "Backspace", "Delete", "Enter"]) {
      expect(check({ code, primary: true })).toEqual({ reason: "editingKey" });
      expect(check({ code, primary: true, shift: true, alt: true })).toEqual({ reason: "editingKey" });
    }
    expect(check({ code: "Insert", primary: true })).toBeUndefined();
  });

  it("refuses Shift with a number or symbol key for Open settings alone, whose key only the native menu matches", () => {
    for (const code of ["Digit1", "Comma", "Backslash", "Slash"]) {
      expect(validateBinding("openSettings", { code, primary: true, shift: true }, defaults)).toEqual({ reason: "shiftedKey" });
      expect(validateBinding("lock", { code, primary: true, shift: true }, defaults)).toBeUndefined();
    }
    expect(validateBinding("openSettings", { code: "KeyK", primary: true, shift: true }, defaults)).toBeUndefined();
    expect(sanitizeOverrides({ openSettings: { code: "Digit1", primary: true, shift: true } })).toEqual({ overrides: {}, skipped: 1 });
  });

  it("refuses the control keys of the terminal and what the system keeps", () => {
    expect(check({ code: "KeyK", other: true })).toEqual({ reason: IS_MAC ? "terminalControl" : "osReserved" });
    // ⌃Tab is allowed without ⌘ or Ctrl, for the console-session switch it belongs to.
    expect(validateBinding("nextConsoleSession", { code: "Tab", other: true, alt: true }, defaults)).toBeUndefined();
    if (IS_MAC) expect(check({ code: "Space", primary: true })).toEqual({ reason: "osReserved" });
    else expect(check({ code: "KeyC", primary: true })).toEqual({ reason: "interrupt" });
  });

  it("refuses another command's binding, also of a command this client does not have, and a fixed key", () => {
    expect(check(defaultBinding("pin"))).toEqual({ reason: "conflict", with: "pin" });
    expect(check(defaultBinding("back"))).toEqual({ reason: "conflict", with: "back" });
    expect(check({ code: "KeyV", primary: true })).toEqual({ reason: "conflict", with: "paste" });
    expect(check({ code: "KeyH", primary: true, alt: true })).toEqual({ reason: "conflict", with: "hideOthers" });
  });

  it("looks at the bindings as they stand, so a freed combination can be taken", () => {
    const moved = effectiveBindings({ pin: { code: "KeyK", primary: true, shift: true } });
    expect(validateBinding("lock", defaultBinding("pin"), moved)).toBeUndefined();
    expect(validateBinding("lock", { code: "KeyK", primary: true, shift: true }, moved)).toEqual({ reason: "conflict", with: "pin" });
  });
});

describe("the overrides", () => {
  const stored = () => localStorage.getItem(PREFERENCE_KEYS.keyboardShortcuts);

  it("keep only what differs from the defaults, and drop the entry once nothing does", () => {
    expect(setBinding("lock", { code: "KeyK", primary: true, shift: true })).toBeUndefined();
    expect(JSON.parse(stored()!)).toEqual({ lock: { code: "KeyK", primary: true, shift: true } });
    expect(bindingFor("lock")).toEqual({ code: "KeyK", primary: true, shift: true });
    expect(setBinding("lock", defaultBinding("lock"))).toBeUndefined();
    expect(stored()).toBeNull();
  });

  it("refuse a binding that cannot be used and change nothing", () => {
    expect(setBinding("lock", defaultBinding("pin"))).toEqual({ reason: "conflict", with: "pin" });
    expect(bindingFor("lock")).toEqual(defaultBinding("lock"));
    expect(stored()).toBeNull();
  });

  it("are read back from storage and merged with the defaults", () => {
    setBinding("archive", { code: "KeyK", primary: true, alt: true });
    const raw = stored();
    shortcutOverrides.set({});
    expect(bindingFor("archive")).toEqual(defaultBinding("archive"));
    localStorage.setItem(PREFERENCE_KEYS.keyboardShortcuts, raw!);
    expect(sanitizeOverrides(JSON.parse(raw!)).overrides).toEqual({ archive: { code: "KeyK", primary: true, alt: true } });
    expect(effectiveBindings({ archive: { code: "KeyK", primary: true, alt: true } }).lock).toEqual(defaultBinding("lock"));
  });

  it("drop an unknown id, a malformed binding and one that cannot stand, and count them", () => {
    const { overrides, skipped } = sanitizeOverrides({
      lock: { code: "KeyK", primary: true, shift: true },
      nonsense: { code: "KeyJ", primary: true },
      pin: "KeyP",
      archive: { code: "KeyK", primary: true, shift: true },
      copyId: { code: "Escape", primary: true },
      back: defaultBinding("back"),
    });
    expect(overrides).toEqual({ lock: { code: "KeyK", primary: true, shift: true } });
    expect(skipped).toBe(4);
    expect(sanitizeOverrides("not an object")).toEqual({ overrides: {}, skipped: 0 });
  });

  it("are judged as a whole, so a swap stands whatever order it is written in", () => {
    const lock = defaultBinding("lock");
    const pin = defaultBinding("pin");
    expect(sanitizeOverrides({ lock: pin, pin: lock })).toEqual({ overrides: { lock: pin, pin: lock }, skipped: 0 });
    expect(sanitizeOverrides({ pin: lock, lock: pin })).toEqual({ overrides: { pin: lock, lock: pin }, skipped: 0 });
  });

  it("are judged as a whole, so a chain of moves stands whatever order it is written in", () => {
    const moved = { code: "KeyK", primary: true, shift: true };
    const chain = { lock: defaultBinding("pin"), pin: moved };
    expect(sanitizeOverrides(chain)).toEqual({ overrides: chain, skipped: 0 });
    expect(sanitizeOverrides({ pin: moved, lock: defaultBinding("pin") })).toEqual({ overrides: { pin: moved, lock: defaultBinding("pin") }, skipped: 0 });
  });

  it("keep the earlier of two entries that claim one combination, and give the dropped command its default", () => {
    const claimed = { code: "KeyK", primary: true, shift: true };
    expect(sanitizeOverrides({ lock: claimed, pin: claimed })).toEqual({ overrides: { lock: claimed }, skipped: 1 });
    expect(sanitizeOverrides({ pin: claimed, lock: claimed })).toEqual({ overrides: { pin: claimed }, skipped: 1 });
    // The dropped pin keeps its own default, which the lock entry then clashes with.
    expect(sanitizeOverrides({ pin: { code: "Escape", primary: true }, lock: defaultBinding("pin") })).toEqual({ overrides: {}, skipped: 2 });
  });

  it("restore a default only when no other command holds it, and say who does", () => {
    setBinding("pin", { code: "KeyK", primary: true, shift: true });
    setBinding("lock", defaultBinding("pin"));
    expect(restoreBinding("pin")).toEqual({ reason: "conflict", with: "lock" });
    expect(bindingFor("pin")).toEqual({ code: "KeyK", primary: true, shift: true });
    expect(restoreBinding("lock")).toBeUndefined();
    expect(restoreBinding("pin")).toBeUndefined();
    expect(stored()).toBeNull();
  });

  it("are replaced as a whole by an import, which reports what stood", () => {
    setBinding("pin", { code: "KeyJ", primary: true, shift: true });
    expect(replaceOverrides({ lock: { code: "KeyK", primary: true, shift: true }, bad: 1 })).toEqual({ applied: 1, skipped: 1 });
    expect(bindingFor("pin")).toEqual(defaultBinding("pin"));
    expect(bindingFor("lock")).toEqual({ code: "KeyK", primary: true, shift: true });
    expect(replaceOverrides({})).toEqual({ applied: 0, skipped: 0 });
    expect(stored()).toBeNull();
  });
});
