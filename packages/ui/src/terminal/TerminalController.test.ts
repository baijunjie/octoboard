// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TerminalController } from "./TerminalController";

/** Enough of xterm for the controller: writes are parsed in a later task, in order, as xterm's are,
 * a device-attributes query in the output is answered through `onData`, as xterm answers it, and
 * data that is the user's is announced through `_core.coreService.onUserInput` right before it. */
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
    /** xterm names its input textarea in English when it opens. */
    open(): void {
      this.textarea = document.createElement("textarea");
      this.textarea.setAttribute("aria-label", "Terminal input");
    }
    getSelection(): string { return this.selection; }
    selection = "";
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
  controller = new TerminalController(
    {
      onStatusChange: () => {},
      onPaintedChange: (value) => painted.push(value),
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
