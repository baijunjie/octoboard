// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { shiftEnterSequence } from "./terminalKeys";

const key = (name: string, modifiers: KeyboardEventInit = {}) => new KeyboardEvent("keydown", { key: name, ...modifiers });

describe("shiftEnterSequence", () => {
  it("is Escape and a carriage return for Shift+Enter, repeats included", () => {
    expect(shiftEnterSequence(key("Enter", { shiftKey: true }))).toBe("\x1b\r");
    expect(shiftEnterSequence(key("Enter", { shiftKey: true, repeat: true }))).toBe("\x1b\r");
  });

  it("does not depend on the event type", () => {
    expect(shiftEnterSequence(new KeyboardEvent("keyup", { key: "Enter", shiftKey: true }))).toBe("\x1b\r");
  });

  it.each<[string, string, KeyboardEventInit]>([
    ["plain Enter", "Enter", {}],
    ["Shift+Enter during composition", "Enter", { shiftKey: true, isComposing: true }],
    ["Shift+Enter that commits a composition (keyCode 229, not flagged as composing)", "Enter", { shiftKey: true, keyCode: 229 }],
    ["Ctrl+Shift+Enter", "Enter", { shiftKey: true, ctrlKey: true }],
    ["Alt+Shift+Enter", "Enter", { shiftKey: true, altKey: true }],
    ["Meta+Shift+Enter", "Enter", { shiftKey: true, metaKey: true }],
    ["Ctrl+Enter", "Enter", { ctrlKey: true }],
    ["Shift alone", "Shift", { shiftKey: true }],
    ["Shift+Tab", "Tab", { shiftKey: true }],
    ["Shift+letter", "A", { shiftKey: true }],
  ])("leaves %s to xterm", (_name, name, modifiers) => {
    expect(shiftEnterSequence(key(name, modifiers))).toBeUndefined();
  });
});
