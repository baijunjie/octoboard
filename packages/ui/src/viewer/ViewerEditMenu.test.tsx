// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import type { ClipboardCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import { BROWSER_HOST } from "../test/hosts";
import { FIND_REGION } from "./find/findDom";
import { ViewerEditMenu } from "./ViewerEditMenu";
import { chooseItem, closeMenu, menuEntries, menuItem, recordSelections, rightClick } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SOURCE = "# Title\n\nfirst\n\nsecond\n";

let root: Root;
let container: HTMLDivElement;
let clipboard: ClipboardCapability | undefined;
let selectAll: ReturnType<typeof vi.fn<(region: HTMLElement) => boolean>>;
let kind: "browser" | "tauri";
const regionRef = createRef<HTMLDivElement>();

function render(props: { source?: string } = {}) {
  act(() => root.render(
    <PlatformProvider value={{ kind, host: BROWSER_HOST, clipboard }}>
      <div ref={regionRef} tabIndex={0} {...FIND_REGION}>
        <p id="first">first</p>
        <p id="second">second</p>
        <p><a id="link" href="https://example.com/">link</a> <button id="inner" type="button">inner</button> <img id="image" alt="" src="data:image/png;base64,AAAA" /></p>
      </div>
      <ViewerEditMenu region={regionRef} selectAll={selectAll} source={props.source} />
    </PlatformProvider>,
  ));
}

beforeEach(() => {
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  clipboard = { writeText: vi.fn(async () => {}), readText: vi.fn(async () => "") };
  selectAll = vi.fn(() => true);
  kind = "browser";
  document.getSelection()!.removeAllRanges();
  render();
});
afterEach(() => {
  act(() => root.unmount());
  document.body.replaceChildren();
});

const region = () => regionRef.current!;
const select = (id: string) => document.getSelection()!.selectAllChildren(document.getElementById(id)!);

async function open(target: Element = region()) {
  expect((await rightClick(target)).defaultPrevented).toBe(true);
}

it("offers only Select All while nothing of the surface is selected", async () => {
  await open();
  expect(menuEntries()).toEqual(["Select all"]);
});

it("offers Copy, then Select All, while text of the surface is selected", async () => {
  select("first");
  await open();
  expect(menuEntries()).toEqual(["Copy", "Select all"]);
});

it("offers only Select All for a selection outside the surface", async () => {
  const outside = document.body.appendChild(document.createElement("p"));
  outside.textContent = "elsewhere";
  document.getSelection()!.selectAllChildren(outside);
  await open();
  expect(menuEntries()).toEqual(["Select all"]);
});

it("offers only Select All for a collapsed selection in the surface", async () => {
  document.getSelection()!.setBaseAndExtent(document.getElementById("first")!.firstChild!, 2, document.getElementById("first")!.firstChild!, 2);
  await open();
  expect(menuEntries()).toEqual(["Select all"]);
});

it("reads the selection each time it opens, not once", async () => {
  select("first");
  await open();
  expect(menuEntries()).toEqual(["Copy", "Select all"]);
  await closeMenu();
  document.getSelection()!.removeAllRanges();
  await open();
  expect(menuEntries()).toEqual(["Select all"]);
});

it("copies the selected text", async () => {
  select("second");
  await open();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith("second");
});

it("copies the selection captured when the menu opened, not a later one", async () => {
  select("first");
  await open();
  select("second");
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith("first");
});

it("copies the Markdown source when the whole document is selected, and the selected text for a part of it", async () => {
  render({ source: SOURCE });
  document.getSelection()!.selectAllChildren(region());
  await open();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenLastCalledWith(SOURCE);

  await closeMenu();
  select("second");
  await open();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenLastCalledWith("second");
});

it("copies the whole selected text, not a source, where the surface has none", async () => {
  document.getSelection()!.selectAllChildren(region());
  await open();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledOnce();
  expect(clipboard!.writeText).toHaveBeenCalledWith(expect.stringContaining("first"));
  expect(clipboard!.writeText).not.toHaveBeenCalledWith(SOURCE);
});

it("hands the surface itself to what Select All does there, once the menu has closed", async () => {
  let menuShown: boolean | undefined;
  selectAll.mockImplementation(() => {
    menuShown = document.querySelector("[role=menu]") !== null;
    return true;
  });
  await open();
  await chooseItem("Select all");
  expect(selectAll).toHaveBeenCalledExactlyOnceWith(region());
  expect(menuShown).toBe(false);
});

// jsdom runs no exit animation, so the menu closes as it does under reduced motion: before its own focus return,
// with focus on <body>. WebKit clears a selection set while the surface is not focused.
it.each(["Escape", "Select all"])("has focus on the surface when the menu closes by %s and the selection is set", async (how) => {
  select("first");
  const focusAt = recordSelections(() => document.activeElement);
  selectAll.mockImplementation(() => {
    document.getSelection()!.selectAllChildren(region());
    return true;
  });
  await open();
  if (how === "Escape") await closeMenu();
  else await chooseItem("Select all");
  expect(focusAt).toEqual([region()]);
});

it("reports a clipboard failure as a copy error with a localized title", async () => {
  clipboard!.writeText = vi.fn(async () => { throw new Error("raw system error"); });
  render();
  select("first");
  await open();
  await chooseItem("Copy");
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({
    message: "Could not copy the selection",
    classification: { kind: "actionable", recovery: "clipboardCopy" },
    title: "Could not copy",
  });
  errorAlerts.acknowledge();
});

it("gives focus back to the surface when the menu closes", async () => {
  await open();
  expect(document.activeElement).not.toBe(region());
  await closeMenu();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(document.activeElement).toBe(region());
});

// jsdom drops the selection on any `focus()`, which a browser does not do to a selection outside
// the focused element, so only the press itself and what the menu captured can be checked here.
it("keeps the existing selection through a secondary press, and offers it to Copy", async () => {
  select("first");
  const press = new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true });
  region().dispatchEvent(press);
  // A prevented press leaves the browser's own selection handling, which would collapse it, out of it.
  expect(press.defaultPrevented).toBe(true);
  expect(document.getSelection()!.toString()).toBe("first");
  await open();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith("first");
});

it("leaves an ordinary press to the browser", () => {
  const press = new MouseEvent("mousedown", { button: 0, bubbles: true, cancelable: true });
  region().dispatchEvent(press);
  expect(press.defaultPrevented).toBe(false);
});

it("replaces the browser's own menu, and shows no visible trigger", async () => {
  expect(container.querySelector("button:not([hidden]):not(#inner)")).toBeNull();
  await open();
  expect(document.querySelector("[role=menu]")).not.toBeNull();
});

it.each([
  { key: "ContextMenu" },
  { key: "F10", shiftKey: true },
])("opens with the keyboard gesture $key on the focused surface", async (gesture) => {
  region().focus();
  select("first");
  const event = new KeyboardEvent("keydown", { ...gesture, bubbles: true, cancelable: true });
  await act(async () => { region().dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true);
  expect(menuEntries()).toEqual(["Copy", "Select all"]);
});

it("gives focus back to the control the menu was opened on, not to the surface", async () => {
  const inner = document.getElementById("inner")!;
  inner.focus();
  const event = new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true });
  await act(async () => { inner.dispatchEvent(event); });
  expect(event.defaultPrevented).toBe(true);
  expect(document.querySelector("[role=menu]")).not.toBeNull();
  await closeMenu();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(document.activeElement).toBe(inner);
});

it("leaves the selection to the browser's own Select All where the surface's declines", async () => {
  const execCommand = vi.fn(() => true);
  Object.defineProperty(document, "execCommand", { value: execCommand, configurable: true });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  selectAll.mockReturnValue(false);
  await open();
  await chooseItem("Select all");
  expect(selectAll).toHaveBeenCalledExactlyOnceWith(region());
  expect(execCommand).toHaveBeenCalledExactlyOnceWith("selectAll");
  expect(document.activeElement).toBe(region());
});

it("does not run the browser's Select All where the surface's took it", async () => {
  const execCommand = vi.fn(() => true);
  Object.defineProperty(document, "execCommand", { value: execCommand, configurable: true });
  onTestFinished(() => { Reflect.deleteProperty(document, "execCommand"); });
  await open();
  await chooseItem("Select all");
  expect(execCommand).not.toHaveBeenCalled();
});

// jsdom drops the selection on any `focus()`, as WebKit does when focus moves into a menu, and drops it again as the
// menu hands focus back after it is restored, so the selection is read as it is set.
it.each(["Escape", "Copy"])("puts the selection back when the menu closes by %s", async (how) => {
  select("first");
  const set = recordSelections();
  await open();
  expect(document.getSelection()!.toString()).toBe("");
  if (how === "Copy") await chooseItem("Copy");
  else await closeMenu();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(set).toEqual(["first"]);
});

it("does not put the old selection back over a Select All", async () => {
  select("first");
  const set = recordSelections();
  await open();
  await chooseItem("Select all");
  expect(set).toEqual([]);
});

// What a browser does for a second right-click on the open menu's surface: the surface is inert behind the menu, so
// the press lands on <body> and closes the menu, and the `contextmenu` event that follows opens it again. Both events are
// batched here, where a browser flushes between them.
async function rightClickAgain() {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
  await act(async () => {
    document.body.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true }));
    region().dispatchEvent(event);
  });
  expect(event.defaultPrevented).toBe(true);
}

it("keeps the selection, and takes focus into the menu, when it is right-clicked open again before it has gone", async () => {
  select("first");
  const set = recordSelections();
  await open();
  // What the inert surface does to the focus in a browser.
  await act(async () => { (document.activeElement as HTMLElement).blur(); });
  await rightClickAgain();
  // The menu itself, as a pointer-opened menu focuses it: the reopened content took focus anew.
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
  await closeMenu();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(set).toEqual(["first"]);
});

it("offers Copy again on a reopen where the browser cleared the selection as focus entered the menu", async () => {
  select("first");
  await open();
  document.getSelection()!.removeAllRanges();
  await rightClickAgain();
  expect(menuEntries()).toEqual(["Copy", "Select all"]);
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith("first");
});

it("copies the text the first open found on a reopen, not the word the browser selected under the second right-click", async () => {
  select("first");
  const set = recordSelections();
  await open();
  select("second");
  await rightClickAgain();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith("first");
  expect(document.querySelector("[role=menu]")).toBeNull();
  // The kept ends are back, in place of the word.
  expect(set.at(-1)).toBe("first");
});

it("copies the Markdown source on a reopen of a whole-document selection that the browser has cleared", async () => {
  render({ source: SOURCE });
  document.getSelection()!.selectAllChildren(region());
  await open();
  document.getSelection()!.removeAllRanges();
  await rightClickAgain();
  await chooseItem("Copy");
  expect(clipboard!.writeText).toHaveBeenCalledExactlyOnceWith(SOURCE);
});

it("leaves a caret elsewhere alone when the menu closes with focus in an editable element", async () => {
  const input = document.body.appendChild(document.createElement("input"));
  select("first");
  const set = recordSelections();
  await open();
  await act(async () => { input.focus(); });
  // Focus leaving the menu for the input dismisses it.
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(set).toEqual([]);
});

it.each(["a link", "an image"])("leaves the right-click on %s to the browser in a browser tab", async (what) => {
  const target = document.getElementById(what === "a link" ? "link" : "image")!;
  const press = new MouseEvent("mousedown", { button: 2, bubbles: true, cancelable: true });
  target.dispatchEvent(press);
  expect(press.defaultPrevented).toBe(false);
  expect((await rightClick(target)).defaultPrevented).toBe(false);
  expect(document.querySelector("[role=menu]")).toBeNull();
});

it.each(["a link", "an image"])("opens its own menu on %s in the desktop shell", async (what) => {
  kind = "tauri";
  render();
  await open(document.getElementById(what === "a link" ? "link" : "image")!);
  expect(menuEntries()).toEqual(["Select all"]);
});

it("opens its own menu from the keyboard on a focused link in a browser tab", async () => {
  const link = document.getElementById("link")!;
  link.focus();
  const event = new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true, cancelable: true });
  await act(async () => { link.dispatchEvent(event); });
  expect(menuEntries()).toEqual(["Select all"]);
});

it.each(["Escape", "Select all"])("gives focus back to the inner control the menu was opened from, before the selection is set, when it closes by %s", async (how) => {
  const link = document.getElementById("link")!;
  link.focus();
  select("first");
  const focusAt = recordSelections(() => document.activeElement);
  selectAll.mockImplementation(() => {
    document.getSelection()!.selectAllChildren(region());
    return true;
  });
  await act(async () => { link.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true, cancelable: true })); });
  expect(document.querySelector("[role=menu]")).not.toBeNull();
  if (how === "Escape") await closeMenu();
  else await chooseItem("Select all");
  expect(focusAt).toEqual([link]);
});

// A shadow host stands for the focusable control inside its open shadow root (the code renderer's separator controls),
// and is not focusable itself: the control the menu was opened from cannot take focus back.
it.each(["Escape", "Select all"])("falls back to the surface for focus, before the selection is set, when the control the menu was opened from is a shadow host and the menu closes by %s", async (how) => {
  const host = region().appendChild(document.createElement("div"));
  onTestFinished(() => host.remove());
  const control = host.attachShadow({ mode: "open" }).appendChild(document.createElement("button"));
  control.focus();
  // The page sees the focus on the host.
  expect(document.activeElement).toBe(host);
  select("first");
  const focusAt = recordSelections(() => document.activeElement);
  selectAll.mockImplementation(() => {
    document.getSelection()!.selectAllChildren(region());
    return true;
  });
  await act(async () => { control.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true, composed: true, cancelable: true })); });
  expect(document.querySelector("[role=menu]")).not.toBeNull();
  if (how === "Escape") await closeMenu();
  else await chooseItem("Select all");
  expect(focusAt).toEqual([region()]);
});

it("drops a Select All still waiting for the close when the menu is opened again first, and keeps the selection", async () => {
  select("first");
  const set = recordSelections();
  await open();
  const item = menuItem("Select all");
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
  await act(async () => {
    item.click();
    region().dispatchEvent(event);
  });
  // The reopened content takes focus once it has mounted, and Escape is heard from where the menu holds focus.
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("menu"));
  await closeMenu();
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(selectAll).not.toHaveBeenCalled();
  expect(set).toEqual(["first"]);
});
