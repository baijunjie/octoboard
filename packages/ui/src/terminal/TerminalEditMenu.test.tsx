// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { ClipboardCapability } from "../platform";
import type { TerminalController } from "./TerminalController";
import { TerminalEditMenu } from "./TerminalEditMenu";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { BROWSER_HOST } from "../test/hosts";
import { testDaemon } from "../test/testDaemon";
import { TestHost } from "../test/TestHost";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root;
let container: HTMLDivElement;
let target: HTMLDivElement;
let input: HTMLTextAreaElement;
let controller: { selectedText: string; programCopiedText?: string; mouseReporting?: boolean; inputSessionId?: string; selectAll: () => void; pathLinkAt: ReturnType<typeof vi.fn>; revealPathLink: ReturnType<typeof vi.fn>; openPathLinkInDefaultApplication: ReturnType<typeof vi.fn>; paste: ReturnType<typeof vi.fn>; pasteImage: ReturnType<typeof vi.fn>; focus: () => void };
let clipboard: ClipboardCapability | undefined;
let sessionId: string;
let isLocal: boolean | undefined;
const controllerRef = { current: undefined as TerminalController | undefined };
const targetRef = createRef<HTMLDivElement>();
const render = () => act(() => root.render(
  <TestHost daemon={testDaemon({ isLocal })} platform={{ kind: "browser", host: BROWSER_HOST, clipboard }}>
    <TerminalEditMenu key={sessionId} sessionId={sessionId} controllerRef={controllerRef} contextTargetRef={targetRef} />
  </TestHost>,
));
beforeEach(() => {
  container = document.body.appendChild(document.createElement("div"));
  target = document.body.appendChild(document.createElement("div"));
  input = target.appendChild(document.createElement("textarea"));
  targetRef.current = target;
  controller = {
    selectedText: "", inputSessionId: "s-1", paste: vi.fn(), pasteImage: vi.fn(), pathLinkAt: vi.fn(), revealPathLink: vi.fn(async () => {}), openPathLinkInDefaultApplication: vi.fn(async () => {}), focus: () => input.focus(),
    selectAll: () => { controller.selectedText = "all terminal output"; },
  };
  controllerRef.current = controller as unknown as TerminalController;
  clipboard = { writeText: vi.fn(async () => {}), readText: vi.fn(async () => "clipboard text") };
  sessionId = "s-1";
  isLocal = true;
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

it.each([
  { mouseReporting: true, programCopiedText: undefined, hint: true },
  { mouseReporting: true, programCopiedText: "program copy", hint: false },
  { mouseReporting: false, programCopiedText: undefined, hint: false },
])("says how to select in the terminal only when a program's drag left nothing to copy (%o)", async ({ mouseReporting, programCopiedText, hint }) => {
  controller.mouseReporting = mouseReporting;
  controller.programCopiedText = programCopiedText;
  await open();
  const copy = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent?.startsWith("Copy"))!;
  expect(/(Option|Shift)-drag to select/.test(copy.textContent ?? "")).toBe(hint);
});

it("selects terminal output, then copies the selection captured at opening", async () => {
  await open();
  await choose("Select all");
  await open();
  controller.selectedText = "changed after menu opened";
  await choose("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledWith("all terminal output");
});

it.each([
  { selected: "", expected: "program copy" },
  { selected: "terminal selection", expected: "terminal selection" },
])("copies what the program copied only while the terminal has nothing selected ($expected)", async ({ selected, expected }) => {
  controller.selectedText = selected;
  controller.programCopiedText = "program copy";
  await open();
  expect(item("Copy").getAttribute("aria-disabled")).not.toBe("true");
  await choose("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledWith(expected);
});

it("reads the clipboard only for Paste and sends it to the captured session", async () => {
  await open();
  await choose("Paste");
  expect(clipboard!.readText).toHaveBeenCalledOnce();
  expect(controller.paste).toHaveBeenCalledWith("clipboard text", "s-1");
});

it("pastes the text and reads no image when the clipboard holds text", async () => {
  const readImage = vi.fn(async () => new Blob(["x"], { type: "image/png" }));
  clipboard = { ...clipboard!, readImage };
  render();
  await open();
  await choose("Paste");
  expect(readImage).not.toHaveBeenCalled();
  expect(controller.pasteImage).not.toHaveBeenCalled();
  expect(controller.paste).toHaveBeenCalledExactlyOnceWith("clipboard text", "s-1");
});

it("leaves a clipboard with no text to the controller, for the captured session, reading no image itself", async () => {
  const readImage = vi.fn(async () => new Blob(["x"], { type: "image/png" }));
  clipboard = { writeText: vi.fn(async () => {}), readText: vi.fn(async () => ""), readImage };
  render();
  await open();
  await choose("Paste");
  // The controller reads the image itself, and only if it takes images.
  expect(readImage).not.toHaveBeenCalled();
  expect(controller.pasteImage).toHaveBeenCalledExactlyOnceWith("s-1");
  expect(controller.paste).not.toHaveBeenCalled();
});

it("pastes no image into a session the menu was dismissed from while the text was read", async () => {
  let finish!: (text: string) => void;
  const readImage = vi.fn(async () => new Blob(["x"], { type: "image/png" }));
  clipboard = { writeText: vi.fn(async () => {}), readText: vi.fn(() => new Promise<string>((resolve) => { finish = resolve; })), readImage };
  render();
  await open();
  await choose("Paste");
  sessionId = "s-2";
  controller.inputSessionId = "s-2";
  render();
  await act(async () => finish(""));
  expect(readImage).not.toHaveBeenCalled();
  expect(controller.pasteImage).not.toHaveBeenCalled();
  expect(controller.paste).not.toHaveBeenCalled();
});

it("reads no clipboard for a Paste chosen after the controller stopped taking input", async () => {
  await open();
  controller.inputSessionId = undefined;
  await choose("Paste");
  expect(clipboard!.readText).not.toHaveBeenCalled();
  expect(controller.paste).not.toHaveBeenCalled();
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

it.each([
  ["read", "Clipboard unavailable"],
  ["input", ""],
  ["clipboard", "Clipboard unavailable"],
])("disables Paste without %s, giving a reason only for the clipboard", async (missing, reason) => {
  if (missing === "read") clipboard!.readText = undefined;
  if (missing === "input") controller.inputSessionId = undefined;
  if (missing === "clipboard") clipboard = undefined;
  render();
  await open();
  expect(item(`Paste${reason}`).getAttribute("aria-disabled")).toBe("true");
});

it("copies the selection through the document without a clipboard, keeping Copy enabled", async () => {
  const copied: string[] = [];
  const execCommand = vi.fn(() => copied.push(document.getSelection()!.toString()) > 0);
  Object.assign(document, { execCommand });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  clipboard = undefined;
  controller.selectedText = "selection";
  render();
  await open();
  expect(item("Copy").getAttribute("aria-disabled")).not.toBe("true");
  await choose("Copy");
  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(copied).toEqual(["selection"]);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it.each(["Copy", "Paste"])("reports %s failures with localized text", async (action) => {
  controller.selectedText = "selection";
  clipboard!.writeText = vi.fn(async () => { throw new Error("raw system error"); });
  clipboard!.readText = vi.fn(async () => { throw new Error("raw system error"); });
  await open();
  await choose(action);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({
    // A failed copy is titled with what was not copied and says why; a failed paste likewise, with its own way out.
    title: action === "Copy" ? "Could not copy the selection" : "Could not paste",
    message: action === "Copy" ? "The clipboard could not be written to." : "The clipboard could not be read.",
    classification: { kind: "actionable", recovery: action === "Copy" ? "clipboardTerminalCopy" : "clipboardPaste" },
  });
  errorAlerts.acknowledge();
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

const closeMenu = () => act(() => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });

const entriesOf = () => [...document.querySelectorAll<HTMLElement>("[role=menuitem], [role=separator]")].map((entry) => entry.getAttribute("role") === "separator" ? "-" : entry.textContent);

it("offers Open in first, then a separator, over a file path link, with File manager and Default application", async () => {
  await open();
  expect(item("Open in")).toBeUndefined();
  closeMenu();
  controller.pathLinkAt.mockReturnValue({ path: "/r/a.ts", kind: "file" });
  await open();
  expect(controller.pathLinkAt).toHaveBeenLastCalledWith(30, 40);
  expect(entriesOf()).toEqual(["Open in", "-", "Copy", "Paste", "Select all"]);
  await act(async () => { item("Open in").click(); });
  expect(entriesOf().slice(-2)).toEqual(["File manager", "Default application"]);
  await choose("File manager");
  expect(controller.revealPathLink).toHaveBeenCalledExactlyOnceWith({ path: "/r/a.ts", kind: "file" });
  expect(controller.openPathLinkInDefaultApplication).not.toHaveBeenCalled();
});

it("opens a file path link in the default application from Open in", async () => {
  controller.pathLinkAt.mockReturnValue({ path: "/r/a.ts", kind: "file" });
  await open();
  await act(async () => { item("Open in").click(); });
  await choose("Default application");
  expect(controller.openPathLinkInDefaultApplication).toHaveBeenCalledExactlyOnceWith({ path: "/r/a.ts", kind: "file" });
});

it("offers a folder link and a file the system could launch File manager alone", async () => {
  for (const link of [{ path: "/r/dir", kind: "directory" }, { path: "/r/run.sh", kind: "file", viewerOnly: true }]) {
    controller.pathLinkAt.mockReturnValue(link);
    await open();
    await act(async () => { item("Open in").click(); });
    expect(entriesOf().slice(-1)).toEqual(["File manager"]);
    expect(item("Default application")).toBeUndefined();
    closeMenu();
    closeMenu();
  }
});

it("offers no Open in to a menu opened with the keyboard, whatever was hovered", async () => {
  controller.pathLinkAt.mockReturnValue({ path: "/r/a.ts", kind: "file" });
  await act(async () => { input.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true })); });
  expect(item("Copy")).toBeDefined();
  expect(item("Open in")).toBeUndefined();
});

it("offers no Open in in the window of a daemon elsewhere", async () => {
  isLocal = false;
  render();
  controller.pathLinkAt.mockReturnValue({ path: "/r/a.ts", kind: "file" });
  await open();
  expect(item("Copy")).toBeDefined();
  expect(item("Open in")).toBeUndefined();
});

it("focuses the terminal when the menu opens, and takes focus into the menu when it is right-clicked open again", async () => {
  const focusTerminal = vi.spyOn(controller, "focus");
  await open();
  expect(focusTerminal).toHaveBeenCalledOnce();
  // What the inert terminal does to the focus in a browser.
  await act(async () => { (document.activeElement as HTMLElement).blur(); });
  // The press lands on <body> and closes the menu, and the `contextmenu` event that follows opens it again; both are batched
  // here, where a browser flushes between them.
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 30, clientY: 40 });
  await act(async () => {
    document.body.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true }));
    input.dispatchEvent(event);
  });
  expect(event.defaultPrevented).toBe(true);
  // The menu itself, as a pointer-opened menu focuses it: the reopened content took focus anew.
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
  await act(async () => { document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); });
  expect(document.querySelector("[role=menu]")).toBeNull();
});
