// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";

import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { eventKey, type Binding } from "../shortcuts/binding";
import { bindingFor, restoreAllBindings, setBinding } from "../shortcuts/store";
import { ShortcutsSection } from "./ShortcutsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const APP: PlatformAdapter = { kind: "tauri", windowChrome: { leftInset: () => 0, subscribe: () => () => {} }, appMenu: { onSettingsRequested: () => () => {}, setLabels: async () => {} } };

function mount(platform: PlatformAdapter = { kind: "browser" }) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<PlatformProvider value={platform}><LanguageProvider><ShortcutsSection /></LanguageProvider></PlatformProvider>));
  return container;
}

afterEach(() => {
  restoreAllBindings();
  localStorage.clear();
});

const button = (container: HTMLElement, name: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`);
const press = (binding: Binding, init: KeyboardEventInit = {}) => {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...eventKey(binding), ...init });
  act(() => { document.body.dispatchEvent(event); });
  return event;
};
const text = (container: HTMLElement) => container.textContent ?? "";

it("lists only the commands this client has, and says the fixed ones cannot be changed", () => {
  const browser = mount();
  expect(text(browser)).toContain("Toggle focus mode");
  expect(text(browser)).toContain("Next region of the window");
  expect(text(browser)).not.toContain("Back");
  expect(text(browser)).not.toContain("Open settings");
  expect(text(browser)).not.toContain("Undo");
  expect(button(browser, "Change the shortcut for Toggle focus mode")).not.toBeNull();
  expect(button(browser, "Change the shortcut for Next region of the window")).toBeNull();
  const headings = Array.from(browser.querySelectorAll("h3"));
  expect(headings.length).toBeGreaterThan(0);
  expect(headings.every((heading) => heading.hasAttribute("data-sticky-heading") && heading.closest("section")?.getAttribute("aria-labelledby") === heading.id)).toBe(true);
});

it("lists the desktop app's own commands and the native menu's fixed keys there", () => {
  const app = mount(APP);
  expect(text(app)).toContain("Open settings");
  expect(text(app)).toContain("Next console session");
  expect(text(app)).toContain("Undo");
  expect(text(app)).toContain("Fixed");
});

it("records the next combination, saves it at once and offers Reset", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Lock or unlock the session")!.click());
  expect(text(container)).toContain("Press the new keys…");
  const event = press({ code: "KeyK", primary: true, shift: true });
  expect(event.defaultPrevented).toBe(true);
  expect(bindingFor("lock")).toEqual({ code: "KeyK", primary: true, shift: true });
  expect(text(container)).not.toContain("Press the new keys…");
  expect(button(container, "Reset the shortcut for Lock or unlock the session to its default")).not.toBeNull();
  expect(button(container, "Restore defaults") ?? [...container.querySelectorAll("button")].find((b) => b.textContent === "Restore defaults")).toBeTruthy();
});

it("says why a combination cannot be used, keeps recording and saves nothing", () => {
  const container = mount();
  act(() => button(container, "Change the shortcut for Lock or unlock the session")!.click());
  press({ code: "KeyP", primary: true, shift: true });
  expect(text(container)).toContain("Already used for Pin or unpin the session.");
  expect(text(container)).toContain("Press the new keys…");
  press({ code: "KeyK" });
  expect(text(container)).toContain("Hold");
  press({ code: "ShiftLeft", shift: true });
  expect(text(container)).toContain("Press the new keys…");
  expect(bindingFor("lock")).toEqual({ code: "KeyL", primary: true, shift: true });
});

it("stops on Escape without anything else seeing the key, and gives the keys back to the page after", () => {
  const container = mount();
  const reached: string[] = [];
  const listener = (event: KeyboardEvent) => reached.push(event.code);
  window.addEventListener("keydown", listener, { capture: true });
  onTestFinished(() => window.removeEventListener("keydown", listener, { capture: true }));
  act(() => button(container, "Change the shortcut for Lock or unlock the session")!.click());
  press({ code: "Escape" }, { key: "Escape" });
  expect(reached).toEqual([]);
  expect(text(container)).not.toContain("Press the new keys…");
  press({ code: "KeyK", primary: true, shift: true });
  expect(reached).toEqual(["KeyK"]);
  expect(bindingFor("lock")).toEqual({ code: "KeyL", primary: true, shift: true });
});

it("resets one command, and restores every default", () => {
  setBinding("lock", { code: "KeyK", primary: true, shift: true });
  setBinding("pin", { code: "KeyJ", primary: true, shift: true });
  const container = mount();
  act(() => button(container, "Reset the shortcut for Lock or unlock the session to its default")!.click());
  expect(bindingFor("lock")).toEqual({ code: "KeyL", primary: true, shift: true });
  expect(bindingFor("pin")).toEqual({ code: "KeyJ", primary: true, shift: true });
  act(() => [...container.querySelectorAll("button")].find((b) => b.textContent === "Restore defaults")!.click());
  expect(bindingFor("pin")).toEqual({ code: "KeyP", primary: true, shift: true });
  expect([...container.querySelectorAll("button")].some((b) => b.textContent === "Restore defaults")).toBe(false);
});

const LOCK_CHANGE = "Change the shortcut for Lock or unlock the session";

it("lets an unmodified Enter or Space through to the focused Cancel, and says nothing of them", () => {
  const container = mount();
  act(() => button(container, LOCK_CHANGE)!.click());
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
    act(() => button(container, LOCK_CHANGE)!.click());
    const event = press({ code: "Tab", alt }, { key: "Tab" });
    expect(event.defaultPrevented).toBe(false);
    expect(text(container)).not.toContain("Press the new keys…");
  }
});

it("shows a refusal as a field's error is, tied to the control, and speaks it again when it repeats", async () => {
  const container = mount();
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 150)); });
  act(() => button(container, LOCK_CHANGE)!.click());
  press({ code: "KeyP", primary: true, shift: true });
  const message = [...container.querySelectorAll("span")].find((span) => span.textContent === "Already used for Pin or unpin the session.")!;
  expect(message.className).toContain("text-danger");
  expect(button(container, "Cancel changing the shortcut for Lock or unlock the session")!.getAttribute("aria-describedby")).toBe(message.id);
  const spoken = () => container.querySelector("[role=status]")!.textContent!;
  const first = spoken();
  press({ code: "KeyP", primary: true, shift: true });
  expect(spoken()).not.toBe(first);
  expect(spoken().trim()).toBe(first.trim());
});

it("says a combination is held by a command this client does not list", () => {
  const container = mount();
  act(() => button(container, LOCK_CHANGE)!.click());
  press({ code: "BracketLeft", primary: true });
  expect(text(container)).toContain("Already used for Back in the desktop app.");
});

it("keeps a row's override and says why when its default has been taken by another command", () => {
  setBinding("pin", { code: "KeyK", primary: true, shift: true });
  setBinding("lock", { code: "KeyP", primary: true, shift: true });
  const container = mount();
  act(() => button(container, "Reset the shortcut for Pin or unpin the session to its default")!.click());
  expect(text(container)).toContain("Already used for Lock or unlock the session.");
  expect(bindingFor("pin")).toEqual({ code: "KeyK", primary: true, shift: true });
});

it("moves focus to Change only when a reset succeeds, and speaks it once the focus move is over", async () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  setBinding("lock", { code: "KeyK", primary: true, shift: true });
  const container = mount();
  await act(async () => { await vi.advanceTimersByTimeAsync(200); });
  const status = () => container.querySelector("[role=status]")!.textContent!.trim();
  const reset = button(container, "Reset the shortcut for Lock or unlock the session to its default")!;
  act(() => reset.focus());
  act(() => reset.click());
  expect(document.activeElement).toBe(button(container, LOCK_CHANGE));
  expect(status()).toBe("");
  await act(async () => { await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS + 10); });
  expect(status()).toMatch(/^Lock or unlock the session is back to .*L\.$/);
});

it("leaves focus on Reset when the reset is refused", () => {
  setBinding("pin", { code: "KeyK", primary: true, shift: true });
  setBinding("lock", { code: "KeyP", primary: true, shift: true });
  const container = mount();
  const reset = button(container, "Reset the shortcut for Pin or unpin the session to its default")!;
  act(() => reset.focus());
  act(() => reset.click());
  expect(document.activeElement).toBe(reset);
});
