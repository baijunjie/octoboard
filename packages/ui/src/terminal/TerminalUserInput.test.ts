// @vitest-environment jsdom
import { Terminal } from "@xterm/xterm";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { TerminalController } from "./TerminalController";

const platform = vi.hoisted(() => ({ mac: true }));
vi.mock("../useWindowShortcut", async (original) => ({
  ...await original<typeof import("../useWindowShortcut")>(),
  get IS_MAC() { return platform.mac; },
}));

/** The real xterm, to pin what `TerminalController` relies on through private API: if an upgrade
 * renames `_core.coreService.onUserInput` or changes what it marks, this fails. */
type Core = { coreService: { onUserInput: (listener: () => void) => unknown } };

function observe(): { term: Terminal; events: string[] } {
  const term = new Terminal();
  onTestFinished(() => term.dispose());
  const events: string[] = [];
  (term as unknown as { _core: Core })._core.coreService.onUserInput(() => events.push("user"));
  term.onData((data) => events.push(data));
  return { term, events };
}

describe("xterm's user-input mark", () => {
  it("comes right before the data of input, and not before data with `wasUserInput` false", () => {
    const { term, events } = observe();
    term.input("x");
    term.input("y", false);
    expect(events).toEqual(["user", "x", "y"]);
  });

  it("is absent from its own answer to a query in the output", async () => {
    const { term, events } = observe();
    await new Promise<void>((resolve) => term.write("\x1b[c", resolve));
    expect(events).toHaveLength(1);
    // eslint-disable-next-line no-control-regex -- Verify the terminal response includes its escape prefix.
    expect(events[0]).toMatch(/^\x1b\[\?/);
  });
});

/** No screen is opened: handled keys return from xterm before it needs the DOM composition helper. */
type KeyboardCore = {
  _keyDown: (event: KeyboardEvent) => boolean | undefined;
  _keyUp: (event: KeyboardEvent) => void;
  _customKeyEventHandler: (event: KeyboardEvent) => boolean;
};

function editingTerminal(mac = true) {
  platform.mac = mac;
  class Socket extends EventTarget {
    static readonly OPEN = 1;
    static readonly CONNECTING = 0;
    static last: Socket;
    readyState = Socket.CONNECTING;
    sent: unknown[] = [];
    constructor() { super(); Socket.last = this; }
    send(data: unknown) { this.sent.push(data); }
    close() { this.readyState = 3; }
    open() { this.readyState = Socket.OPEN; this.dispatchEvent(new Event("open")); }
  }
  vi.stubGlobal("WebSocket", Socket);
  const controller = new TerminalController({ onStatusChange: () => {} }, "dark", { gpu: false });
  onTestFinished(() => {
    controller.dispose();
    vi.unstubAllGlobals();
    platform.mac = true;
  });
  controller.attach("editing", "ws://daemon/ws/term/editing", false);
  const core = (controller.term as unknown as { _core: KeyboardCore })._core;
  return { controller, core, socket: Socket.last };
}

const editingKey = (type: string, key: string, modifiers: KeyboardEventInit) =>
  new KeyboardEvent(type, { key, cancelable: true, ...modifiers });

describe("macOS editing through the real xterm and controller", () => {
  it("queues raw editing bytes once per keydown, including repeats, without extra keyup input", () => {
    const { controller, core, socket } = editingTerminal();
    expect(controller.term.options.macOptionIsMeta).toBe(false);
    const xtermData = vi.fn();
    controller.term.onData(xtermData);
    const press = (key: string, modifiers: KeyboardEventInit) => {
      const down = editingKey("keydown", key, modifiers);
      expect(core._keyDown(down)).toBe(false);
      expect(down.defaultPrevented).toBe(true);
      const up = editingKey("keyup", key, modifiers);
      core._keyUp(up);
      expect(up.defaultPrevented).toBe(true);
    };
    press("ArrowLeft", { metaKey: true });
    press("ArrowRight", { metaKey: true });
    press("Backspace", { metaKey: true });
    press("ArrowLeft", { altKey: true });
    press("ArrowRight", { altKey: true });
    press("ArrowRight", { altKey: true, repeat: true });
    expect(socket.sent).toEqual([]);
    socket.open();
    expect(socket.sent.filter((frame): frame is Uint8Array => ArrayBuffer.isView(frame)).map((frame) => Array.from(frame))).toEqual([
      [1], [5], [21], [27, 98], [27, 102], [27, 102],
    ]);
    expect(xtermData).toHaveBeenCalledTimes(6);
  });

  it("invalidates a program's copy and selection-restoration mark only when the editing key goes down", async () => {
    const { controller, core, socket } = editingTerminal();
    socket.open();
    const copy = () => new Promise<void>((resolve) => controller.term.write("\x1b]52;c;cGlja2Vk\x07", resolve));
    await copy();
    expect(controller.programCopiedText).toBe("picked");
    const activity = () => (controller as unknown as { userActs: number }).userActs;
    const before = activity();
    core._keyDown(editingKey("keydown", "ArrowLeft", { metaKey: true }));
    expect(controller.programCopiedText).toBeUndefined();
    expect(activity()).toBe(before + 1);
    await copy();
    core._keyUp(editingKey("keyup", "ArrowLeft", { metaKey: true }));
    expect(controller.programCopiedText).toBe("picked");
    expect(activity()).toBe(before + 1);
  });

  it.each([true, false])("leaves composition, extended chords and character entry to xterm (macOS: %s)", (mac) => {
    const { core, socket } = editingTerminal(mac);
    const keys: [string, KeyboardEventInit][] = [
      ["ArrowLeft", { metaKey: true, isComposing: true }],
      ["ArrowLeft", { metaKey: true, shiftKey: true }],
      ["ArrowRight", { metaKey: true, ctrlKey: true }],
      ["Backspace", { metaKey: true, altKey: true }],
      ["ArrowLeft", { altKey: true, shiftKey: true }],
      ["ArrowRight", { altKey: true, ctrlKey: true }],
      ["ArrowUp", { metaKey: true }],
      ["ArrowDown", { metaKey: true }],
      ["Dead", { altKey: true }],
      ["å", { altKey: true }],
      ["ArrowLeft", {}],
    ];
    if (!mac) keys.push(["ArrowLeft", { metaKey: true }], ["Backspace", { metaKey: true }], ["ArrowRight", { altKey: true }]);
    for (const [key, modifiers] of keys) {
      const event = editingKey("keydown", key, modifiers);
      expect(core._customKeyEventHandler(event)).toBe(true);
      expect(event.defaultPrevented).toBe(false);
    }
    socket.open();
    expect(socket.sent.filter((frame) => ArrayBuffer.isView(frame))).toEqual([]);
  });
});
