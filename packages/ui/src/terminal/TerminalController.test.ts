// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import { TerminalController } from "./TerminalController";

/** Enough of xterm for the controller: writes are parsed in a later task, in order, as xterm's are,
 * a device-attributes query in the output is answered through `onData`, as xterm answers it, a
 * mouse tracking mode set in the output clears the selection, as xterm's does even for the mode
 * already on, as does a switch to or from the alternate screen (`?1049`), and data that is the
 * user's is announced through `_core.coreService.onUserInput` right before it. */
const { FakeTerminal } = vi.hoisted(() => {
  class FakeTerminal {
    static last: FakeTerminal;
    screen = "";
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    private queue: [string, (() => void) | undefined][] = [];
    private parseTimer: ReturnType<typeof setTimeout> | undefined;
    private dataListeners: ((data: string) => void)[] = [];
    private userInputListeners: (() => void)[] = [];
    _core = {
      coreService: {
        onUserInput: (listener: () => void) => {
          this.userInputListeners.push(listener);
        },
      },
    };

    constructor(options: Record<string, unknown> = {}) {
      this.options = options;
      FakeTerminal.last = this;
    }

    loadAddon(): void {}
    private oscHandlers = new Map<number, (payload: string) => boolean>();
    parser = {
      registerOscHandler: (ident: number, handler: (payload: string) => boolean) => {
        this.oscHandlers.set(ident, handler);
        return { dispose: () => {} };
      },
    };
    registerLinkProvider(): void {}
    onScroll = () => ({ dispose: () => {} });
    onWriteParsed = () => ({ dispose: () => {} });
    onResize = () => ({ dispose: () => {} });
    keyHandler?: (event: KeyboardEvent) => boolean;
    buffer = {
      onBufferChange: () => ({ dispose: () => {} }),
      active: { type: "normal", viewportY: 0, baseY: 0, cursorY: 0, getLine: () => undefined, getNullCell: () => ({}) },
    };

    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean): void {
      this.keyHandler = handler;
    }
    textarea?: HTMLTextAreaElement;
    element?: HTMLElement;
    /** xterm names its input textarea in English when it opens, and puts its element, with the screen
     * inside it, in the container. */
    open(container: HTMLElement): void {
      this.textarea = document.createElement("textarea");
      this.textarea.setAttribute("aria-label", "Terminal input");
      this.element = container.appendChild(document.createElement("div"));
      this.element.appendChild(document.createElement("div")).className = "xterm-screen";
    }
    getSelection(): string { return this.selection; }
    hasSelection(): boolean { return this.selection !== ""; }
    selection = "";
    /** Where `select` put the selection, in xterm's terms: columns and buffer rows, the end exclusive. */
    selectionRange?: { start: { x: number; y: number }; end: { x: number; y: number } };
    getSelectionPosition() { return this.selection ? this.selectionRange : undefined; }
    /** Ends where xterm's `SelectionModel.finalSelectionEnd` puts the end of a start and a length:
     * running on across rows, and at a row's end rather than the next row's start. */
    select(column: number, row: number, length: number): void {
      this.selection = "selected";
      const reach = column + length;
      let end = { x: reach % this.cols, y: row + Math.floor(reach / this.cols) };
      if (reach <= this.cols) end = { x: reach, y: row };
      else if (reach % this.cols === 0) end = { x: this.cols, y: row + reach / this.cols - 1 };
      this.selectionRange = { start: { x: column, y: row }, end };
    }
    selectAll(): void { this.selection = this.screen; }
    paste(text: string): void { this.userTypes(`\x1b[200~${text}\x1b[201~`); }
    focus(): void {}
    /** Drops what is still queued, so no parse runs into the next test (xterm itself would still parse it). */
    dispose(): void {
      clearTimeout(this.parseTimer);
      this.queue = [];
    }
    modes = { mouseTrackingMode: "none" };
    private binaryListeners: ((data: string) => void)[] = [];

    onBinary(listener: (data: string) => void): void {
      this.binaryListeners.push(listener);
    }

    /** A default-encoding mouse report, which xterm sends unmarked, as raw code units. */
    binary(data: string): void {
      for (const listener of this.binaryListeners) listener(data);
    }

    onData(listener: (data: string) => void): void {
      this.dataListeners.push(listener);
    }

    reset(): void {
      this.screen = "";
    }

    get parsing(): boolean {
      return this.queue.length > 0;
    }

    write(data: string | Uint8Array, callback?: () => void): void {
      const text = typeof data === "string" ? data : String.fromCharCode(...data);
      if (this.queue.push([text, callback]) === 1) this.parseTimer = setTimeout(() => this.parse());
    }

    /** A key pressed and released, which xterm turns into `data`. */
    press(key: string, data: string): void {
      this.keyDown(key);
      this.userTypes(data);
      this.keyHandler?.(new KeyboardEvent("keyup", { key, keyCode: key.toUpperCase().charCodeAt(0) }));
    }

    /** A key going down, which xterm may or may not turn into data. */
    keyDown(key: string): void {
      this.keyHandler?.(new KeyboardEvent("keydown", { key, keyCode: key.toUpperCase().charCodeAt(0) }));
    }

    /** Data xterm marks as the user's: a key, a paste, an input method's text. */
    userTypes(data: string): void {
      for (const listener of this.userInputListeners) listener();
      this.emit(data);
    }

    /** Data xterm emits on its own, such as the answer to a query. */
    emit(data: string): void {
      for (const listener of this.dataListeners) listener(data);
    }

    private parse(): void {
      for (const [text, callback] of this.queue.splice(0)) {
        if (text.includes("\x1b[c")) this.emit("\x1b[?1;2c");
        // eslint-disable-next-line no-control-regex -- An OSC sequence begins with ESC and ends with BEL.
        for (const [, ident, payload] of text.matchAll(/\x1b\](\d+);([^\x07]*)\x07/g)) this.oscHandlers.get(Number(ident))?.(payload);
        // eslint-disable-next-line no-control-regex -- A mode set begins with ESC.
        for (const [, mode] of text.matchAll(/\x1b\[\?(1000|1002|1003)h/g)) {
          this.modes.mouseTrackingMode = { "1000": "vt200", "1002": "drag", "1003": "any" }[mode]!;
          this.selection = "";
        }
        // eslint-disable-next-line no-control-regex -- A mode set begins with ESC.
        for (const [, set] of text.matchAll(/\x1b\[\?1049([hl])/g)) {
          this.buffer.active.type = set === "h" ? "alternate" : "normal";
          this.selection = "";
        }
        this.screen += text;
        callback?.();
      }
    }
  }
  return { FakeTerminal };
});
type FakeTerminal = InstanceType<typeof FakeTerminal>;

class FakeSocket extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static all: FakeSocket[] = [];
  readyState = FakeSocket.CONNECTING;
  binaryType = "blob";
  sent: unknown[] = [];
  closed = false;

  constructor(readonly url: string) {
    super();
    FakeSocket.all.push(this);
  }

  send(data: unknown): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.readyState = 3;
  }

  open(): void {
    this.readyState = FakeSocket.OPEN;
    this.dispatchEvent(new Event("open"));
  }

  receive(text: string): void {
    const data = Uint8Array.from(text, (char) => char.charCodeAt(0)).buffer;
    this.dispatchEvent(new MessageEvent("message", { data }));
  }

  /** The daemon's mark of the replay that follows, sent first on attaching to a live process. */
  mark(replay: { length: number; offset: number; answered: number }): void {
    this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ type: "replay", ...replay }) }));
  }
}

vi.mock("@xterm/xterm", () => ({ Terminal: FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit(): void {} } }));
vi.mock("@xterm/addon-web-links", () => ({ WebLinksAddon: class {} }));
// Stands in for the WebGL renderer, which jsdom cannot create.
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: class { onContextLoss(): void {} dispose(): void {} } }));

const url = (session: string) => `ws://daemon/ws/term/${session}`;
const lastSocket = () => FakeSocket.all[FakeSocket.all.length - 1];
const framesPending = new Set<ReturnType<typeof setTimeout>>();
/** Lets every write queued so far be parsed, and every frame requested so far run, including those
 * one schedules for the next. It waits on the work itself rather than on a fixed delay: a parse
 * whose callback requests a frame is a chain of timers, and a delay timer that expires in the same
 * stall of the event loop as the first of them fires before the next one is even scheduled. */
const settle = async () => {
  do await new Promise((resolve) => setTimeout(resolve));
  while (term.parsing || framesPending.size > 0);
};

let painted: boolean[];
let copied: string[];
let container: HTMLElement;
let controller: TerminalController;
let term: FakeTerminal;

beforeEach(async () => {
  FakeSocket.all = [];
  vi.stubGlobal("WebSocket", FakeSocket);
  framesPending.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => {
    const frame = setTimeout(() => {
      framesPending.delete(frame);
      callback();
    });
    framesPending.add(frame);
    return frame;
  });
  vi.stubGlobal("cancelAnimationFrame", (frame: ReturnType<typeof setTimeout>) => {
    clearTimeout(frame);
    framesPending.delete(frame);
  });
  painted = [];
  copied = [];
  controller = new TerminalController(
    {
      onStatusChange: () => {},
      onPaintedChange: (value) => painted.push(value),
      copyToClipboard: (text) => copied.push(text),
    },
    "dark",
  );
  term = FakeTerminal.last;
  container = document.createElement("div");
  controller.mount(container);
  await settle();
});

afterEach(() => {
  // Disposing the controller bumps its generation, so a paint frame still pending does nothing when it
  // runs, and disposes the fake terminal, which drops its queued writes: a test that ends with work
  // pending leaves nothing to land in the next, such as a replay's paint reaching the module-level
  // `painted` the next test reads.
  controller.dispose();
  vi.unstubAllGlobals();
});

describe("a live session's replay", () => {
  it("is written to xterm in slices, in order, and the screen counts as painted once the last is parsed", async () => {
    const write = vi.spyOn(term, "write");
    controller.attach("a", url("a"), false);
    lastSocket().open();
    const replay = "abcdefghij".repeat(15_000);
    lastSocket().receive(replay);

    const slices = write.mock.calls.slice(1);
    expect(slices.map(([data]) => (data as Uint8Array).length)).toEqual([65_536, 65_536, 18_928]);
    expect(slices.map(([, callback]) => callback !== undefined)).toEqual([false, false, true]);
    expect(painted).toEqual([]);

    await settle();
    expect(term.screen).toBe(replay);
    expect(painted).toEqual([true]);
  });
});

describe("a dormant session's saved output", () => {
  it("is loaded once and read-only, and typing over it sends nothing", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    const saved = lastSocket();
    expect(saved.url).toBe(url("a"));

    saved.receive("last words\x1b[c");
    await settle();
    expect(term.screen).toContain("last words");
    expect(painted).toEqual([]);

    // Every later render of the same dormant session detaches again.
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    await settle();
    expect(FakeSocket.all).toHaveLength(1);
    expect(term.screen).toContain("last words");

    term.userTypes("x");
    expect(saved.sent).toEqual([]);
  });

  it("replaces the live shutdown screen with the same session's saved snapshot only once", async () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    live.receive("shutdown cleanup");
    await settle();
    expect(term.screen).toContain("shutdown cleanup");

    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    const saved = lastSocket();
    expect(saved).not.toBe(live);
    expect(live.closed).toBe(true);
    // A re-render while the saved socket is connecting must not restart its load.
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    saved.receive("frozen conversation\x1b[c");
    live.receive("late cleanup");
    await settle();
    saved.close();
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    await settle();

    expect(FakeSocket.all).toHaveLength(2);
    expect(term.screen).toContain("frozen conversation");
    expect(term.screen).not.toContain("cleanup");
    expect(saved.sent).toEqual([]);
  });

  it("clears queued live cleanup before drawing the saved snapshot", async () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.receive("shutdown cleanup\x1b[c");
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    // The snapshot can arrive after the live output already queued in xterm is parsed.
    await settle();
    lastSocket().receive("frozen conversation");
    await settle();

    expect(term.screen).toContain("frozen conversation");
    expect(term.screen).not.toContain("shutdown cleanup");
  });

  it("passes a focus report on to a live process", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    lastSocket().receive("last words");
    await settle();

    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    await settle();
    term.emit("\x1b[I");
    expect(live.sent).toContainEqual(new TextEncoder().encode("\x1b[I"));
  });

  it("holds the user's typing for the connection but not xterm's answer to a query, while the screen resets", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    lastSocket().receive("last words");
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    term.emit("\x1b[c");
    term.userTypes("x");
    await settle();
    live.open();
    expect(live.sent).toEqual([
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
      new TextEncoder().encode("x"),
    ]);
  });

  it("does not take a mouse report for typing while the screen resets under mouse tracking", () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    term.modes.mouseTrackingMode = "vt200";
    term.userTypes("\x1b[<0;10;5M");
    term.userTypes("\x1b[<0;10;5m");
    term.binary("\x1b[M #\xe0");
    term.userTypes("x");
    live.open();
    expect(live.sent).toEqual([
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
      new TextEncoder().encode("x"),
    ]);
  });

  it("takes an SGR-shaped report for typing when mouse tracking is off", () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    term.userTypes("\x1b[<0;10;5M");
    live.open();
    expect(live.sent).toContainEqual(new TextEncoder().encode("\x1b[<0;10;5M"));
  });

  it.each([
    ["before it arrives", false],
    ["while it is still being parsed", true],
  ])("is not drawn for another session selected %s", async (_, arrivedFirst) => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    const a = lastSocket();
    if (arrivedFirst) a.receive("from a");
    controller.detach({ keepScreenFor: "b", savedOutputUrl: url("b") });
    if (!arrivedFirst) a.receive("from a");
    lastSocket().receive("from b");
    await settle();

    expect(a.closed).toBe(true);
    expect(term.screen).not.toContain("from a");
    expect(term.screen).toContain("from b");
  });

  it.each([
    ["after being parsed", true],
    ["while still queued", false],
  ])("gives way to the live replay on resume %s", async (_, parsedFirst) => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    const saved = lastSocket();
    saved.receive("old output");
    if (parsedFirst) await settle();

    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    live.receive("replay");
    saved.receive("late");
    await settle();

    expect(term.screen).toContain("replay");
    expect(term.screen).not.toContain("old output");
    expect(term.screen).not.toContain("late");
  });
});

describe("xterm's answers to queries in the output", () => {
  const replies = (socket: FakeSocket) => socket.sent.filter((frame) => typeof frame === "string" && frame.includes('"reply"'));
  const reply = (data: string, offset: number) => JSON.stringify({ type: "reply", data, offset });

  it("go as reply frames naming the point in the output they answer, apart from the user's input", async () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    live.mark({ length: 0, offset: 100, answered: 0 });
    live.receive("\x1b[c");
    await settle();
    term.userTypes("x");
    live.receive("more\x1b[c");
    await settle();
    expect(live.sent.slice(1)).toEqual([reply("\x1b[?1;2c", 103), new TextEncoder().encode("x"), reply("\x1b[?1;2c", 110)]);
  });

  it("are given for the replay only past what was answered already", async () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    const replay = "old\x1b[c|new\x1b[c";
    live.mark({ length: replay.length, offset: 50, answered: replay.indexOf("|") });
    live.receive(replay);
    await settle();
    live.receive("\x1b[c");
    await settle();
    expect(replies(live)).toEqual([reply("\x1b[?1;2c", 50), reply("\x1b[?1;2c", 53)]);
  });

  it("are not given without the daemon's mark", async () => {
    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    live.receive("\x1b[c");
    await settle();
    expect(replies(live)).toEqual([]);
  });

  it("are not given for output of the connection before, while no reset holds them back", async () => {
    // Before its first frame after mounting, a reset of the screen holds nothing back.
    const fresh = new TerminalController({ onStatusChange: () => {} }, "dark");
    fresh.attach("a", url("a"), false);
    const first = lastSocket();
    first.open();
    first.mark({ length: 0, offset: 0, answered: 0 });
    first.receive("\x1b[c");
    fresh.attach("a", url("a"), false);
    const second = lastSocket();
    second.open();
    await settle();
    fresh.dispose();
    expect([...replies(first), ...replies(second)]).toEqual([]);
  });
});

describe("the input's accessible name", () => {
  it("replaces xterm's English name once the terminal is open, and follows a later label", () => {
    controller.setInputLabel("终端输入");
    expect(term.textarea?.getAttribute("aria-label")).toBe("终端输入");
    controller.setInputLabel("Terminal entrée");
    expect(term.textarea?.getAttribute("aria-label")).toBe("Terminal entrée");
  });
});


it("selects terminal content and reads its selection independently of the textarea", async () => {
  term.write("saved terminal output");
  await settle();
  controller.selectAll();
  expect(controller.selectedText).toContain("saved terminal output");
});

it("pastes through xterm into a connecting session, retaining bracketed paste", () => {
  controller.attach("s-paste", url("s-paste"), false);
  controller.paste("first\nsecond", "s-paste");
  lastSocket().open();
  const bytes = lastSocket().sent.filter((entry) => ArrayBuffer.isView(entry)) as Uint8Array[];
  expect(bytes.map((entry) => new TextDecoder().decode(entry))).toEqual(["\x1b[200~first\nsecond\x1b[201~"]);
});

it("drops paste after its destination changed or input became unavailable", () => {
  controller.attach("s-new", url("s-new"), false);
  lastSocket().open();
  controller.paste("old text", "s-old");
  controller.detach({ keepScreenFor: "s-new" });
  controller.paste("stopped text", "s-new");
  expect(lastSocket().sent.filter((entry) => ArrayBuffer.isView(entry))).toEqual([]);
});

describe("keyboard focus on attaching a session the user picked", () => {
  it.each([
    [false, 2],
    [true, 0],
  ])("goes to the terminal unless the session was picked by a tap (tap: %s)", (touch, focuses) => {
    const focus = vi.spyOn(term, "focus");
    document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: touch ? "touch" : "mouse", bubbles: true }));
    controller.attach("a", url("a"), true);
    lastSocket().open();
    expect(focus).toHaveBeenCalledTimes(focuses);
  });
});

describe("text a program asks to copy (OSC 52)", () => {
  const copy = (text: string) => `\x1b]52;c;${btoa(String.fromCharCode(...new TextEncoder().encode(text)))}\x07`;

  const liveCopy = async (text: string) => {
    controller.attach("a", url("a"), false);
    lastSocket().open();
    lastSocket().receive("replay");
    lastSocket().receive(copy(text));
    await settle();
  };

  it("goes to the clipboard and the editing menu from live output, but not from the replay of what came before the attach", async () => {
    controller.attach("a", url("a"), false);
    lastSocket().open();
    lastSocket().receive(`before${copy("old")}`);
    await settle();
    expect(controller.programCopiedText).toBeUndefined();
    lastSocket().receive(copy("选中的文本"));
    await settle();
    expect(copied).toEqual(["选中的文本"]);
    expect(controller.programCopiedText).toBe("选中的文本");
  });

  it("is ignored in a dormant session's saved output", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    lastSocket().receive(copy("saved"));
    await settle();
    expect(copied).toEqual([]);
    expect(controller.programCopiedText).toBeUndefined();
  });

  it("stays on offer through the pointer's way to the editing menu: its motion reports and a right press", async () => {
    await liveCopy("picked");
    container.appendChild(term.textarea!);
    term.userTypes("\x1b[<35;4;2M");
    term.textarea!.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true }));
    expect(controller.programCopiedText).toBe("picked");
  });

  it("stays on offer through a Control-click only on macOS, where it opens the editing menu", async () => {
    await liveCopy("picked");
    container.appendChild(term.textarea!);
    term.textarea!.dispatchEvent(new MouseEvent("mousedown", { button: 0, ctrlKey: true, bubbles: true }));
    expect(controller.programCopiedText).toBe(IS_MAC ? "picked" : undefined);
  });

  it.each<[string, () => void]>([
    ["a key", () => term.press("x", "x")],
    ["Ctrl+C", () => term.keyHandler?.(new KeyboardEvent("keydown", { key: "c", keyCode: 67, ctrlKey: true }))],
    ["a paste", () => controller.paste("text", "a")],
    ["a primary press", () => term.textarea!.dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true }))],
    ["a session switch", () => controller.attach("b", url("b"), false)],
  ])("is dropped by %s", async (_, act) => {
    await liveCopy("picked");
    container.appendChild(term.textarea!);
    act();
    expect(controller.programCopiedText).toBeUndefined();
  });
});

describe("an Option-click", () => {
  it("moves the cursor only on a screen without mouse reporting", () => {
    const press = () => term.textarea!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    container.appendChild(term.textarea!);
    term.modes.mouseTrackingMode = "drag";
    press();
    expect(term.options.altClickMovesCursor).toBe(false);
    term.modes.mouseTrackingMode = "none";
    press();
    expect(term.options.altClickMovesCursor).toBe(true);
  });
});

describe("pointer motion with no button held", () => {
  it.each([
    { tracking: "any", selection: "picked", buttons: 0, over: "screen", reported: false },
    { tracking: "any", selection: "picked", buttons: 0, over: "edge", reported: false },
    { tracking: "any", selection: "", buttons: 0, over: "screen", reported: true },
    { tracking: "any", selection: "picked", buttons: 1, over: "screen", reported: true },
    { tracking: "drag", selection: "picked", buttons: 0, over: "screen", reported: true },
    { tracking: "none", selection: "picked", buttons: 0, over: "screen", reported: true },
  ])("reaches xterm's report unless that would clear a selection, and the screen's link hover always (%o)", ({ tracking, selection, buttons, over, reported }) => {
    const screen = term.element!.querySelector<HTMLElement>(".xterm-screen")!;
    const target = (over === "screen" ? screen : term.element!).appendChild(document.createElement("div"));
    const report = vi.fn();
    const hover = vi.fn();
    term.element!.addEventListener("mousemove", report);
    screen.addEventListener("mousemove", hover);
    term.modes.mouseTrackingMode = tracking;
    term.selection = selection;
    target.dispatchEvent(new MouseEvent("mousemove", { buttons, bubbles: true }));
    expect(report).toHaveBeenCalledTimes(reported ? 1 : 0);
    expect(hover).toHaveBeenCalledTimes(over === "screen" ? 1 : 0);
  });
});

describe("a selection made under mouse reporting, when the program re-asserts its modes", () => {
  /** What Claude Code writes on its own after a pause in its input. */
  const reassert = "\x1b(B\x0f\x1b[?2004h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h";
  const selected = async (column = 3, row = 1, length = 7) => {
    controller.attach("a", url("a"), false);
    lastSocket().open();
    lastSocket().receive(`replay${reassert}`);
    await settle();
    term.select(column, row, length);
  };

  it.each([
    { what: "Claude Code's re-assertion", output: reassert, kept: true },
    { what: "the mouse modes alone", output: "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h", kept: true },
    { what: "a re-assertion with text", output: `${reassert}redrawn`, kept: false },
    { what: "another tracking mode", output: "\x1b[?1000h", kept: false },
    { what: "a re-assertion that switches to the alternate screen", output: `${reassert}\x1b[?1049h`, kept: false },
  ])("is kept only through output that sets nothing but the modes it had: $what", async ({ output, kept }) => {
    await selected();
    lastSocket().receive(output);
    await settle();
    expect(term.getSelectionPosition()).toEqual(kept ? { start: { x: 3, y: 1 }, end: { x: 10, y: 1 } } : undefined);
  });

  it("is kept whole when it spans rows", async () => {
    await selected(70, 1, 2 * 80 + 5 - 70);
    expect(term.getSelectionPosition()).toEqual({ start: { x: 70, y: 1 }, end: { x: 5, y: 3 } });
    lastSocket().receive(reassert);
    await settle();
    expect(term.getSelectionPosition()).toEqual({ start: { x: 70, y: 1 }, end: { x: 5, y: 3 } });
  });

  it.each([
    { what: "clicked", act: () => term.textarea!.dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true })), kept: false },
    { what: "typed a key", act: () => term.press("x", "x"), kept: false },
    { what: "pressed Control-C", act: () => term.keyHandler?.(new KeyboardEvent("keydown", { key: "c", keyCode: 67, ctrlKey: true })), kept: false },
    { what: "moved the pointer with no button held", act: () => term.userTypes("\x1b[<35;4;2M"), kept: true },
  ])("is put back after the user $what while the output was being parsed only if that could not mean to clear it", async ({ act, kept }) => {
    await selected();
    container.appendChild(term.textarea!);
    const write = term.write.bind(term);
    let snapshotTaken = false;
    // The first empty write after the output arrives is the one that notes the selection, ahead of
    // the output; the user acts right after it, before the output is parsed.
    vi.spyOn(term, "write").mockImplementation((data, callback) => {
      if (data !== "" || snapshotTaken || !callback) return write(data, callback);
      snapshotTaken = true;
      write(data, () => {
        callback();
        act();
      });
    });
    lastSocket().receive(reassert);
    await settle();
    expect(term.getSelectionPosition()).toEqual(kept ? { start: { x: 3, y: 1 }, end: { x: 10, y: 1 } } : undefined);
  });
});
