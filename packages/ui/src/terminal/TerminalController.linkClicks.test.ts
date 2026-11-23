// @vitest-environment jsdom
import { Terminal } from "@xterm/xterm";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PathAnswer } from "./links/pathLinks";
import { BROWSER_HOST } from "../test/hosts";
import { TerminalController, type TerminalControllerHandlers } from "./TerminalController";

/**
 * A click on a link in the real xterm, through the real controller: what reaches the session's socket, and what the link
 * opens. It pins what `LinkClickReports` leans on (the order xterm's listeners run in, and the private link detection it
 * reads) end to end: if an upgrade changes either, a click that opens a link reaches the program again and this fails.
 * jsdom lays nothing out, so a character measures 8 by 16 pixels here and the screen's box is put where xterm sized it.
 */
const CELL = { width: 8, height: 16 };
const COLS = 80;
const ROWS = 24;
const URL_TEXT = "https://example.com/page";
const PATH_TEXT = "/tmp/note.txt";
const SGR_MODE = "\x1b[?1006h";
const PRESS_AND_DRAG_MODE = "\x1b[?1002h";

class FakeSocket extends EventTarget {
  static readonly OPEN = 1;
  static all: FakeSocket[] = [];
  readyState = 0;
  binaryType = "blob";
  sent: unknown[] = [];

  constructor(readonly url: string) {
    super();
    FakeSocket.all.push(this);
  }

  send(data: unknown): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = 3;
  }

  open(): void {
    this.readyState = FakeSocket.OPEN;
    this.dispatchEvent(new Event("open"));
  }

  receive(text: string): void {
    this.dispatchEvent(new MessageEvent("message", { data: Uint8Array.from(text, (char) => char.charCodeAt(0)).buffer }));
  }
}

const xterm = vi.hoisted(() => ({ last: undefined as unknown }));
vi.mock("@xterm/xterm", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@xterm/xterm")>();
  return {
    ...actual,
    Terminal: class extends actual.Terminal {
      constructor(options?: ConstructorParameters<typeof actual.Terminal>[0]) {
        super(options);
        xterm.last = this;
      }
    },
  };
});

// jsdom lays nothing out, so fitting would leave the terminal two columns wide: it keeps xterm's own 80 by 24.
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { activate(): void {} dispose(): void {} fit(): void {} } }));

let controller: TerminalController;
let term: Terminal;
let screen: HTMLElement;
let openLink: ReturnType<typeof vi.fn<(url: string) => Promise<void>>>;
let openPath: ReturnType<typeof vi.fn<(target: unknown, onHost: boolean) => Promise<void>>>;

beforeEach(() => {
  FakeSocket.all = [];
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(CELL.width * 32);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(CELL.height);
  // xterm's DOM renderer measures text through a canvas context, which jsdom does not have.
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    font: "", fillStyle: "", measureText: () => ({ width: CELL.width }), clearRect() {}, fillRect() {}, getImageData: () => ({ data: [0, 0, 0, 255] }),
  } as unknown as CanvasRenderingContext2D);
  window.matchMedia = ((query: string) => ({ matches: false, media: query, addListener: () => {}, removeListener: () => {}, addEventListener: () => {}, removeEventListener: () => {} })) as unknown as typeof window.matchMedia;
});

afterEach(() => {
  controller?.dispose();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

/** A controller on a session that takes input, whose program has written `output`, which xterm has parsed. */
async function showing(output: string) {
  openLink = vi.fn(async () => {});
  openPath = vi.fn(async () => {});
  const handlers: TerminalControllerHandlers = {
    onStatusChange: () => {},
    onPaintedChange: () => {},
    copyToClipboard: () => {},
    openLink,
    pathLinks: {
      probe: async (paths) => paths.map((): PathAnswer => ({ kind: "file" })),
      open: openPath,
    },
  };
  controller = new TerminalController(handlers, "dark", { host: BROWSER_HOST, gpu: false });
  term = xterm.last as Terminal;
  const container = document.body.appendChild(document.createElement("div"));
  controller.mount(container);
  screen = term.element!.querySelector<HTMLElement>(".xterm-screen")!;
  screen.getBoundingClientRect = () => new DOMRect(0, 0, COLS * CELL.width, ROWS * CELL.height);
  controller.attach("s-link", "ws://daemon/ws/term/s-link", false);
  const socket = FakeSocket.all.at(-1)!;
  socket.open();
  // The first data of a connection is its replay; the program's output follows on the next row.
  socket.receive(`replay\r\n${output}`);
  await vi.waitFor(() => expect(term.buffer.active.getLine(1)?.translateToString(true)).toContain("now"));
  return socket;
}

/** The middle of a cell of the second row, where the output is, 1-based as xterm counts them. */
const at = (col: number) => ({ clientX: (col - 0.5) * CELL.width, clientY: 1.5 * CELL.height });
const mouse = (type: string, point: { clientX: number; clientY: number }, buttons = 0) =>
  screen.dispatchEvent(new MouseEvent(type, { ...point, buttons, button: 0, bubbles: true }));
/** What reached the socket as input, as text. */
const reports = (socket: FakeSocket) => socket.sent.filter((entry) => ArrayBuffer.isView(entry)).map((entry) => new TextDecoder().decode(entry as Uint8Array));
// eslint-disable-next-line no-control-regex -- Terminal mouse reports begin with the escape control character.
const MOUSE_REPORT = /^\x1b\[<\d+;\d+;\d+[Mm]$/;
const mouseReports = (socket: FakeSocket) => reports(socket).filter((report) => MOUSE_REPORT.test(report) || report.startsWith("\x1b[M"));

/** A primary click on `col`, the pointer first coming to rest there. */
async function click(col: number, { onLink = true } = {}) {
  mouse("mousemove", at(col));
  // A path is a link once the daemon has answered for it.
  await vi.waitFor(() => expect(screen.classList.contains("xterm-cursor-pointer")).toBe(onLink));
  mouse("mousedown", at(col), 1);
  mouse("mouseup", at(col));
}

const ENCODINGS = [
  ["SGR", SGR_MODE, "\x1b[<"],
  ["default", "", "\x1b[M"],
] as const;
const OUTPUT = `see ${URL_TEXT} now and ${PATH_TEXT} too`;
const URL_COL = 10;
const PATH_COL = OUTPUT.indexOf(PATH_TEXT) + 4;
const PLAIN_COL = 2;

describe("a click under mouse reporting", () => {
  it.each(ENCODINGS)("that opens a web link is not reported to the program, in the %s encoding", async (_name, mode) => {
    const socket = await showing(`${PRESS_AND_DRAG_MODE}${mode}${OUTPUT}`);
    await click(URL_COL);
    expect(openLink).toHaveBeenCalledOnce();
    expect(mouseReports(socket)).toEqual([]);
  });

  it.each(ENCODINGS)("that opens a path link is not reported to the program, in the %s encoding", async (_name, mode) => {
    const socket = await showing(`${PRESS_AND_DRAG_MODE}${mode}${OUTPUT}`);
    await click(PATH_COL);
    expect(openPath).toHaveBeenCalledOnce();
    expect(mouseReports(socket)).toEqual([]);
  });

  it.each(ENCODINGS)("on no link is reported, the press and the release, in the %s encoding", async (_name, mode, start) => {
    const socket = await showing(`${PRESS_AND_DRAG_MODE}${mode}${OUTPUT}`);
    await click(PLAIN_COL, { onLink: false });
    expect(openLink).not.toHaveBeenCalled();
    const sent = mouseReports(socket);
    expect(sent).toHaveLength(2);
    expect(sent.every((report) => report.startsWith(start))).toBe(true);
  });

  it("is reported whole, the press before the motion before the release, where the pointer leaves its cell with the button down, though the link opens on the release", async () => {
    const socket = await showing(`${PRESS_AND_DRAG_MODE}${SGR_MODE}${OUTPUT}`);
    mouse("mousemove", at(URL_COL));
    await vi.waitFor(() => expect(screen.classList.contains("xterm-cursor-pointer")).toBe(true));
    mouse("mousedown", at(URL_COL), 1);
    expect(mouseReports(socket)).toEqual([]);
    mouse("mousemove", at(URL_COL + 3), 1);
    mouse("mouseup", at(URL_COL + 3));
    expect(openLink).toHaveBeenCalledOnce();
    expect(mouseReports(socket)).toEqual([`\x1b[<0;${URL_COL};2M`, `\x1b[<32;${URL_COL + 3};2M`, `\x1b[<0;${URL_COL + 3};2m`]);
  });

  it.each([
    ["attaches to another session", () => controller.attach("s-other", "ws://daemon/ws/term/s-other", false)],
    ["detaches and attaches to another session", () => {
      controller.detach({});
      controller.attach("s-other", "ws://daemon/ws/term/s-other", false);
    }],
  ])("held on a link while the controller %s reaches neither session, and the next click on no link there is reported whole", async (_name, goToOther) => {
    const output = `${PRESS_AND_DRAG_MODE}${SGR_MODE}${OUTPUT}`;
    const first = await showing(output);
    mouse("mousemove", at(URL_COL));
    await vi.waitFor(() => expect(screen.classList.contains("xterm-cursor-pointer")).toBe(true));
    mouse("mousedown", at(URL_COL), 1);
    goToOther();
    const other = FakeSocket.all.at(-1)!;
    expect(other).not.toBe(first);
    other.open();
    other.receive(`replay\r\n${output}`);
    await vi.waitFor(() => expect(term.buffer.active.getLine(1)?.translateToString(true)).toContain("now"));
    // The release, and the drag before it, belong to the press that was dropped.
    mouse("mousemove", at(URL_COL + 3), 1);
    mouse("mouseup", at(URL_COL + 3));
    expect(mouseReports(first)).toEqual([]);
    expect(mouseReports(other)).toEqual([]);
    await click(PLAIN_COL, { onLink: false });
    expect(mouseReports(first)).toEqual([]);
    expect(mouseReports(other)).toEqual([`\x1b[<0;${PLAIN_COL};2M`, `\x1b[<0;${PLAIN_COL};2m`]);
  });
});
