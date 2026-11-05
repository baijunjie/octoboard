// @vitest-environment jsdom
import { expect, it } from "vitest";

import { translatorFor } from "../i18n/catalog";
import type { HostFacts } from "../platform/host";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { describeBinding, type Binding } from "./binding";
import { BindingKbd } from "./ShortcutKbd";

const ALL_MODIFIERS: Binding = { code: "KeyF", primary: true, shift: true, alt: true, other: true };

/** The caps `binding` is drawn as on `host`, one text per key. */
function caps(binding: Binding, host: HostFacts): string[] {
  const container = mountHost(<BindingKbd binding={binding} />, { platform: { kind: "browser", host } });
  return [...container.querySelectorAll("kbd > *")].map((cap) => cap.textContent ?? "");
}

it("draws the modifiers as their key names on a Windows host", () => {
  expect(caps(ALL_MODIFIERS, WINDOWS_HOST)).toEqual(["Win", "Ctrl", "Alt", "Shift", "F"]);
});

it("draws the modifiers as macOS symbols on a macOS host", () => {
  expect(caps(ALL_MODIFIERS, MAC_HOST)).toEqual(["⌃", "⌥", "⇧", "⌘", "F"]);
});

it("draws the navigation keys as text only where their macOS symbols mean nothing", () => {
  expect(caps({ code: "PageUp", primary: true }, WINDOWS_HOST)).toEqual(["Ctrl", "PgUp"]);
  expect(caps({ code: "PageUp", primary: true }, MAC_HOST)).toEqual(["⌘", "⇞"]);
  expect(caps({ code: "Enter", primary: true }, WINDOWS_HOST)).toEqual(["Ctrl", "↵"]);
});

it("says the localized key names whatever the caps show", () => {
  expect(describeBinding(ALL_MODIFIERS, translatorFor("en"), WINDOWS_HOST)).toBe("Windows+Ctrl+Alt+Shift+F");
  expect(describeBinding({ code: "PageUp", primary: true }, translatorFor("de"), WINDOWS_HOST)).toBe("Strg+Bild auf");
});

it("names the combination as a screen reader should say it, in the order the caps show", () => {
  const kbd = (host: HostFacts) => mountHost(<BindingKbd binding={ALL_MODIFIERS} />, { platform: { kind: "browser", host } }).querySelector("kbd")!;
  const windows = kbd(WINDOWS_HOST);
  expect(windows.getAttribute("role")).toBe("img");
  expect(windows.getAttribute("aria-label")).toBe(describeBinding(ALL_MODIFIERS, translatorFor("en"), WINDOWS_HOST));
  expect(kbd(MAC_HOST).getAttribute("aria-label")).toBe("Control+Option+Shift+Command+F");
});

it("spaces key names apart and leaves macOS's glyphs as HeroUI sets them", () => {
  const kbd = (host: HostFacts) => mountHost(<BindingKbd binding={ALL_MODIFIERS} />, { platform: { kind: "browser", host } }).querySelector("kbd")!;
  expect(kbd(WINDOWS_HOST).hasAttribute("data-kbd-spelled")).toBe(true);
  expect(kbd(WINDOWS_HOST).className).toContain("space-x-1");
  expect(kbd(MAC_HOST).hasAttribute("data-kbd-spelled")).toBe(false);
  expect(kbd(MAC_HOST).className).not.toContain("space-x-1");
});
