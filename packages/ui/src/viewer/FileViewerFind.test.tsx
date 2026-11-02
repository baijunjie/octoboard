// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { IS_MAC } from "../input/keys";
import { eventKey } from "../shortcuts/binding";
import { FIND_BINDING } from "../shortcuts/commands";
import { ThemeProvider } from "../preferences/theme";
import type { ViewerSubject } from "./content";
import * as findDom from "./findDom";
import { FileViewer } from "./FileViewer";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in draws the text it is handed.
vi.mock("./renderer", async () => {
  const { useEffect } = await import("react");
  return {
    HighlightedFile: ({ text, onDrawn }: { text: string; onDrawn?: () => void }) => {
      useEffect(() => onDrawn?.(), [text, onDrawn]);
      return <pre data-renderer="">{text}</pre>;
    },
    RenderedDiff: ({ patch, onDrawn }: { patch: string; onDrawn?: () => void }) => {
      useEffect(() => onDrawn?.(), [patch, onDrawn]);
      return <pre data-renderer="">{patch}</pre>;
    },
    endRendererPool: () => {},
  };
});

window.matchMedia ??= ((query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);

// jsdom lays nothing out and has no scrolling: a line is 20px tall, and a region, once made
// `scrollable`, is a 100px window on a long file.
Range.prototype.getClientRects = function (this: Range) {
  const region = this.startContainer.parentElement?.closest<HTMLElement>("[data-find-region]");
  const top = (this.startContainer.textContent ?? "").slice(0, this.startOffset).split("\n").length * 20 - 20 - (region?.scrollTop ?? 0);
  return [{ top, bottom: top + 20, left: 0, right: 10, width: 10, height: 20 }] as unknown as DOMRectList;
};

function scrollable(region: HTMLElement): void {
  let top = 0;
  Object.defineProperties(region, {
    scrollTop: { get: () => top, set: (value: number) => (top = value), configurable: true },
    clientHeight: { value: 100, configurable: true },
    scrollHeight: { value: 10000, configurable: true },
  });
  region.style.overflowY = "auto";
}

/** A file of `lines` lines, with the given text on the given lines. */
const lines = (count: number, at: Record<number, string>) => Array.from({ length: count }, (_, index) => at[index] ?? "").join("\n");

let container: HTMLDivElement;
let root: Root;
const onClose = vi.fn();

beforeEach(() => {
  document.documentElement.style.setProperty("--breakpoint-phone", "594px");
  localStorage.clear();
  onClose.mockClear();
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  vi.useRealTimers();
  container.remove();
});

const textFile = (path: string, text: string): ViewerSubject => ({ key: path, path, content: { state: "file", body: { kind: "text", text, size: text.length } } });
const binary: ViewerSubject = { key: "b", path: "bin/b.wasm", content: { state: "file", body: { kind: "binary", size: 3 } } };

function show(subject: ViewerSubject): void {
  act(() => {
    root.render(
      <ViewerHost>
        <ThemeProvider>
          <FileViewer subject={subject} onClose={onClose} />
        </ThemeProvider>
      </ViewerHost>,
    );
  });
}

// The code surface draws through a lazily imported renderer, which a timer reaches.
const settled = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
const dialog = () => document.querySelector<HTMLElement>("[role=dialog]")!;
const bar = () => document.querySelector<HTMLElement>("[role=search]");
const input = () => bar()!.querySelector("input")!;
// The count beside the query, which the query is described by; what is announced is the status region.
const status = () => document.getElementById(input().getAttribute("aria-describedby")!)!.textContent;
const announced = () => bar()!.querySelector("[role=status]")!.textContent;
const toggle = (name: string) => bar()!.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)!;

function press(target: Element, key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}
const find = (target: Element = dialog()) => press(target, "f", eventKey(FIND_BINDING));

function type(value: string): void {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  act(() => {
    setter.call(input(), value);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// The query rests before it is searched.
const rest = () => act(() => vi.advanceTimersByTimeAsync(300));

async function open(text = "alpha beta\nBeta gamma\nbeta"): Promise<void> {
  show(textFile("a.ts", text));
  await settled();
  vi.useFakeTimers();
  find();
}

it("opens on the platform's find key in a text view, with the query focused, and offers nothing for a binary", async () => {
  show(binary);
  expect(find().defaultPrevented).toBe(false);
  expect(bar()).toBeNull();
  await open();
  expect(bar()).not.toBeNull();
  expect(document.activeElement).toBe(input());
  expect(bar()!.getAttribute("aria-label")).toBe("Find in file");
});

it("counts the matches as the query settles and steps through them, wrapping around", async () => {
  await open();
  type("beta");
  expect(status()).toBe("");
  await rest();
  expect(status()).toBe("1/3");
  press(input(), "Enter");
  expect(status()).toBe("2/3");
  press(input(), "Enter");
  press(input(), "Enter");
  expect(status()).toBe("1/3");
  press(input(), "Enter", { shiftKey: true });
  expect(status()).toBe("3/3");
  act(() => toggle("Previous match").click());
  expect(status()).toBe("2/3");
});

it("scrolls the current match to the middle of the region as the reader steps to it", async () => {
  await open(lines(100, { 10: "needle", 40: "needle" }));
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  scrollable(region);
  type("needle");
  await rest();
  // Line 10 is at 200px, 10px of its 20 in the middle of a 100px window.
  expect(region.scrollTop).toBe(160);
  press(input(), "Enter");
  expect(region.scrollTop).toBe(760);
});

it("scrolls to the first match of a second query, and of a changed option, not to the match that was current", async () => {
  await open(lines(100, { 10: "Alpha", 30: "alpha", 60: "omega" }));
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  scrollable(region);
  type("alpha");
  await rest();
  expect(region.scrollTop).toBe(160);
  type("omega");
  await rest();
  expect(region.scrollTop).toBe(1160);
  type("alpha");
  await rest();
  expect(region.scrollTop).toBe(160);
  // With the case switch on the first match is the second line it is on.
  act(() => toggle("Match case").click());
  await rest();
  expect(status()).toBe("1/1");
  expect(region.scrollTop).toBe(560);
});

it("scrolls a match clear of the gap the bar leaves at the top", async () => {
  await open(lines(100, { 10: "needle" }));
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  scrollable(region);
  region.style.scrollPaddingTop = "40px";
  type("needle");
  await rest();
  // The middle of the 60px below the padding is 70px down.
  expect(region.scrollTop).toBe(140);
});

it("keeps what the reader is reading in place as the bar opens and closes over a scrolled region", async () => {
  // Where the browser has no scroll anchoring of its own (WebKit).
  const supports = vi.spyOn(CSS, "supports").mockReturnValue(false);
  onTestFinished(() => supports.mockRestore());
  show(textFile("a.ts", lines(100, { 10: "needle" })));
  await settled();
  vi.useFakeTimers();
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  scrollable(region);
  // The gap is read where the content inherits it, so it is there before any region is drawn.
  region.closest<HTMLElement>("[data-find-content]")!.style.setProperty("--find-bar-gap", "56px");
  region.scrollTop = 300;
  find();
  expect(region.scrollTop).toBe(356);
  act(() => toggle("Close find").click());
  expect(region.scrollTop).toBe(300);
});

it("announces the count in full, after the focus move that opened the bar, and draws it short and hidden", async () => {
  await open();
  type("beta");
  await rest();
  expect(announced()).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(1600));
  expect(announced()).toBe("1 of 3");
  expect(input().getAttribute("aria-describedby")).toBeTruthy();
  expect(document.getElementById(input().getAttribute("aria-describedby")!)!.getAttribute("aria-hidden")).toBe("true");
  act(() => toggle("Use regular expression").click());
  type("(");
  await rest();
  expect(announced()).toBe("Invalid regular expression");
});

it("narrows the search with the case and whole word switches, which say whether they are on", async () => {
  await open();
  type("beta");
  await rest();
  expect(toggle("Match case").getAttribute("aria-pressed")).toBe("false");
  act(() => toggle("Match case").click());
  await rest();
  expect(toggle("Match case").getAttribute("aria-pressed")).toBe("true");
  expect(status()).toBe("1/2");
  act(() => toggle("Whole word").click());
  type("bet");
  await rest();
  expect(status()).toBe("0/0");
});

it("says a regular expression that does not compile is invalid, and finds nothing", async () => {
  await open();
  act(() => toggle("Use regular expression").click());
  type("(");
  await rest();
  expect(status()).toBe("Invalid");
  expect(input().getAttribute("aria-invalid")).toBe("true");
  type("^b.ta$");
  await rest();
  expect(status()).toBe("1/1");
});

it("closes on Escape before the viewer does, and gives focus back to the code", async () => {
  await open();
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  press(input(), "Escape");
  expect(bar()).toBeNull();
  expect(document.activeElement).toBe(region);
  // The viewer's own close is the dialog's Escape handling, which the first press did not reach.
  expect(dialog()).not.toBeNull();
});

it("closes on Escape pressed in the code while the bar is open", async () => {
  await open();
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  act(() => region.focus());
  const event = press(region, "Escape");
  expect(event.defaultPrevented).toBe(true);
  expect(bar()).toBeNull();
});

it("opens and closes from the header's toggle, which is there only over text", async () => {
  const headerToggle = () => document.querySelector<HTMLButtonElement>("[data-viewer-description] button[aria-expanded]");
  show(binary);
  await settled();
  expect(headerToggle()).toBeNull();
  show(textFile("a.ts", "alpha"));
  await settled();
  expect(headerToggle()!.getAttribute("aria-label")).toBe("Find in file");
  act(() => headerToggle()!.click());
  await settled();
  expect(bar()).not.toBeNull();
  expect(document.activeElement).toBe(input());
  expect(headerToggle()!.getAttribute("aria-label")).toBe("Close find");
  expect(headerToggle()!.getAttribute("aria-expanded")).toBe("true");
  act(() => headerToggle()!.click());
  expect(bar()).toBeNull();
});

it("closes with its button", async () => {
  await open();
  act(() => toggle("Close find").click());
  expect(bar()).toBeNull();
});

it("selects the query when the key is pressed with the bar open already", async () => {
  await open();
  type("beta");
  act(() => (document.querySelector("[data-find-region]") as HTMLElement).focus());
  find();
  expect(document.activeElement).toBe(input());
  expect([input().selectionStart, input().selectionEnd]).toEqual([0, 4]);
});

it("leaves the arrow keys to the query", async () => {
  await open();
  const event = press(input(), "ArrowRight");
  expect(event.defaultPrevented).toBe(false);
});

it("keeps the bar and its query on another file and searches that one", async () => {
  await open();
  type("beta");
  await rest();
  act(() => {
    root.render(
      <ViewerHost>
        <ThemeProvider>
          <FileViewer subject={textFile("b.ts", "beta beta")} onClose={onClose} />
        </ThemeProvider>
      </ViewerHost>,
    );
  });
  await rest();
  expect(input().value).toBe("beta");
  expect(status()).toBe("1/2");
});

it("hides the bar over a view with no text and brings it back with the next text", async () => {
  await open();
  type("beta");
  await rest();
  show(binary);
  await rest();
  expect(bar()).toBeNull();
  show(textFile("b.ts", "beta"));
  await rest();
  expect(bar()).not.toBeNull();
  expect(status()).toBe("1/1");
});

it("searches the text again when what is drawn changes", async () => {
  await open();
  type("beta");
  await rest();
  const pre = dialog().querySelector("pre[data-renderer]")!;
  act(() => {
    pre.textContent = "beta beta beta beta";
  });
  await rest();
  expect(status()).toBe("1/4");
});

const headerToggle = () => document.querySelector<HTMLButtonElement>("[data-viewer-description] button[aria-expanded]")!;

it("closes the bar, not the viewer, on Escape with focus on the header's toggle, though it is expanded", async () => {
  await open();
  act(() => headerToggle().focus());
  expect(headerToggle().getAttribute("aria-expanded")).toBe("true");
  // Its tooltip is open with the focus, and the first Escape dismisses it.
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
  press(headerToggle(), "Escape");
  const event = press(headerToggle(), "Escape");
  expect(event.defaultPrevented).toBe(true);
  expect(bar()).toBeNull();
  expect(onClose).not.toHaveBeenCalled();
});

it("returns focus to the header's toggle when the toggle closes the bar, and to the code when Escape or Close do", async () => {
  await open();
  expect(document.activeElement).toBe(input());
  act(() => headerToggle().click());
  expect(bar()).toBeNull();
  expect(document.activeElement).toBe(headerToggle());
  find();
  act(() => toggle("Close find").click());
  expect(document.activeElement).toBe(dialog().querySelector("[data-find-region]"));
});

it("hands focus on when the bar goes with the text it searched, so it does not fall to the page", async () => {
  await open();
  expect(document.activeElement).toBe(input());
  show(binary);
  await rest();
  expect(bar()).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
  expect(dialog().contains(document.activeElement)).toBe(true);
});

it("turns the focus ring on for what its key focuses", async () => {
  await open();
  setInteractionModality("pointer");
  find();
  expect(getInteractionModality()).toBe("keyboard");
});

it("dismisses an open tooltip with the first Escape, and closes the bar, not the viewer, with the next", async () => {
  await open();
  act(() => toggle("Match case").focus());
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
  press(toggle("Match case"), "Escape");
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  expect(bar()).not.toBeNull();
  expect(onClose).not.toHaveBeenCalled();
  press(toggle("Match case"), "Escape");
  expect(bar()).toBeNull();
  expect(onClose).not.toHaveBeenCalled();
});

it("closes the viewer on Escape once the bar is closed", async () => {
  await open();
  press(input(), "Escape");
  expect(bar()).toBeNull();
  press(dialog().querySelector("[data-find-region]")!, "Escape");
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("shows the code's focus ring after Escape closes the bar following a pointer press", async () => {
  await open();
  type("beta");
  await rest();
  setInteractionModality("pointer");
  act(() => toggle("Next match").click());
  press(input(), "Escape");
  expect(bar()).toBeNull();
  const region = dialog().querySelector<HTMLElement>("[data-find-region]")!;
  expect(document.activeElement).toBe(region);
  expect(getInteractionModality()).toBe("keyboard");
  expect(region.hasAttribute("data-focus-visible")).toBe(true);
});

it("keeps Escape for a control elsewhere in the dialog with its popup open", async () => {
  await open();
  const popup = dialog().appendChild(document.createElement("button"));
  popup.setAttribute("aria-expanded", "true");
  act(() => popup.focus());
  press(popup, "Escape");
  expect(bar()).not.toBeNull();
});

it("takes the find key while the bar is up and nothing is drawn yet, focusing the query", async () => {
  await open();
  show({ key: "b", path: "b.ts", content: { state: "loading" } });
  act(() => (document.activeElement as HTMLElement).blur());
  expect(bar()).not.toBeNull();
  const event = find();
  expect(event.defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(input());
});

it("waits for a file that is on its way, and shows its first match, with no count of nothing meanwhile", async () => {
  await open(lines(100, { 10: "needle" }));
  type("needle");
  await rest();
  const revealed: string[] = [];
  const spy = vi.spyOn(findDom, "revealMatch").mockImplementation((range) => void revealed.push(range.startContainer.textContent ?? ""));
  onTestFinished(() => spy.mockRestore());
  show({ key: "b", path: "b.ts", content: { state: "loading" } });
  await rest();
  expect(status()).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(announced()).toBe("");
  show(textFile("b.ts", "first needle\nsecond needle"));
  await rest();
  expect(status()).toBe("1/2");
  expect(revealed).toEqual(["first needle\nsecond needle"]);
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(announced()).toBe("1 of 2");
});

it("closes the viewer on Escape again once the bar has gone with its text by itself", async () => {
  await open();
  show(binary);
  await rest();
  expect(bar()).toBeNull();
  press(dialog(), "Escape");
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("matches the find key by its physical key, as the shortcut list does, whatever character it types", async () => {
  show(textFile("a.ts", "alpha"));
  await settled();
  const modifier = IS_MAC ? { metaKey: true } : { ctrlKey: true };
  // The same key on a layout whose letter is not Latin.
  expect(press(dialog(), "а", { code: "KeyF", ...modifier }).defaultPrevented).toBe(true);
  expect(bar()).not.toBeNull();
  act(() => toggle("Close find").click());
  // The letter f on another key is not Find.
  expect(press(dialog(), "f", { code: "KeyJ", ...modifier }).defaultPrevented).toBe(false);
  expect(bar()).toBeNull();
});
