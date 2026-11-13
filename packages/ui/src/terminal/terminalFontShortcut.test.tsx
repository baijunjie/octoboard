// @vitest-environment jsdom
import { act } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { eventKey } from "../shortcuts/binding";
import { restoreAllBindings, setBinding } from "../shortcuts/store";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { matchFontSizeShortcut, useTerminalFontShortcuts } from "./terminalFontShortcut";
import {
  DEFAULT_TERMINAL_FONT_SIZE,
  MAX_TERMINAL_FONT_SIZE,
  MIN_TERMINAL_FONT_SIZE,
  resetTerminalFontSize,
  terminalFontSize,
} from "./terminalFontSize";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  restoreAllBindings();
  resetTerminalFontSize();
  document.body.replaceChildren();
});

/** What `matchFontSizeShortcut` makes of a key pressed in `where`: a terminal's input, or the page. */
function press(host: typeof MAC_HOST, where: "terminal" | "page", init: { code: string; key: string; shift?: boolean; alt?: boolean; other?: boolean }, kind: "tauri" | "browser" = "tauri") {
  const target = document.createElement("textarea");
  if (where === "terminal") {
    const terminal = document.createElement("div");
    terminal.className = "xterm";
    terminal.append(target);
    document.body.append(terminal);
  } else {
    document.body.append(target);
  }
  const apple = host.modifier === "meta";
  const event = new KeyboardEvent("keydown", {
    code: init.code,
    key: init.key,
    metaKey: apple,
    ctrlKey: !apple,
    shiftKey: init.shift,
    altKey: init.alt,
    bubbles: true,
  });
  let found: ReturnType<typeof matchFontSizeShortcut>;
  target.addEventListener("keydown", (e) => { found = matchFontSizeShortcut(e, { kind, host, appMenu: kind === "tauri" && host.os === "macos" ? {} : undefined }); });
  target.dispatchEvent(event);
  return found;
}

describe.each([["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]])("on %s", (_name, host) => {
  it("takes the modifier with = or +, - and 0 as increase, decrease and reset while a terminal has focus", () => {
    expect(press(host, "terminal", { code: "Equal", key: "=" })).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Equal", key: "+", shift: true })).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Minus", key: "-" })).toBe("decreaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Digit0", key: "0" })).toBe("resetTerminalFontSize");
  });

  it("leaves the keys to the browser's page zoom anywhere but in a terminal", () => {
    expect(press(host, "page", { code: "Equal", key: "=" })).toBeUndefined();
    expect(press(host, "page", { code: "Minus", key: "-" })).toBeUndefined();
    expect(press(host, "page", { code: "Digit0", key: "0" })).toBeUndefined();
  });

  it("takes + and - typed on another key of a layout, and no other combination of them", () => {
    expect(press(host, "terminal", { code: "Backslash", key: "+" })).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Slash", key: "-" })).toBe("decreaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Slash", key: "-", shift: true })).toBeUndefined();
    expect(press(host, "terminal", { code: "Equal", key: "=", alt: true })).toBeUndefined();
    expect(press(host, "terminal", { code: "KeyK", key: "k" })).toBeUndefined();
  });

  it("follows a command rebound by the user, and then takes the layout's characters no more", () => {
    expect(setBinding("increaseTerminalFontSize", { code: "KeyJ", primary: true, alt: true }, host)).toBeUndefined();
    expect(press(host, "terminal", eventInit(host, { code: "KeyJ", primary: true, alt: true }))).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Equal", key: "=" })).toBeUndefined();
    expect(press(host, "terminal", { code: "Backslash", key: "+" })).toBeUndefined();
  });
});

describe("the typed + and - against the keys of Back and Forward, which only the macOS desktop app has", () => {
  it("leaves a typed + to Forward in the macOS desktop app, as the key typing it is Forward's on a German layout", () => {
    expect(press(MAC_HOST, "terminal", { code: "BracketRight", key: "+" })).toBeUndefined();
    expect(press(MAC_HOST, "terminal", { code: "BracketLeft", key: "-" })).toBeUndefined();
  });

  it("leaves a typed + or - to the keys of the native menu's fixed commands in the macOS desktop app too", () => {
    expect(press(MAC_HOST, "terminal", { code: "KeyZ", key: "+" })).toBeUndefined();
    expect(press(MAC_HOST, "terminal", { code: "KeyC", key: "-" })).toBeUndefined();
  });

  it.each([
    ["the Windows desktop app", WINDOWS_HOST, "tauri"],
    ["Windows in a browser", WINDOWS_HOST, "browser"],
    ["macOS in a browser", MAC_HOST, "browser"],
  ] as const)("takes it as an increase or a decrease in %s, which has no Forward", (_name, host, kind) => {
    expect(press(host, "terminal", { code: "BracketRight", key: "+" }, kind)).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "BracketLeft", key: "-" }, kind)).toBe("decreaseTerminalFontSize");
  });
});

describe.each([["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]])("the typed + and - against the keys of other commands on %s", (_name, host) => {
  it("leaves a typed character to a command rebound onto its key, and takes it again once that is undone", () => {
    expect(press(host, "terminal", { code: "Backslash", key: "+" })).toBe("increaseTerminalFontSize");
    expect(setBinding("openSettings", { code: "Backslash", primary: true }, host)).toBeUndefined();
    expect(press(host, "terminal", { code: "Backslash", key: "+" })).toBeUndefined();
    restoreAllBindings();
    expect(press(host, "terminal", { code: "Backslash", key: "+" })).toBe("increaseTerminalFontSize");
  });

  it("takes a + typed on the key whose code is the decrease's, as a Nordic layout does, as an increase while the font size commands keep their keys", () => {
    expect(press(host, "terminal", { code: "Minus", key: "+" })).toBe("increaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Minus", key: "-" })).toBe("decreaseTerminalFontSize");
    // The key typing - on that layout.
    expect(press(host, "terminal", { code: "Slash", key: "-" })).toBe("decreaseTerminalFontSize");
  });

  it("takes a + typed on the decrease's key as nothing, never a decrease, once the increase is rebound", () => {
    expect(setBinding("increaseTerminalFontSize", { code: "KeyJ", primary: true, alt: true }, host)).toBeUndefined();
    expect(press(host, "terminal", { code: "Minus", key: "+" })).toBeUndefined();
    expect(press(host, "terminal", { code: "Minus", key: "-" })).toBe("decreaseTerminalFontSize");
  });

  it("takes a - typed on the increase's key, as Turkish Q does, as a decrease, and as nothing, never an increase, once the decrease is rebound", () => {
    expect(press(host, "terminal", { code: "Equal", key: "-" })).toBe("decreaseTerminalFontSize");
    expect(setBinding("decreaseTerminalFontSize", { code: "KeyJ", primary: true, alt: true }, host)).toBeUndefined();
    expect(press(host, "terminal", { code: "Equal", key: "-" })).toBeUndefined();
    expect(press(host, "terminal", { code: "Equal", key: "=" })).toBe("increaseTerminalFontSize");
  });

  it("stops taking the decrease's old key as a decrease once it is rebound, and takes its new keys as one", () => {
    expect(setBinding("decreaseTerminalFontSize", { code: "KeyJ", primary: true, alt: true }, host)).toBeUndefined();
    expect(press(host, "terminal", eventInit(host, { code: "KeyJ", primary: true, alt: true }))).toBe("decreaseTerminalFontSize");
    expect(press(host, "terminal", { code: "Minus", key: "-" })).toBeUndefined();
  });
});

describe.each([["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]])("useTerminalFontShortcuts and the typed +/- on %s", (_name, host) => {
  const mount = () => mountHost(<Shortcuts />, { platform: { kind: "browser", host } });

  it("increases the font size by a + on Forward's key in a browser, which has no Forward", () => {
    mount();
    expect(pressKey(host, "terminal", "BracketRight", "+").defaultPrevented).toBe(true);
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE + 1);
  });

  it("increases the font size by a + typed on the key whose code is the decrease's", () => {
    mount();
    expect(pressKey(host, "terminal", "Minus", "+").defaultPrevented).toBe(true);
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE + 1);
  });
});

describe("useTerminalFontShortcuts and the typed + on Forward's key in the macOS desktop app", () => {
  it("leaves the font size alone and the event unhandled, as the typed + is Forward's keys there", () => {
    mountHost(<Shortcuts />, { platform: { kind: "tauri", host: MAC_HOST } });
    expect(pressKey(MAC_HOST, "terminal", "BracketRight", "+").defaultPrevented).toBe(false);
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE);
  });
});

function eventInit(host: typeof MAC_HOST, binding: Parameters<typeof eventKey>[0]) {
  const { code, shiftKey, altKey } = eventKey(binding, host);
  return { code, key: "j", shift: shiftKey, alt: altKey };
}

function Shortcuts(): null {
  useTerminalFontShortcuts();
  return null;
}

/** Presses the modifier with `code` in a terminal's input, or on the page, and returns the event. */
function pressKey(host: typeof MAC_HOST, where: "terminal" | "page", code: string, key: string): KeyboardEvent {
  const target = document.createElement("textarea");
  if (where === "terminal") {
    const terminal = document.createElement("div");
    terminal.className = "xterm";
    terminal.append(target);
    document.body.append(terminal);
  } else {
    document.body.append(target);
  }
  const apple = host.modifier === "meta";
  const event = new KeyboardEvent("keydown", { code, key, metaKey: apple, ctrlKey: !apple, bubbles: true, cancelable: true });
  act(() => void target.dispatchEvent(event));
  return event;
}

describe.each([["macOS", MAC_HOST], ["Windows", WINDOWS_HOST]])("useTerminalFontShortcuts on %s", (_name, host) => {
  const mount = () => mountHost(<Shortcuts />, { platform: { kind: "browser", host } });

  it("steps the font size up and down by one and resets it to the default while a terminal has focus", () => {
    mount();
    expect(pressKey(host, "terminal", "Equal", "=").defaultPrevented).toBe(true);
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE + 1);
    pressKey(host, "terminal", "Equal", "=");
    pressKey(host, "terminal", "Minus", "-");
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE + 1);
    expect(pressKey(host, "terminal", "Digit0", "0").defaultPrevented).toBe(true);
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE);
  });

  it("holds the size within the range however long a key is held", () => {
    mount();
    for (let i = 0; i < 30; i++) pressKey(host, "terminal", "Equal", "=");
    expect(terminalFontSize.get()).toBe(MAX_TERMINAL_FONT_SIZE);
    for (let i = 0; i < 30; i++) pressKey(host, "terminal", "Minus", "-");
    expect(terminalFontSize.get()).toBe(MIN_TERMINAL_FONT_SIZE);
  });

  it("leaves the keys, and the browser's page zoom, alone outside a terminal", () => {
    mount();
    for (const [code, key] of [["Equal", "="], ["Minus", "-"], ["Digit0", "0"]]) {
      expect(pressKey(host, "page", code, key).defaultPrevented, code).toBe(false);
    }
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE);
  });

  it("follows a rebound command and no longer takes its default keys", () => {
    mount();
    expect(setBinding("decreaseTerminalFontSize", { code: "KeyJ", primary: true, alt: true }, host)).toBeUndefined();
    const { code, altKey } = eventKey({ code: "KeyJ", primary: true, alt: true }, host);
    const apple = host.modifier === "meta";
    const target = document.createElement("textarea");
    const terminal = document.createElement("div");
    terminal.className = "xterm";
    terminal.append(target);
    document.body.append(terminal);
    act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { code, key: "j", altKey, metaKey: apple, ctrlKey: !apple, bubbles: true, cancelable: true })));
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE - 1);
    pressKey(host, "terminal", "Minus", "-");
    expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE - 1);
  });
});
