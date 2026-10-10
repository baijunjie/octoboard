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
    keyHandler?: (event: KeyboardEvent) => boolean;
    wheelHandler?: () => boolean;
    buffer = { active: { type: "normal" } };

    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean): void {
      this.keyHandler = handler;
    }
    attachCustomWheelEventHandler(handler: () => boolean): void {
      this.wheelHandler = handler;
    }
    textarea?: HTMLTextAreaElement;
    /** xterm names its input textarea in English when it opens. */
    open(): void {
      this.textarea = document.createElement("textarea");
      this.textarea.setAttribute("aria-label", "Terminal input");
    }
    focus(): void {}
    dispose(): void {}
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

    write(data: string | Uint8Array, callback?: () => void): void {
      const text = typeof data === "string" ? data : String.fromCharCode(...data);
      if (this.queue.push([text, callback]) === 1) setTimeout(() => this.parse());
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

const url = (session: string) => `ws://daemon/ws/term/${session}`;
const lastSocket = () => FakeSocket.all[FakeSocket.all.length - 1];
/** Lets every write queued so far be parsed. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

let wakes: string[];
/** What `onWake` reports: whether the session started. */
let wakeStarts: boolean;
let painted: boolean[];
let container: HTMLElement;
let controller: TerminalController;
let term: FakeTerminal;

beforeEach(async () => {
  FakeSocket.all = [];
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => setTimeout(callback));
  wakes = [];
  wakeStarts = false;
  painted = [];
  controller = new TerminalController(
    {
      onStatusChange: () => {},
      onPaintedChange: (value) => painted.push(value),
      onWake: async (session) => {
        wakes.push(session);
        return wakeStarts;
      },
    },
    "dark",
  );
  term = FakeTerminal.last;
  container = document.createElement("div");
  controller.mount(container);
  await settle();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a dormant session's saved output", () => {
  it("is loaded once, read-only, and only typing wakes the session", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    const saved = lastSocket();
    expect(saved.url).toBe(url("a"));

    saved.receive("last words\x1b[c");
    await settle();
    expect(term.screen).toContain("last words");
    expect(wakes).toEqual([]);
    expect(painted).toEqual([]);

    // Every later render of the same dormant session detaches again.
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    await settle();
    expect(FakeSocket.all).toHaveLength(1);
    expect(term.screen).toContain("last words");

    term.userTypes("x");
    expect(wakes).toEqual(["a"]);
    expect(saved.sent).toEqual([]);
  });

  it.each([
    ["a key", (t: FakeTerminal) => t.press("x", "x")],
    ["a paste", (t: FakeTerminal) => t.userTypes("pasted")],
    ["an input method's text", (t: FakeTerminal) => t.userTypes("你好")],
  ])("lets %s through while it is being parsed", (_, input) => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    lastSocket().receive("last words");
    input(term);
    expect(wakes).toEqual(["a"]);
  });

  it("drops xterm's own answer to a query in it", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    lastSocket().receive("last words\x1b[c");
    await settle();
    expect(wakes).toEqual([]);

    // Once parsed, the same data from xterm is typing again.
    term.emit("x");
    expect(wakes).toEqual(["a"]);
  });

  it("does not take a mouse report for typing while saved output has mouse tracking on", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    lastSocket().receive("last words");
    term.modes.mouseTrackingMode = "vt200";
    term.userTypes("\x1b[<0;10;5M");
    term.userTypes("\x1b[<0;10;5m");
    term.binary("\x1b[M #\xe0");
    expect(wakes).toEqual([]);

    // Typing is still typing under the same mode.
    term.userTypes("x");
    expect(wakes).toEqual(["a"]);
  });

  it("takes an SGR-shaped report for typing when mouse tracking is off", () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    lastSocket().receive("last words");
    term.userTypes("\x1b[<0;10;5M");
    expect(wakes).toEqual(["a"]);
  });

  it("does not let an Alt+click move the cursor, which would reopen it, until it is attached", () => {
    const altClick = () => term.options.altClickMovesCursor;
    expect(altClick()).toBe(false);
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    expect(altClick()).toBe(false);

    controller.attach("a", url("a"), false);
    expect(altClick()).toBe(true);

    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    expect(altClick()).toBe(false);
  });

  it("does not pass on xterm's answer to a query that follows the user's typing", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    controller.armWake("a");
    wakeStarts = true;
    const saved = lastSocket();
    saved.receive("last words");
    term.userTypes("x");
    saved.receive("more\x1b[c");
    await settle();
    expect(wakes).toEqual(["a"]);

    controller.attach("a", url("a"), false);
    const live = lastSocket();
    live.open();
    expect(live.sent).toEqual([
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
      new TextEncoder().encode("x"),
    ]);
  });

  it("does not turn the wheel into arrow keys over saved output left in the alternate screen", () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    term.buffer.active.type = "alternate";
    expect(term.wheelHandler?.()).toBe(false);

    term.buffer.active.type = "normal";
    expect(term.wheelHandler?.()).toBe(true);

    controller.attach("a", url("a"), false);
    term.buffer.active.type = "alternate";
    expect(term.wheelHandler?.()).toBe(true);
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

  it("gives way to the live replay once the session is started", async () => {
    controller.detach({ keepScreenFor: "a", savedOutputUrl: url("a") });
    const saved = lastSocket();
    saved.receive("old output");
    await settle();

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
