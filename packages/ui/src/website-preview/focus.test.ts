// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { installPreviewFocusGuard } from "./focus";

let frame: HTMLIFrameElement;
let win: Window & typeof globalThis;
let doc: Document;
let focused: boolean;
let remove = () => {};

beforeEach(() => {
  frame = document.createElement("iframe");
  document.body.append(frame);
  win = frame.contentWindow as Window & typeof globalThis;
  doc = win.document;
  focused = false;
  vi.spyOn(doc, "hasFocus").mockImplementation(() => focused);
});
afterEach(() => {
  remove();
  remove = () => {};
  frame.remove();
  vi.restoreAllMocks();
});

function tab(target: Element, shiftKey = false, altKey = false) {
  const heard = vi.fn();
  doc.addEventListener("keydown", heard);
  target.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Tab", shiftKey, altKey, bubbles: true, cancelable: true }));
  doc.removeEventListener("keydown", heard);
  return heard.mock.calls.length > 0;
}

it("does nothing outside a frame", () => {
  const before = HTMLElement.prototype.focus;
  remove = installPreviewFocusGuard(window);
  expect(HTMLElement.prototype.focus).toBe(before);
});

it("drops a script's focus move until the frame holds focus", () => {
  remove = installPreviewFocusGuard(win);
  doc.body.innerHTML = "<button>Before</button><textarea></textarea>";
  const field = doc.querySelector("textarea")!;
  field.focus();
  expect(doc.activeElement).toBe(doc.body);

  focused = true;
  field.focus();
  expect(doc.activeElement).toBe(field);
});

it("keeps Tab and Option+Tab from a terminal, leaving other keys to it", () => {
  remove = installPreviewFocusGuard(win);
  doc.body.innerHTML = '<div class="xterm"><textarea></textarea></div><button>After</button>';
  const terminal = doc.querySelector("textarea")!;
  expect(tab(terminal)).toBe(false);
  expect(tab(terminal, true)).toBe(false);
  expect(tab(terminal, false, true)).toBe(false);
  const enter = vi.fn();
  doc.addEventListener("keydown", enter);
  terminal.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  terminal.dispatchEvent(new win.KeyboardEvent("keydown", { key: "Tab", ctrlKey: true, bubbles: true }));
  expect(enter).toHaveBeenCalledTimes(2);
});

it("keeps Tab from the application at the frame's first and last stops only", () => {
  remove = installPreviewFocusGuard(win);
  doc.body.innerHTML = `
    <div inert><button>Behind</button></div>
    <section role="dialog"><button>First</button><button>Middle</button><button>Last</button></section>
    <button disabled>Disabled</button><span tabindex="-1">Unreachable</span><button hidden>Hidden</button>
    <button style="visibility: hidden">Invisible</button><div contenteditable="false">Text</div>`;
  const [, first, middle, last] = doc.querySelectorAll("button");
  expect(tab(middle!)).toBe(true);
  expect(tab(middle!, true)).toBe(true);
  expect(tab(last!)).toBe(false);
  expect(tab(last!, true)).toBe(true);
  expect(tab(last!, false, true)).toBe(false);
  expect(tab(first!, true)).toBe(false);
  expect(tab(first!, true, true)).toBe(false);
  expect(tab(first!)).toBe(true);
});

it("leaves Tab to the application where the browser would move into the focused element", () => {
  remove = installPreviewFocusGuard(win);
  doc.body.innerHTML = '<div role="grid" tabindex="0"><div role="row"><button>Actions</button></div></div>';
  expect(tab(doc.querySelector('[role="grid"]')!)).toBe(true);
});

it("leaves Tab in a menu or a list box to it", () => {
  remove = installPreviewFocusGuard(win);
  doc.body.innerHTML = `
    <div role="menu"><div role="menuitem" tabindex="-1">Copy</div></div>
    <div role="listbox"><div role="option" tabindex="0">English</div></div>`;
  expect(tab(doc.querySelector('[role="menuitem"]')!)).toBe(true);
  expect(tab(doc.querySelector('[role="option"]')!)).toBe(true);
});

it("gives the frame focus only for the visitor's own press", () => {
  const focus = vi.spyOn(win, "focus").mockImplementation(() => {});
  remove = installPreviewFocusGuard(win);
  doc.body.dispatchEvent(new win.PointerEvent("pointerdown", { bubbles: true }));
  expect(focus).not.toHaveBeenCalled();
});

it("restores focus and Tab handling on disposal", () => {
  const before = win.HTMLElement.prototype.focus;
  remove = installPreviewFocusGuard(win);
  remove();
  remove = () => {};
  expect(win.HTMLElement.prototype.focus).toBe(before);
  doc.body.innerHTML = '<div class="xterm"><textarea></textarea></div>';
  expect(tab(doc.querySelector("textarea")!)).toBe(true);
});
