// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { tap } from "../components/hint/tooltipTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";

import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { eventKey as eventKeyFor, type Binding } from "../shortcuts/binding";
import { bindingFor, restoreAllBindings, setBinding as setBindingFor } from "../shortcuts/store";
import { ShortcutsSection } from "./ShortcutsSection";
import { BROWSER_HOST, WINDOWS_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const eventKey = (binding: Binding) => eventKeyFor(binding, BROWSER_HOST);
const setBinding = (id: Parameters<typeof setBindingFor>[0], binding: Binding) => setBindingFor(id, binding, BROWSER_HOST);

const APP: PlatformAdapter = { kind: "tauri", host: BROWSER_HOST, windowChrome: { leftInset: () => 0, subscribe: () => () => {} }, appMenu: { onSettingsRequested: () => () => {}, setLabels: async () => {} } };

function mount(platform: PlatformAdapter = { kind: "browser", host: BROWSER_HOST }) {
  return mountHost(<LanguageProvider><ShortcutsSection /></LanguageProvider>, { platform });
}

afterEach(() => {
  restoreAllBindings();
  localStorage.clear();
});

// A button's accessible name, from its `aria-label` or the elements its `aria-labelledby` names.
const nameOf = (control: Element) => control.getAttribute("aria-label")
  ?? (control.getAttribute("aria-labelledby") ?? "").split(/\s+/).filter(Boolean).map((id) => document.getElementById(id)?.textContent ?? "").join(" ");
const button = (container: HTMLElement, name: string) => [...container.querySelectorAll<HTMLButtonElement>("button")].find((control) => nameOf(control) === name) ?? null;
const press = (binding: Binding, init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(binding), ...init });
  act(() => { document.body.dispatchEvent(event); });
  return event;
};
const text = (container: HTMLElement) => container.textContent ?? "";
const describedBy = (control: HTMLElement) => (control.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean)
  .map((id) => document.getElementById(id)?.textContent ?? "").join(" ");

it("lists only the commands this client has, and says the fixed ones cannot be changed", () => {
  const browser = mount();
  expect(text(browser)).toContain("Shortcut changes apply only to this browser.");
  expect(text(browser)).toContain("Toggle focus mode");
  for (const retired of ["Lock or unlock the session", "Pin or unpin the session", "Archive the session", "Copy session ID"]) expect(text(browser)).not.toContain(retired);
  expect(text(browser)).toContain("Next region of the window");
  expect(text(browser)).not.toContain("Back");
  expect(text(browser)).toContain("Open settings");
  expect(text(browser)).not.toContain("Undo");
  expect(button(browser, "Change the shortcut for Toggle focus mode")).not.toBeNull();
  expect(button(browser, "Change the shortcut for Next region of the window")).toBeNull();
  const headings = Array.from(browser.querySelectorAll("h3"));
  expect(headings.length).toBeGreaterThan(0);
  expect(headings.every((heading) => heading.closest("[data-sticky-heading]") !== null && heading.closest("section")?.getAttribute("aria-labelledby") === heading.id)).toBe(true);
});

it("edits through the displayed shortcut itself, which gives way to a prompt that is not a control while recording", () => {
  const container = mount();
  const change = button(container, "Change the shortcut for Toggle focus mode")!;
  expect(change.querySelector('[aria-label="Ctrl+Shift+F"]')).not.toBeNull();
  expect(describedBy(change)).toBe("Ctrl+Shift+F");
  expect([...container.querySelectorAll("button")].some((control) => control.textContent === "Change")).toBe(false);
  act(() => change.click());
  const prompt = [...container.querySelectorAll("kbd")].find((cap) => cap.textContent === "Press the new keys…")!;
  expect(prompt).toBeDefined();
  expect(prompt.closest("button")).toBeNull();
  expect(button(container, "Change the shortcut for Toggle focus mode")).toBeNull();
  const cancel = button(container, "Cancel changing the shortcut for Toggle focus mode")!;
  expect(document.activeElement).toBe(cancel);
  expect(describedBy(cancel)).toBe("Press the new keys…");
  press({ code: "KeyK", primary: true, shift: true });
  const updated = button(container, "Change the shortcut for Toggle focus mode")!;
  expect(updated.querySelector('[aria-label="Ctrl+Shift+K"]')).not.toBeNull();
  expect(describedBy(updated)).toBe("Ctrl+Shift+K");
  const reset = button(container, "Reset the shortcut for Toggle focus mode to its default")!;
  expect(reset.textContent).toBe("");
  expect(reset.querySelector("svg[aria-hidden=true]")).not.toBeNull();
});

it("cancels recording with the separate Cancel button and returns focus to the shortcut", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Toggle focus mode")!.click());
  const cancel = [...container.querySelectorAll<HTMLButtonElement>('button[aria-label="Cancel changing the shortcut for Toggle focus mode"]')];
  expect(cancel).toHaveLength(1);
  expect(cancel[0]!.textContent).toBe("");
  act(() => cancel[0]!.click());
  const restored = button(container, "Change the shortcut for Toggle focus mode")!;
  expect(restored.querySelector('[aria-label="Ctrl+Shift+F"]')).not.toBeNull();
  expect(document.activeElement).toBe(restored);
  expect(text(container)).not.toContain("Press the new keys…");
  act(() => restored.click());
  press({ code: "Escape" }, { key: "Escape" });
  expect(document.activeElement).toBe(button(container, "Change the shortcut for Toggle focus mode"));
});

it("marks a fixed shortcut accessibly without offering an edit control", () => {
  const container = mount();
  const row = container.querySelector('[data-settings-search-id="shortcut.nextRegion"]')!;
  expect(row).not.toBeNull();
  expect(row.querySelector('[role="img"][aria-label="Fixed"]')).not.toBeNull();
  expect(row.querySelector('[aria-label="F6"]')).not.toBeNull();
  expect(row.querySelector("button")).toBeNull();
});

it.each(["Enter", " "] as const)("starts recording on the shortcut and cancels it on Cancel with %s", (key) => {
  const container = mount();
  const change = button(container, "Change the shortcut for Toggle focus mode")!;
  act(() => change.focus());
  tap(change, key);
  const cancel = button(container, "Cancel changing the shortcut for Toggle focus mode")!;
  expect(document.activeElement).toBe(cancel);
  tap(cancel, key);
  const restored = button(container, "Change the shortcut for Toggle focus mode")!;
  expect(restored.querySelector('[aria-label="Ctrl+Shift+F"]')).not.toBeNull();
  expect(document.activeElement).toBe(restored);
  expect(bindingFor("focusMode")).toEqual({ code: "KeyF", primary: true, shift: true });
  expect(text(container)).not.toContain("Press the new keys…");
});

it("lists the font size commands in a Terminal group, between Focus and Application, each rebindable", () => {
  const browser = mount();
  const headings = Array.from(browser.querySelectorAll("h3")).map((heading) => heading.textContent);
  expect(headings.indexOf("Terminal")).toBeGreaterThan(-1);
  expect(headings.indexOf("Terminal")).toBeGreaterThan(headings.indexOf("Focus mode"));
  expect(headings.indexOf("Terminal")).toBeLessThan(headings.indexOf("Application"));
  for (const name of ["Increase terminal font size", "Decrease terminal font size", "Reset terminal font size"]) {
    expect(text(browser), name).toContain(name);
    expect(button(browser, `Change the shortcut for ${name}`), name).not.toBeNull();
  }
});

it("records a new combination for a font size command and saves it", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Increase terminal font size")!.click());
  press({ code: "KeyK", primary: true, alt: true });
  expect(bindingFor("increaseTerminalFontSize")).toEqual({ code: "KeyK", primary: true, alt: true });
  expect(button(container, "Reset the shortcut for Increase terminal font size to its default")).not.toBeNull();
});

it("lists the Find key as Find, a fixed one, not as Find in file", () => {
  const browser = mount();
  expect(text(browser)).not.toContain("Find in file");
  expect(button(browser, "Change the shortcut for Find")).toBeNull();
  const row = [...browser.querySelectorAll("*")].find((element) => element.children.length === 0 && element.textContent === "Find");
  expect(row).toBeDefined();
});

it("lists the desktop app's own commands and the native menu's fixed keys there", () => {
  const app = mount(APP);
  expect(text(app)).toContain("Shortcut changes apply only to this desktop app.");
  expect(text(app)).toContain("Open settings");
  expect(text(app)).toContain("Next console session");
  expect(text(app)).toContain("Undo");
  expect(app.querySelector('[role="img"][aria-label="Fixed"]')).not.toBeNull();
});

it("lists Open settings, which the page handles itself, and the console-session switch in the Windows app, but not the native menu's fixed keys", () => {
  const windows = mount({ kind: "tauri", host: WINDOWS_HOST });
  expect(text(windows)).toContain("Open settings");
  expect(button(windows, "Change the shortcut for Open settings")).not.toBeNull();
  expect(text(windows)).not.toContain("Undo");
  expect(text(windows)).not.toContain("Back");
  // Ctrl+Tab and Ctrl+Shift+Tab, as Windows writes them: never the Windows key, whose Tab is the system's Task View.
  expect(windows.querySelector('[aria-label="Ctrl+Tab"]')).not.toBeNull();
  expect(windows.querySelector('[aria-label="Ctrl+Shift+Tab"]')).not.toBeNull();
  act(() => button(windows, "Change the shortcut for Toggle focus mode")!.click());
  press({ code: "Tab", primary: true }, { key: "Tab" });
  expect(windows.querySelector(".text-danger")?.textContent).toBe("Already used for Next console session.");
});

it("records the next combination, saves it at once and offers Reset", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Toggle focus mode")!.click());
  expect(text(container)).toContain("Press the new keys…");
  const event = press({ code: "KeyK", primary: true, shift: true });
  expect(event.defaultPrevented).toBe(true);
  expect(bindingFor("focusMode")).toEqual({ code: "KeyK", primary: true, shift: true });
  expect(text(container)).not.toContain("Press the new keys…");
  expect(button(container, "Reset the shortcut for Toggle focus mode to its default")).not.toBeNull();
  expect(button(container, "Restore defaults") ?? [...container.querySelectorAll("button")].find((b) => b.textContent === "Restore defaults")).toBeTruthy();
});

it("says why a combination cannot be used, keeps recording and saves nothing", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Toggle focus mode")!.click());
  press({ code: "KeyN", primary: true, shift: true });
  expect(text(container)).toContain("Already used for New session with defaults.");
  expect(text(container)).toContain("Press the new keys…");
  press({ code: "KeyK" });
  expect(text(container)).toContain("Hold");
  press({ code: "ShiftLeft", shift: true });
  expect(text(container)).toContain("Press the new keys…");
  expect(bindingFor("focusMode")).toEqual({ code: "KeyF", primary: true, shift: true });
});

it("stops on Escape without anything else seeing the key, and gives the keys back to the page after", () => {
  const container = mount();
  const reached: string[] = [];
  const listener = (event: KeyboardEvent) => reached.push(event.code);
  window.addEventListener("keydown", listener, { capture: true });
  onTestFinished(() => window.removeEventListener("keydown", listener, { capture: true }));
  act(() => button(container, "Change the shortcut for Toggle focus mode")!.click());
  press({ code: "Escape" }, { key: "Escape" });
  expect(reached).toEqual([]);
  expect(text(container)).not.toContain("Press the new keys…");
  press({ code: "KeyK", primary: true, shift: true });
  expect(reached).toEqual(["KeyK"]);
  expect(bindingFor("focusMode")).toEqual({ code: "KeyF", primary: true, shift: true });
});

it("resets one command, and restores every default", () => {
  setBinding("focusMode", { code: "KeyK", primary: true, shift: true });
  setBinding("quickCreate", { code: "KeyJ", primary: true, shift: true });
  const container = mount();
  act(() => button(container, "Reset the shortcut for Toggle focus mode to its default")!.click());
  expect(bindingFor("focusMode")).toEqual({ code: "KeyF", primary: true, shift: true });
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyJ", primary: true, shift: true });
  act(() => [...container.querySelectorAll("button")].find((b) => b.textContent === "Restore defaults")!.click());
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyN", primary: true, shift: true });
  expect([...container.querySelectorAll("button")].some((b) => b.textContent === "Restore defaults")).toBe(false);
});

const FOCUS_MODE_CHANGE = "Change the shortcut for Toggle focus mode";

it("lets an unmodified Enter or Space through to the focused shortcut button, and says nothing of them", () => {
  const container = mount();
  act(() => button(container, FOCUS_MODE_CHANGE)!.click());
  for (const key of ["Enter", " "]) {
    const event = press({ code: key === " " ? "Space" : "Enter" }, { key });
    expect(event.defaultPrevented).toBe(false);
  }
  expect(text(container)).toContain("Press the new keys…");
  expect(container.querySelector(".text-danger")).toBeNull();
  // With a modifier they are keys to bind, and ⌘Enter is one that edits text.
  press({ code: "Enter", primary: true }, { key: "Enter" });
  expect(text(container)).toContain("move the caret or edit text");
});

it("ends the recording on Tab and on ⌥Tab, which move on as they always do", () => {
  for (const alt of [false, true]) {
    const container = mount();
    act(() => button(container, FOCUS_MODE_CHANGE)!.click());
    const event = press({ code: "Tab", alt }, { key: "Tab" });
    expect(event.defaultPrevented).toBe(false);
    expect(text(container)).not.toContain("Press the new keys…");
  }
});

it("shows a refusal as a field's error is, tied to the control, and speaks it again when it repeats", () => {
  fakeTimers();
  const container = mount();
  act(() => { vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS); });
  act(() => button(container, FOCUS_MODE_CHANGE)!.click());
  press({ code: "KeyN", primary: true, shift: true });
  const message = [...container.querySelectorAll("span")].find((span) => span.textContent === "Already used for New session with defaults.")!;
  expect(message.className).toContain("text-danger");
  expect(button(container, "Cancel changing the shortcut for Toggle focus mode")!.getAttribute("aria-describedby")?.split(/\s+/)).toContain(message.id);
  const spoken = () => container.querySelector("[role=status]")!.textContent!;
  const first = spoken();
  press({ code: "KeyN", primary: true, shift: true });
  expect(spoken()).not.toBe(first);
  expect(spoken().trim()).toBe(first.trim());
});

it("says a combination is held by a command this client does not list", () => {
  const container = mount();
  act(() => button(container, FOCUS_MODE_CHANGE)!.click());
  press({ code: "BracketLeft", primary: true });
  expect(text(container)).toContain("Already used for Back in the desktop app.");
});

it("keeps a row's override and says why when its default has been taken by another command", () => {
  setBinding("quickCreate", { code: "KeyK", primary: true, shift: true });
  setBinding("focusMode", { code: "KeyN", primary: true, shift: true });
  const container = mount();
  act(() => button(container, "Reset the shortcut for New session with defaults to its default")!.click());
  expect(text(container)).toContain("Already used for Toggle focus mode.");
  expect(bindingFor("quickCreate")).toEqual({ code: "KeyK", primary: true, shift: true });
});

it("moves focus to the shortcut button when a reset succeeds, and speaks it once the focus move is over", async () => {
  fakeTimers();
  setBinding("focusMode", { code: "KeyK", primary: true, shift: true });
  const container = mount();
  await act(async () => { await vi.advanceTimersByTimeAsync(FIRST_WRITE_DELAY_MS); });
  const status = () => container.querySelector("[role=status]")!.textContent!.trim();
  const reset = button(container, "Reset the shortcut for Toggle focus mode to its default")!;
  act(() => reset.focus());
  act(() => reset.click());
  expect(document.activeElement).toBe(button(container, FOCUS_MODE_CHANGE));
  expect(status()).toBe("");
  await act(async () => { await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS); });
  expect(status()).toMatch(/^Toggle focus mode is back to .*F\.$/);
});

it("leaves focus on Reset when the reset is refused", () => {
  setBinding("quickCreate", { code: "KeyK", primary: true, shift: true });
  setBinding("focusMode", { code: "KeyN", primary: true, shift: true });
  const container = mount();
  const reset = button(container, "Reset the shortcut for New session with defaults to its default")!;
  act(() => reset.focus());
  act(() => reset.click());
  expect(document.activeElement).toBe(reset);
});

// The test runtime is not an Apple platform, so ⌃ is the primary modifier and ⌘ the other one; the
// Apple-only refusals (control with a key, the system's Space and ` ) cannot be reached here.
const refusals: [string, Binding, string][] = [
  ["Escape, F6 and the context-menu key", { code: "Escape", primary: true }, "Escape, F6 and the context-menu key keep their own meaning."],
  ["the caret and editing keys", { code: "ArrowLeft", primary: true }, "With Ctrl, the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and Enter move the caret or edit text, so they cannot be used."],
  ["a key the system keeps", { code: "KeyK", other: true }, "The system keeps this combination for itself."],
  ["Tab with Ctrl and Alt", { code: "Tab", primary: true, alt: true }, "The system keeps this combination for itself."],
  ["Tab with the other modifier", { code: "Tab", other: true }, "The system keeps this combination for itself."],
  // The switch is the desktop app's, so a browser does not list it, but its Ctrl+Tab is still not another command's.
  ["Ctrl+Tab", { code: "Tab", primary: true }, "Already used for Next console session in the desktop app."],
  ["Ctrl+C", { code: "KeyC", primary: true }, "Ctrl+C interrupts the program in the terminal, so it cannot be used."],
];

it.each(refusals)("says why %s cannot be used, keeps recording and saves nothing", (_name, binding, message) => {
  const container = mount();
  act(() => button(container, FOCUS_MODE_CHANGE)!.click());
  press(binding, { key: binding.code === "Escape" ? "Escape" : "" });
  expect(container.querySelector(".text-danger")?.textContent).toBe(message);
  expect(text(container)).toContain("Press the new keys…");
  expect(bindingFor("focusMode")).toEqual({ code: "KeyF", primary: true, shift: true });
});

it("says Open settings cannot take Shift with a number or symbol key", () => {
  const container = mount(APP);
  act(() => button(container, "Change the shortcut for Open settings")!.click());
  press({ code: "Digit1", primary: true, shift: true });
  expect(container.querySelector(".text-danger")?.textContent).toBe("Open settings cannot use Shift with a number or symbol key.");
  expect(text(container)).toContain("Press the new keys…");
});
