// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ClipboardCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { TerminalController } from "./TerminalController";
import { TerminalEditMenu } from "./TerminalEditMenu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const { toastError } = vi.hoisted(() => ({ toastError: vi.fn() }));
vi.mock("../store", () => ({ useDaemon: () => ({ toastError }) }));
let root: Root;
let container: HTMLDivElement;
let target: HTMLDivElement;
let input: HTMLTextAreaElement;
let controller: { selectedText: string; inputSessionId?: string; selectAll: () => void; paste: ReturnType<typeof vi.fn>; focus: () => void };
let clipboard: ClipboardCapability | undefined;
let sessionId: string;
const controllerRef = { current: undefined as TerminalController | undefined };
const targetRef = createRef<HTMLDivElement>();
const render = () => act(() => root.render(
  <PlatformProvider value={{ kind: "browser", clipboard }}>
    <TerminalEditMenu key={sessionId} sessionId={sessionId} controllerRef={controllerRef} contextTargetRef={targetRef} />
  </PlatformProvider>,
));
beforeEach(() => {
  toastError.mockClear();
  container = document.body.appendChild(document.createElement("div"));
  target = document.body.appendChild(document.createElement("div"));
  input = target.appendChild(document.createElement("textarea"));
  targetRef.current = target;
  controller = {
    selectedText: "", inputSessionId: "s-1", paste: vi.fn(), focus: () => input.focus(),
    selectAll: () => { controller.selectedText = "all terminal output"; },
  };
  controllerRef.current = controller as unknown as TerminalController;
  clipboard = { writeText: vi.fn(async () => {}), readText: vi.fn(async () => "clipboard text") };
  sessionId = "s-1";
  root = createRoot(container);
  render();
});
afterEach(() => {
  act(() => root.unmount());
  document.body.replaceChildren();
});
const item = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === name)!;
async function open() {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 });
  await act(async () => { input.dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true);
}
async function choose(name: string) {
  await act(async () => {
    const entry = item(name);
    entry.focus();
    entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    entry.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
}

it("opens on the hidden input without a visible trigger or clipboard read", async () => {
  expect(container.querySelector("button:not([hidden])")).toBeNull();
  const native = vi.fn();
  input.addEventListener("contextmenu", native);
  await open();
  expect(native).not.toHaveBeenCalled();
  expect(clipboard!.readText).not.toHaveBeenCalled();
  expect(item("Copy").getAttribute("aria-disabled")).toBe("true");
  expect(item("Paste").getAttribute("aria-disabled")).not.toBe("true");
});

it("selects terminal output, then copies the selection captured at opening", async () => {
  await open();
  await choose("Select all");
  await open();
  controller.selectedText = "changed after menu opened";
  await choose("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledWith("all terminal output");
});

it("reads the clipboard only for Paste and sends it to the captured session", async () => {
  await open();
  await choose("Paste");
  expect(clipboard!.readText).toHaveBeenCalledOnce();
  expect(controller.paste).toHaveBeenCalledWith("clipboard text", "s-1");
});

it("dismisses on session switch and never redirects a pending clipboard read", async () => {
  let finish!: (text: string) => void;
  clipboard!.readText = vi.fn(() => new Promise<string>((resolve) => { finish = resolve; }));
  await open();
  await choose("Paste");
  sessionId = "s-2";
  controller.inputSessionId = "s-2";
  render();
  expect(document.querySelector("[role=menu]")).toBeNull();
  await act(async () => finish("old clipboard"));
  expect(controller.paste).not.toHaveBeenCalled();
});

it.each(["read", "input", "clipboard"])("disables Paste without %s", async (missing) => {
  if (missing === "read") clipboard!.readText = undefined;
  if (missing === "input") controller.inputSessionId = undefined;
  if (missing === "clipboard") clipboard = undefined;
  render();
  await open();
  expect(item("Paste").getAttribute("aria-disabled")).toBe("true");
});

it.each(["Copy", "Paste"])("reports %s failures with localized text", async (action) => {
  controller.selectedText = "selection";
  clipboard!.writeText = vi.fn(async () => { throw new Error("raw system error"); });
  clipboard!.readText = vi.fn(async () => { throw new Error("raw system error"); });
  await open();
  await choose(action);
  expect(toastError).toHaveBeenCalledWith(expect.objectContaining({
    message: action === "Copy" ? "Could not copy the selection" : "Could not paste from the clipboard",
    recovery: "clipboard",
  }));
});

it.each([
  { key: "ContextMenu" },
  { key: "F10", shiftKey: true },
])("opens with the keyboard gesture $key before xterm receives it", async (gesture) => {
  const terminalKey = vi.fn();
  input.addEventListener("keydown", terminalKey);
  input.focus();
  const event = new KeyboardEvent("keydown", { ...gesture, bubbles: true, cancelable: true });
  await act(async () => { input.dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true);
  expect(terminalKey).not.toHaveBeenCalled();
  expect(document.querySelector("[role=menu]")).not.toBeNull();
});

it("intercepts only a secondary press, leaving ordinary terminal mouse input alone", () => {
  const terminalMouse = vi.fn();
  input.addEventListener("mousedown", terminalMouse);
  const primary = new MouseEvent("mousedown", { button: 0, bubbles: true, cancelable: true });
  input.dispatchEvent(primary);
  expect(primary.defaultPrevented).toBe(false);
  expect(terminalMouse).toHaveBeenCalledOnce();
  const secondary = new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true });
  input.dispatchEvent(secondary);
  expect(secondary.defaultPrevented).toBe(true);
  expect(terminalMouse).toHaveBeenCalledOnce();
});
