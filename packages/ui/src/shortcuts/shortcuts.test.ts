// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";

import { translatorFor, type Translate } from "../i18n/catalog";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { HostFacts } from "../platform/host";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { ariaBinding, bindingOf, bindingParts as bindingPartsFor, describeBinding as describeBindingFor, eventKey as eventKeyFor, matches as matchesFor, onHost, parseBinding, sameKeys, toAccelerator as toAcceleratorFor, type Binding } from "./binding";
import { clientCapabilities, defaultBinding, isAvailable, REBINDABLE_COMMANDS } from "./commands";
import { bindingFor, effectiveBindings, matchesCommand as matchesCommandFor, replaceOverrides as replaceOverridesFor, restoreAllBindings, restoreBinding as restoreBindingFor, sanitizeOverrides as sanitizeOverridesFor, setBinding as setBindingFor, shortcutOverrides } from "./store";
import { validateBinding as validateBindingFor } from "./validate";

/** The two kinds of host a shortcut can run on: the Apple keyboard, where ⌘ is the primary modifier, and every other. */
describe.each<[string, HostFacts]>([["a macOS host", MAC_HOST], ["a Windows host", WINDOWS_HOST]])("on %s", (_name, host) => {
  const apple = host.modifier === "meta";
  const eventKey = (binding: Binding) => eventKeyFor(binding, host);
  const matches = (binding: Binding, event: KeyboardEvent) => matchesFor(binding, event, host);
  const matchesCommand = (id: Parameters<typeof matchesCommandFor>[0], event: KeyboardEvent) => matchesCommandFor(id, event, host);
  const setBinding = (id: Parameters<typeof setBindingFor>[0], binding: Binding) => setBindingFor(id, binding, host);
  const restoreBinding = (id: Parameters<typeof restoreBindingFor>[0]) => restoreBindingFor(id, host);
  const replaceOverrides = (value: unknown) => replaceOverridesFor(value, host);
  const sanitizeOverrides = (value: unknown) => sanitizeOverridesFor(value, host);
  const validateBinding = (id: Parameters<typeof validateBindingFor>[0], binding: Binding, current: Parameters<typeof validateBindingFor>[2]) => validateBindingFor(id, binding, current, host);
  const bindingParts = (binding: Binding) => bindingPartsFor(binding, host);
  const describeBinding = (binding: Binding, t: Translate) => describeBindingFor(binding, t, host);
  const toAccelerator = (binding: Binding) => toAcceleratorFor(binding, host);

  const press = (binding: Binding, extra: KeyboardEventInit = {}) => new KeyboardEvent("keydown", { ...eventKey(binding), ...extra });
  const defaults = effectiveBindings({});
  const say = (language: "en" | "zh-Hans"): Translate => translatorFor(language);
  const primary = apple ? { metaKey: true } : { ctrlKey: true };

  afterEach(() => restoreAllBindings());

  describe("matching", () => {
    it("matches the physical key with exactly the modifiers of the binding", () => {
      const focusMode: Binding = { code: "KeyF", primary: true, shift: true };
      expect(matches(focusMode, press(focusMode))).toBe(true);
      expect(matches(focusMode, new KeyboardEvent("keydown", { code: "KeyF", shiftKey: true, ...primary, key: "ł" }))).toBe(true);
      expect(matches(focusMode, press({ code: "KeyF", primary: true }))).toBe(false);
      expect(matches(focusMode, press({ code: "KeyF", primary: true, shift: true, alt: true }))).toBe(false);
      expect(matches(focusMode, press({ code: "KeyF", primary: true, shift: true, other: true }))).toBe(false);
      expect(matches(focusMode, press({ code: "KeyN", primary: true, shift: true }))).toBe(false);
    });

    it("means ⌘ on an Apple keyboard and Ctrl elsewhere by `primary`, and the other key by `other`", () => {
      const key = eventKey({ code: "Tab", other: true });
      expect(key).toMatchObject(apple ? { ctrlKey: true, metaKey: false } : { ctrlKey: false, metaKey: true });
      expect(eventKey({ code: "KeyA", primary: true })).toMatchObject(apple ? { metaKey: true, ctrlKey: false } : { ctrlKey: true, metaKey: false });
    });

    it("means Control by `ctrl` on every keyboard: the modifier `other` names on an Apple one and `primary` elsewhere", () => {
      const ctrlTab: Binding = { code: "Tab", ctrl: true };
      expect(eventKey(ctrlTab)).toEqual({ code: "Tab", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false });
      expect(matches(ctrlTab, new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true }))).toBe(true);
      expect(matches(ctrlTab, new KeyboardEvent("keydown", { code: "Tab", metaKey: true }))).toBe(false);
      expect(matches(ctrlTab, new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true, metaKey: true }))).toBe(false);
      expect(onHost(ctrlTab, host)).toEqual(apple ? { code: "Tab", other: true } : { code: "Tab", primary: true });
      expect(sameKeys(ctrlTab, { code: "Tab", other: true }, host)).toBe(apple);
      expect(sameKeys(ctrlTab, { code: "Tab", primary: true }, host)).toBe(!apple);
    });

    it("switches console session on Ctrl+Tab and Ctrl+Shift+Tab by default, never on ⌘ or the Windows key", () => {
      const tab = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { code: "Tab", ...init });
      expect(matchesCommand("nextConsoleSession", tab({ ctrlKey: true }))).toBe(true);
      expect(matchesCommand("previousConsoleSession", tab({ ctrlKey: true, shiftKey: true }))).toBe(true);
      expect(matchesCommand("nextConsoleSession", tab({ metaKey: true }))).toBe(false);
      expect(matchesCommand("previousConsoleSession", tab({ metaKey: true, shiftKey: true }))).toBe(false);
    });

    it("follows the command's current binding", () => {
      const event = press({ code: "KeyJ", primary: true, shift: true });
      expect(matchesCommand("focusMode", event)).toBe(false);
      expect(setBinding("focusMode", { code: "KeyJ", primary: true, shift: true })).toBeUndefined();
      expect(matchesCommand("focusMode", event)).toBe(true);
      expect(matchesCommand("focusMode", press(defaultBinding("focusMode")))).toBe(false);
    });
  });

  describe("formatting", () => {
    it("draws modifiers in the platform's order (⌃⌥⇧⌘ on an Apple keyboard, Win Ctrl Alt Shift elsewhere) and then the key", () => {
      const parts = bindingParts({ code: "KeyF", primary: true, shift: true, alt: true, other: true });
      expect(parts).toEqual(
        apple
          ? [{ abbr: "ctrl" }, { abbr: "option" }, { abbr: "shift" }, { abbr: "command" }, { text: "F" }]
          : [{ abbr: "win", display: "Win" }, { abbr: "ctrl", display: "Ctrl" }, { abbr: "alt", display: "Alt" }, { abbr: "shift", display: "Shift" }, { text: "F" }],
      );
    });

    it("names keys by symbol where HeroUI has one, and by text otherwise", () => {
      expect(bindingParts({ code: "Tab", other: true }).at(-1)).toEqual({ abbr: "tab" });
      expect(bindingParts({ code: "Comma", primary: true }).at(-1)).toEqual({ text: "," });
      expect(bindingParts({ code: "BracketLeft", primary: true }).at(-1)?.text).toBe("[");
      expect(bindingParts({ code: "Digit5", primary: true }).at(-1)?.text).toBe("5");
      expect(bindingParts({ code: "Backslash", primary: true }).at(-1)?.text).toBe("\\");
    });

    it("says a binding in words for a screen reader", () => {
      expect(describeBinding({ code: "KeyF", primary: true, shift: true }, say("en"))).toBe(apple ? "Shift+Command+F" : "Ctrl+Shift+F");
      expect(describeBinding({ code: "PageUp", primary: true }, say("en"))).toBe(apple ? "Command+Page Up" : "Ctrl+Page Up");
      expect(describeBinding({ code: "ArrowUp", primary: true }, say("zh-Hans"))).toBe(apple ? "Command+上箭头" : "Ctrl+上箭头");
    });

    it("draws and says Control as the key it is on the host", () => {
      expect(bindingParts({ code: "Tab", ctrl: true })).toEqual(apple ? [{ abbr: "ctrl" }, { abbr: "tab" }] : [{ abbr: "ctrl", display: "Ctrl" }, { abbr: "tab" }]);
      // In the host's own order: ⌃⌥⇧⌘ on an Apple keyboard, Win Ctrl Alt Shift elsewhere.
      expect(bindingParts({ code: "KeyK", ctrl: true, shift: true, alt: true }).map((part) => part.display ?? part.abbr ?? part.text)).toEqual(apple ? ["ctrl", "option", "shift", "K"] : ["Ctrl", "Alt", "Shift", "K"]);
      expect(describeBinding(defaultBinding("previousConsoleSession"), say("en"))).toBe(apple ? "Control+Shift+Tab" : "Ctrl+Shift+Tab");
      expect(ariaBinding(defaultBinding("nextConsoleSession"), host)).toBe("Control+Tab");
      expect(toAccelerator({ code: "KeyK", ctrl: true })).toBe(apple ? "Ctrl+KeyK" : "CmdOrCtrl+KeyK");
    });

    it("writes a Tauri accelerator", () => {
      expect(toAccelerator({ code: "Comma", primary: true })).toBe("CmdOrCtrl+Comma");
      expect(toAccelerator({ code: "KeyK", primary: true, shift: true, alt: true })).toBe("CmdOrCtrl+Alt+Shift+KeyK");
      expect(toAccelerator({ code: "F5", primary: true })).toBe("CmdOrCtrl+F5");
      expect(toAccelerator({ code: "Backslash", primary: true, other: true })).toBe(`CmdOrCtrl+${apple ? "Ctrl" : "Super"}+Backslash`);
    });
  });

  describe("parsing", () => {
    it("takes only a code with boolean modifiers, and normalises the rest", () => {
      expect(parseBinding({ code: "KeyA", primary: true, shift: false })).toEqual({ code: "KeyA", primary: true });
      for (const bad of [undefined, null, [], "KeyA", {}, { code: "" }, { code: "KeyA", primary: "yes" }]) expect(parseBinding(bad)).toBeUndefined();
    });

    it("takes `ctrl` only without `primary` and `other`, one of which is the same key on every host", () => {
      expect(parseBinding({ code: "Tab", ctrl: true, shift: true, other: false })).toEqual({ code: "Tab", ctrl: true, shift: true });
      expect(parseBinding({ code: "Tab", ctrl: true, primary: true })).toBeUndefined();
      expect(parseBinding({ code: "Tab", ctrl: true, other: true })).toBeUndefined();
      expect(parseBinding({ code: "Tab", ctrl: "yes" })).toBeUndefined();
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
        for (const other of REBINDABLE_COMMANDS) if (other !== command) expect(sameKeys(other.binding, command.binding, host)).toBe(false);
      }
    });

    it("offers each command where its client has what it needs", () => {
      const browser = clientCapabilities({ kind: "browser", host });
      const app = clientCapabilities({ kind: "tauri", host, appMenu: apple ? { onSettingsRequested: () => () => {}, setLabels: async () => {} } : undefined });
      expect(isAvailable("always", browser)).toBe(true);
      expect(isAvailable("app", browser)).toBe(false);
      expect(isAvailable("macApp", browser)).toBe(false);
      expect(isAvailable("nativeMenu", browser)).toBe(false);
      // The desktop app has its own keys on every system; the Windows one has no native menu.
      expect(isAvailable("app", app)).toBe(true);
      expect(isAvailable("macApp", app)).toBe(apple);
      expect(isAvailable("nativeMenu", app)).toBe(apple);
    });
  });

  describe("validation", () => {
    const check = (binding: Binding) => validateBinding("focusMode", binding, defaults);

    it("accepts a combination with the shortcut modifier that nothing else holds", () => {
      expect(check({ code: "KeyK", primary: true, shift: true })).toBeUndefined();
      expect(check({ code: "Digit1", primary: true, alt: true })).toBeUndefined();
      expect(check(defaultBinding("focusMode"))).toBeUndefined();
    });

    it("refuses a modifier alone, a key with no shortcut modifier, and the keys that keep their meaning", () => {
      expect(check({ code: "ShiftLeft", shift: true })).toEqual({ reason: "unsupportedKey" });
      expect(check({ code: "AudioVolumeUp", primary: true })).toEqual({ reason: "unsupportedKey" });
      expect(check({ code: "KeyK" })).toEqual({ reason: "needsModifier" });
      expect(check({ code: "KeyK", shift: true, alt: true })).toEqual({ reason: "needsModifier" });
      for (const code of ["Escape", "F6", "ContextMenu"]) expect(check({ code, primary: true })).toEqual({ reason: "reservedKey" });
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
        expect(validateBinding("focusMode", { code, primary: true, shift: true }, defaults)).toBeUndefined();
      }
      expect(validateBinding("openSettings", { code: "KeyK", primary: true, shift: true }, defaults)).toBeUndefined();
      expect(sanitizeOverrides({ openSettings: { code: "Digit1", primary: true, shift: true } })).toEqual({ overrides: {}, skipped: 1 });
    });

    it("refuses the control keys of the terminal and what the system keeps", () => {
      expect(check({ code: "KeyK", other: true })).toEqual({ reason: apple ? "terminalControl" : "osReserved" });
      if (apple) expect(check({ code: "Space", primary: true })).toEqual({ reason: "osReserved" });
      else expect(check({ code: "KeyC", primary: true })).toEqual({ reason: "interrupt" });
    });

    it("takes Tab only with Control: ⌃Tab with ⌥ or ⇧ on an Apple keyboard, Ctrl+Tab and Ctrl+Shift+Tab elsewhere", () => {
      const tab = (modifiers: Omit<Binding, "code">) => validateBinding("nextConsoleSession", { code: "Tab", ...modifiers }, defaults);
      expect(tab({ ctrl: true })).toBeUndefined();
      expect(tab({ ctrl: true, alt: true })).toEqual(apple ? undefined : { reason: "osReserved" });
      expect(tab({ ctrl: true, alt: true, shift: true })).toEqual(apple ? undefined : { reason: "osReserved" });
      // ⌘Tab is macOS's application switcher, and Tab with the Windows key Windows' Task View.
      expect(tab({ primary: true })).toEqual(apple ? { reason: "osReserved" } : undefined);
      expect(tab({ other: true })).toEqual(apple ? undefined : { reason: "osReserved" });
      expect(tab({ primary: true, other: true })).toEqual({ reason: "osReserved" });
      expect(tab({ alt: true })).toEqual({ reason: "needsModifier" });
    });

    it("finds a command's keys however they are written, so Control with Tab is the switch's on every keyboard", () => {
      for (const written of [{ code: "Tab", ctrl: true }, onHost({ code: "Tab", ctrl: true }, host)]) {
        expect(check(written)).toEqual({ reason: "conflict", with: "nextConsoleSession" });
        expect(validateBinding("previousConsoleSession", written, defaults)).toEqual({ reason: "conflict", with: "nextConsoleSession" });
      }
      // The same written keys are another combination on the other kind of keyboard, which nothing holds.
      expect(validateBinding("previousConsoleSession", { code: "Tab", other: true }, defaults)).toEqual(apple ? { reason: "conflict", with: "nextConsoleSession" } : { reason: "osReserved" });
    });

    it("refuses another command's binding, also of a command this client does not have, and a fixed key", () => {
      expect(check(defaultBinding("quickCreate"))).toEqual({ reason: "conflict", with: "quickCreate" });
      expect(check(defaultBinding("back"))).toEqual({ reason: "conflict", with: "back" });
      expect(check({ code: "KeyV", primary: true })).toEqual({ reason: "conflict", with: "paste" });
      expect(check({ code: "KeyH", primary: true, alt: true })).toEqual({ reason: "conflict", with: "hideOthers" });
      // Find is the file viewer's and the Settings dialog's own, so no command can take the key their dialogs listen for.
      expect(check({ code: "KeyF", primary: true })).toEqual({ reason: "conflict", with: "find" });
    });

    it("looks at the bindings as they stand, so a freed combination can be taken", () => {
      const moved = effectiveBindings({ quickCreate: { code: "KeyK", primary: true, shift: true } });
      expect(validateBinding("focusMode", defaultBinding("quickCreate"), moved)).toBeUndefined();
      expect(validateBinding("focusMode", { code: "KeyK", primary: true, shift: true }, moved)).toEqual({ reason: "conflict", with: "quickCreate" });
    });
  });

  describe("the overrides", () => {
    const stored = () => localStorage.getItem(PREFERENCE_KEYS.keyboardShortcuts);

    it("keep only what differs from the defaults, and drop the entry once nothing does", () => {
      expect(setBinding("focusMode", { code: "KeyK", primary: true, shift: true })).toBeUndefined();
      expect(JSON.parse(stored()!)).toEqual({ focusMode: { code: "KeyK", primary: true, shift: true } });
      expect(bindingFor("focusMode")).toEqual({ code: "KeyK", primary: true, shift: true });
      expect(setBinding("focusMode", defaultBinding("focusMode"))).toBeUndefined();
      expect(stored()).toBeNull();
    });

    it("keep nothing for the default's own keys, however a recording writes them", () => {
      expect(setBinding("nextConsoleSession", { code: "KeyJ", primary: true, alt: true })).toBeUndefined();
      const pressed = bindingOf(new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true }), host);
      expect(pressed).toEqual(apple ? { code: "Tab", other: true } : { code: "Tab", primary: true });
      expect(setBinding("nextConsoleSession", pressed)).toBeUndefined();
      expect(stored()).toBeNull();
      expect(sanitizeOverrides({ nextConsoleSession: pressed })).toEqual({ overrides: {}, skipped: 0 });
    });

    it("refuse a binding that cannot be used and change nothing", () => {
      expect(setBinding("focusMode", defaultBinding("quickCreate"))).toEqual({ reason: "conflict", with: "quickCreate" });
      expect(bindingFor("focusMode")).toEqual(defaultBinding("focusMode"));
      expect(stored()).toBeNull();
    });

    it("are read back from storage and merged with the defaults", () => {
      setBinding("newSession", { code: "KeyK", primary: true, alt: true });
      const raw = stored();
      shortcutOverrides.set({});
      expect(bindingFor("newSession")).toEqual(defaultBinding("newSession"));
      localStorage.setItem(PREFERENCE_KEYS.keyboardShortcuts, raw!);
      expect(sanitizeOverrides(JSON.parse(raw!)).overrides).toEqual({ newSession: { code: "KeyK", primary: true, alt: true } });
      expect(effectiveBindings({ newSession: { code: "KeyK", primary: true, alt: true } }).focusMode).toEqual(defaultBinding("focusMode"));
    });

    it("drop an unknown id, a malformed binding and one that cannot stand, and count them", () => {
      const { overrides, skipped } = sanitizeOverrides({
        focusMode: { code: "KeyK", primary: true, shift: true },
        nonsense: { code: "KeyJ", primary: true },
        quickCreate: "KeyN",
        newSession: { code: "KeyK", primary: true, shift: true },
        openSettings: { code: "Escape", primary: true },
        back: defaultBinding("back"),
      });
      expect(overrides).toEqual({ focusMode: { code: "KeyK", primary: true, shift: true } });
      expect(skipped).toBe(4);
      expect(sanitizeOverrides("not an object")).toEqual({ overrides: {}, skipped: 0 });
    });

    it("are judged as a whole, so a swap stands whatever order it is written in", () => {
      const focusMode = defaultBinding("focusMode");
      const quickCreate = defaultBinding("quickCreate");
      expect(sanitizeOverrides({ focusMode: quickCreate, quickCreate: focusMode })).toEqual({ overrides: { focusMode: quickCreate, quickCreate: focusMode }, skipped: 0 });
      expect(sanitizeOverrides({ quickCreate: focusMode, focusMode: quickCreate })).toEqual({ overrides: { quickCreate: focusMode, focusMode: quickCreate }, skipped: 0 });
    });

    it("are judged as a whole, so a chain of moves stands whatever order it is written in", () => {
      const moved = { code: "KeyK", primary: true, shift: true };
      const chain = { focusMode: defaultBinding("quickCreate"), quickCreate: moved };
      expect(sanitizeOverrides(chain)).toEqual({ overrides: chain, skipped: 0 });
      expect(sanitizeOverrides({ quickCreate: moved, focusMode: defaultBinding("quickCreate") })).toEqual({ overrides: { quickCreate: moved, focusMode: defaultBinding("quickCreate") }, skipped: 0 });
    });

    it("keep the earlier of two entries that claim one combination, and give the dropped command its default", () => {
      const claimed = { code: "KeyK", primary: true, shift: true };
      expect(sanitizeOverrides({ focusMode: claimed, quickCreate: claimed })).toEqual({ overrides: { focusMode: claimed }, skipped: 1 });
      expect(sanitizeOverrides({ quickCreate: claimed, focusMode: claimed })).toEqual({ overrides: { quickCreate: claimed }, skipped: 1 });
      // The dropped quickCreate keeps its own default, which the focusMode entry then clashes with.
      expect(sanitizeOverrides({ quickCreate: { code: "Escape", primary: true }, focusMode: defaultBinding("quickCreate") })).toEqual({ overrides: {}, skipped: 2 });
    });

    it("restore a default only when no other command holds it, and say who does", () => {
      setBinding("quickCreate", { code: "KeyK", primary: true, shift: true });
      setBinding("focusMode", defaultBinding("quickCreate"));
      expect(restoreBinding("quickCreate")).toEqual({ reason: "conflict", with: "focusMode" });
      expect(bindingFor("quickCreate")).toEqual({ code: "KeyK", primary: true, shift: true });
      expect(restoreBinding("focusMode")).toBeUndefined();
      expect(restoreBinding("quickCreate")).toBeUndefined();
      expect(stored()).toBeNull();
    });

    it("are replaced as a whole by an import, which reports what stood and which overrides were set", () => {
      setBinding("quickCreate", { code: "KeyJ", primary: true, shift: true });
      expect(replaceOverrides({ focusMode: { code: "KeyK", primary: true, shift: true }, bad: 1 })).toEqual({ applied: 1, skipped: 1, overrides: { focusMode: { code: "KeyK", primary: true, shift: true } } });
      expect(bindingFor("quickCreate")).toEqual(defaultBinding("quickCreate"));
      expect(bindingFor("focusMode")).toEqual({ code: "KeyK", primary: true, shift: true });
      expect(replaceOverrides({})).toEqual({ applied: 0, skipped: 0, overrides: {} });
      expect(stored()).toBeNull();
    });
  });

  it("drops retired selected-session shortcuts and frees their modifier combinations", () => {
    const retired = { lock: "KeyL", pin: "KeyP", archive: "KeyA", copyId: "KeyC" };
    const overrides = Object.fromEntries(Object.entries(retired).map(([id, code]) => [id, { code, primary: true, shift: true }]));
    expect(sanitizeOverrides(overrides)).toEqual({ overrides: {}, skipped: 4 });
    for (const code of Object.values(retired)) expect(validateBinding("focusMode", { code, primary: true, shift: true }, defaults)).toBeUndefined();
  });
});

/** Shortcuts carried in a configuration document from a machine of the other kind: each entry means on the host
 * reading it the keys its modifiers name there, and one those keys cannot be, or that would take another command's
 * keys, gives its command this host's default back rather than any other key. */
describe("shortcuts carried between platforms", () => {
  const sanitizeOn = (host: HostFacts, value: unknown) => sanitizeOverridesFor(value, host);

  afterEach(() => restoreAllBindings());

  it("keep a Mac's ⌃Tab keys on a Mac, and give them up for Ctrl+Tab on Windows, where they are the Windows key", () => {
    // The two switch keys swapped on a Mac, in the form a Mac has always stored them.
    const swapped = { nextConsoleSession: { code: "Tab", other: true, shift: true }, previousConsoleSession: { code: "Tab", other: true } };
    expect(sanitizeOn(MAC_HOST, swapped)).toEqual({ overrides: swapped, skipped: 0 });
    expect(matchesCommandFor("previousConsoleSession", new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true }), MAC_HOST)).toBe(false);
    expect(replaceOverridesFor(swapped, MAC_HOST).applied).toBe(2);
    expect(matchesCommandFor("previousConsoleSession", new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true }), MAC_HOST)).toBe(true);
    expect(JSON.parse(localStorage.getItem(PREFERENCE_KEYS.keyboardShortcuts)!)).toEqual(swapped);

    // On Windows both are Tab with the Windows key, which the system keeps: each command has its default, and the two
    // defaults do not clash.
    expect(replaceOverridesFor(swapped, WINDOWS_HOST)).toEqual({ applied: 0, skipped: 2, overrides: {} });
    expect(matchesCommandFor("nextConsoleSession", new KeyboardEvent("keydown", { code: "Tab", ctrlKey: true }), WINDOWS_HOST)).toBe(true);
  });

  it("read a Windows Ctrl+Tab as ⌘Tab on a Mac, which is refused there, never as ⌃Tab", () => {
    // Next moved away on Windows and Previous given Ctrl+Tab.
    const windows = { nextConsoleSession: { code: "KeyJ", primary: true, alt: true }, previousConsoleSession: { code: "Tab", primary: true } };
    expect(sanitizeOn(WINDOWS_HOST, windows)).toEqual({ overrides: windows, skipped: 0 });
    // On a Mac Previous is ⌘Tab, the system's application switcher: it keeps its own ⌃⇧Tab, and Next has ⌘⌥J.
    expect(sanitizeOn(MAC_HOST, windows)).toEqual({ overrides: { nextConsoleSession: windows.nextConsoleSession }, skipped: 1 });
  });

  it("drop an entry that would take another command's keys on this host, and move none onto another key", () => {
    // Written as the primary modifier with Tab, it is Ctrl+Tab on Windows, the switch's own default.
    expect(sanitizeOn(WINDOWS_HOST, { focusMode: { code: "Tab", primary: true } })).toEqual({ overrides: {}, skipped: 1 });
    expect(sanitizeOn(MAC_HOST, { focusMode: { code: "Tab", primary: true } })).toEqual({ overrides: {}, skipped: 1 });
    // ⌃Tab written by a Mac is the switch's default there, and the Windows key with Tab on Windows.
    expect(sanitizeOn(MAC_HOST, { newSession: { code: "Tab", other: true } })).toEqual({ overrides: {}, skipped: 1 });
    expect(sanitizeOn(WINDOWS_HOST, { newSession: { code: "Tab", other: true } })).toEqual({ overrides: {}, skipped: 1 });
  });

  it("judge a swap of the switch keys made on Windows by their keys, with no false clash", () => {
    const swapped = { nextConsoleSession: { code: "Tab", primary: true, shift: true }, previousConsoleSession: { code: "Tab", primary: true } };
    expect(sanitizeOn(WINDOWS_HOST, swapped)).toEqual({ overrides: swapped, skipped: 0 });
    expect(sanitizeOn(MAC_HOST, swapped)).toEqual({ overrides: {}, skipped: 2 });
  });

  it("keep a ⌘ or Ctrl shortcut as the platform's own modifier on either", () => {
    const moved = { focusMode: { code: "KeyK", primary: true, shift: true } };
    expect(sanitizeOn(MAC_HOST, moved)).toEqual({ overrides: moved, skipped: 0 });
    expect(sanitizeOn(WINDOWS_HOST, moved)).toEqual({ overrides: moved, skipped: 0 });
  });

  it("keep an entry written with `ctrl` as written, judged as Control on each host", () => {
    const control = { focusMode: { code: "KeyK", ctrl: true, shift: true } };
    expect(sanitizeOn(WINDOWS_HOST, control)).toEqual({ overrides: control, skipped: 0 });
    // ⌃⇧K is a terminal control key on a Mac.
    expect(sanitizeOn(MAC_HOST, control)).toEqual({ overrides: {}, skipped: 1 });
    // Equal to the switch's default on either host, so not kept.
    expect(sanitizeOn(MAC_HOST, { nextConsoleSession: { code: "Tab", ctrl: true } })).toEqual({ overrides: {}, skipped: 0 });
  });
});
