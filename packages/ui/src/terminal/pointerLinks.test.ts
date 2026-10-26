// @vitest-environment jsdom
import { type ILink, type ITerminalOptions, Terminal } from "@xterm/xterm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { installPathLinks, type PathAnswer } from "./pathLinks";
import { watchPointerForLinks } from "./pointerLinks";

/** The real xterm, to pin the link detection `watchPointerForLinks` relies on: if an upgrade changes
 * what it listens to or when it asks the providers, this fails. jsdom lays nothing out, so a
 * character measures 8 by 16 pixels here and the screen's box is put where xterm sized it. */
const CELL = { width: 8, height: 16 };
const COLS = 20;
const ROWS = 5;

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(CELL.width * 32);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(CELL.height);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
  })) as unknown as typeof window.matchMedia;
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

function openTerminal(options: ITerminalOptions = {}): { term: Terminal; screen: HTMLElement; signal: AbortSignal } {
  const term = new Terminal({ ...options, cols: COLS, rows: ROWS });
  term.open(document.body.appendChild(document.createElement("div")));
  const screen = term.element!.querySelector<HTMLElement>(".xterm-screen")!;
  screen.getBoundingClientRect = () => new DOMRect(0, 0, COLS * CELL.width, ROWS * CELL.height);
  return { term, screen, signal: new AbortController().signal };
}

function write(term: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

/** The middle of a cell, 1-based as xterm counts them. */
function at(col: number, row: number): { clientX: number; clientY: number } {
  return { clientX: (col - 0.5) * CELL.width, clientY: (row - 0.5) * CELL.height };
}

/** A real mouse event over the screen, which bubbles as the browser's does. */
function mouse(screen: HTMLElement, type: string, point: { clientX: number; clientY: number }, buttons = 0): void {
  screen.dispatchEvent(new MouseEvent(type, { ...point, buttons, bubbles: true }));
}

function moveTo(screen: HTMLElement, point: { clientX: number; clientY: number }, buttons = 0): void {
  mouse(screen, "mousemove", point, buttons);
}

/** Whether xterm shows a link under the pointer: it gives the screen the pointer cursor. */
function showsLink(screen: HTMLElement): boolean {
  return screen.classList.contains("xterm-cursor-pointer");
}

/** A link provider whose link over columns 5 to 11 of the first row exists only once `ready`. */
function lateProvider(term: Terminal): { asked: number[]; ready: () => void } {
  const asked: number[] = [];
  let ready = false;
  term.registerLinkProvider({
    provideLinks(y, callback) {
      asked.push(y);
      const link: ILink = { range: { start: { x: 5, y: 1 }, end: { x: 11, y: 1 } }, text: "/a/b.ts", activate: () => {} };
      callback(ready && y === 1 ? [link] : undefined);
    },
  });
  return { asked, ready: () => (ready = true) };
}

describe("xterm's link detection", () => {
  it("does not ask again for a pointer that stays on its row", () => {
    const { term, screen } = openTerminal();
    const provider = lateProvider(term);
    moveTo(screen, at(6, 1));
    provider.ready();
    moveTo(screen, at(6, 1));
    moveTo(screen, at(7, 1));
    expect(provider.asked).toEqual([1]);
    expect(showsLink(screen)).toBe(false);
  });

  it("keeps a row's replies while the pointer is off the screen", () => {
    const { term, screen } = openTerminal();
    const provider = lateProvider(term);
    moveTo(screen, at(6, 1));
    screen.dispatchEvent(new MouseEvent("mouseleave"));
    provider.ready();
    moveTo(screen, at(7, 1));
    expect(provider.asked).toEqual([1]);
    expect(showsLink(screen)).toBe(false);
  });
});

describe("asking again under a resting pointer", () => {
  it("shows the link a provider has come to have under the pointer", () => {
    const { term, screen, signal } = openTerminal();
    const provider = lateProvider(term);
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, 1));
    provider.ready();
    askAgain();
    expect(showsLink(screen)).toBe(true);
  });

  it("goes up a row from the bottom half of the screen", () => {
    const { term, screen, signal } = openTerminal();
    const provider = lateProvider(term);
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, ROWS));
    provider.asked.length = 0;
    askAgain();
    expect(provider.asked).toEqual([ROWS - 1, ROWS]);
  });

  it("waits for a pointer off the screen, and asks once it comes back onto the same row", () => {
    const { term, screen, signal } = openTerminal();
    const provider = lateProvider(term);
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, 1));
    screen.dispatchEvent(new MouseEvent("mouseleave"));
    provider.ready();
    askAgain();
    expect(provider.asked).toEqual([1]);
    moveTo(screen, at(6, 1));
    expect(showsLink(screen)).toBe(true);
    moveTo(screen, at(7, 1));
    expect(provider.asked).toEqual([1, 2, 1]);
  });

  it("waits for a held button, and asks once it is released", () => {
    const { term, screen, signal } = openTerminal();
    const provider = lateProvider(term);
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, 1));
    mouse(screen, "mousedown", at(6, 1), 1);
    provider.ready();
    askAgain();
    moveTo(screen, at(6, 1), 1);
    expect(provider.asked).toEqual([1]);
    mouse(screen, "mouseup", at(6, 1));
    expect(showsLink(screen)).toBe(true);
  });

  it("leaves a link on the row it passes over, and shows none where the pointer rests on no link", () => {
    const { term, screen, signal } = openTerminal();
    const events: string[] = [];
    term.registerLinkProvider({
      provideLinks(y, callback) {
        const link: ILink = {
          range: { start: { x: 5, y: 2 }, end: { x: 11, y: 2 } },
          text: "/a/b.ts",
          activate: () => {},
          hover: () => events.push("hover"),
          leave: () => events.push("leave"),
        };
        callback(y === 2 ? [link] : undefined);
      },
    });
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, 1));
    askAgain();
    expect(events).toEqual(["hover", "leave"]);
    expect(showsLink(screen)).toBe(false);
  });

  it("leaves an OSC 8 hyperlink on the row it passes over", async () => {
    const events: string[] = [];
    const { term, screen, signal } = openTerminal({
      linkHandler: { activate: () => {}, hover: () => events.push("hover"), leave: () => events.push("leave") },
    });
    await write(term, "plain text\r\n\x1b]8;;https://example.com/\x07a hyperlink\x1b]8;;\x07");
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(3, 1));
    askAgain();
    expect(events).toEqual(["hover", "leave"]);
    expect(showsLink(screen)).toBe(false);
  });

  it("asks nothing once its signal is aborted", () => {
    const { term, screen } = openTerminal();
    const provider = lateProvider(term);
    const controller = new AbortController();
    const askAgain = watchPointerForLinks(term, screen, controller.signal);
    moveTo(screen, at(6, 1));
    controller.abort();
    askAgain();
    expect(provider.asked).toEqual([1]);
  });

  it("reports no motion to a program that tracks the pointer, and leaves the selection alone", async () => {
    const { term, screen, signal } = openTerminal();
    lateProvider(term);
    const reports: string[] = [];
    term.onData((data) => reports.push(data));
    term.onBinary((data) => reports.push(data));
    await write(term, "\x1b[?1003h\x1b[?1006hsee /a/b.ts here");
    const askAgain = watchPointerForLinks(term, screen, signal);
    moveTo(screen, at(6, 1));
    // A real move is reported: the program tracks the pointer.
    expect(reports).toHaveLength(1);
    term.select(0, 0, 3);
    askAgain();
    expect(reports).toHaveLength(1);
    expect(term.getSelection()).toBe("see");
  });
});

describe("a path the pointer rests on before the daemon answers", () => {
  it("becomes a link once the answer comes, without the pointer moving", async () => {
    const { term, screen, signal } = openTerminal();
    let answer!: (answers: PathAnswer[]) => void;
    const probe = vi.fn(() => new Promise<PathAnswer[]>((resolve) => (answer = resolve)));
    const askAgain = watchPointerForLinks(term, screen, signal);
    installPathLinks(term, { probe, open: async () => {} }, () => false, askAgain);
    await write(term, "see /a/b.ts here");
    moveTo(screen, at(6, 1));
    expect(showsLink(screen)).toBe(false);
    await vi.waitFor(() => expect(probe).toHaveBeenCalledWith(["/a/b.ts"]));
    answer([{ kind: "file" }]);
    await vi.waitFor(() => expect(showsLink(screen)).toBe(true));
  });
});
