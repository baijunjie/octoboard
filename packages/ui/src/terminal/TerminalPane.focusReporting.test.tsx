// @vitest-environment jsdom
import { act, StrictMode, useLayoutEffect, useRef, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { windowAttention } from "../components/announce/windowTestSupport";
import { sessionOf } from "../fixtures/builders";
import type { Session } from "../protocol";
import { ThemeProvider } from "../preferences/theme";
import { fakeTimers, FAKE_FRAME_MS } from "../test/clock";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import type { Daemon } from "../store";
import { TerminalPane, type TerminalPaneHandle } from "./TerminalPane";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The controller is real. The renderer emits the same focus bytes as xterm when mode 1004 is on,
 * and parses writes later, so an old screen's reporting mode can still be active during a switch. */
const { FocusTerminal } = vi.hoisted(() => {
  const disposable = () => ({ dispose() {} });
  class FocusTerminal {
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    element?: HTMLElement;
    textarea?: HTMLTextAreaElement;
    private reportsFocus = false;
    private listeners: ((data: string) => void)[] = [];
    private writes: [string | Uint8Array, (() => void) | undefined][] = [];
    private timer?: ReturnType<typeof setTimeout>;
    private disposed = false;
    _core = { coreService: { onUserInput: disposable } };
    parser = { registerOscHandler: disposable };
    modes = { mouseTrackingMode: "none" };
    buffer = {
      onBufferChange: disposable,
      active: { type: "normal", viewportY: 0, baseY: 0, cursorY: 0, getLine: () => undefined, getNullCell: () => ({}) },
    };
    constructor(options: Record<string, unknown>) {
      this.options = options;
    }
    loadAddon(): void {}
    registerLinkProvider(): void {}
    attachCustomKeyEventHandler(): void {}
    onScroll = disposable;
    onResize = disposable;
    onWriteParsed = disposable;
    onBinary(): void {}
    onData(listener: (data: string) => void): void { this.listeners.push(listener); }
    hasSelection(): boolean { return false; }
    getSelection(): string { return ""; }
    getSelectionPosition(): undefined { return undefined; }
    open(container: HTMLElement): void {
      this.element = container.appendChild(document.createElement("div"));
      this.element.appendChild(document.createElement("div")).className = "xterm-screen";
      this.textarea = this.element.appendChild(document.createElement("textarea"));
      this.textarea.className = "xterm-helper-textarea";
      this.textarea.addEventListener("focus", () => {
        if (this.reportsFocus) this.listeners.forEach((listener) => listener("\x1b[I"));
      });
    }
    focus(): void {
      if (this.disposed) throw new Error("Focus reached a disposed terminal");
      if (!this.textarea?.closest("[inert]")) this.textarea?.focus();
    }
    reset(): void { this.reportsFocus = false; }
    write(data: string | Uint8Array, callback?: () => void): void {
      if (this.disposed) throw new Error("Write reached a disposed terminal");
      this.writes.push([data, callback]);
      if (this.timer !== undefined) return;
      this.timer = setTimeout(() => {
        this.timer = undefined;
        for (const [data, done] of this.writes.splice(0)) {
          const text = typeof data === "string" ? data : new TextDecoder().decode(data);
          if (text.includes("\x1b[?1004h")) this.reportsFocus = true;
          done?.();
        }
      });
    }
    dispose(): void {
      this.disposed = true;
      clearTimeout(this.timer);
      this.writes = [];
      this.element?.remove();
    }
  }
  return { FocusTerminal };
});

vi.mock("@xterm/xterm", () => ({ Terminal: FocusTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit(): void {} } }));
vi.mock("@xterm/addon-web-links", () => ({ WebLinksAddon: class {} }));
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: class { onContextLoss(): void {} dispose(): void {} } }));

class Socket extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static all: Socket[] = [];
  readyState = Socket.CONNECTING;
  sent: unknown[] = [];
  constructor(readonly url: string) { super(); Socket.all.push(this); }
  send(data: unknown): void { this.sent.push(data); }
  close(): void { this.readyState = 3; }
  open(): void { this.readyState = Socket.OPEN; this.dispatchEvent(new Event("open")); }
  receive(text: string): void {
    this.dispatchEvent(new MessageEvent("message", { data: Uint8Array.from(text, (char) => char.charCodeAt(0)).buffer }));
  }
  input(): string[] {
    return this.sent.filter((data): data is Uint8Array => ArrayBuffer.isView(data)).map((data) => new TextDecoder().decode(data));
  }
}

const oldSession = sessionOf("old", "console", undefined, "Old", "idle");
const newSession = sessionOf("new", "console", undefined, "New", "idle");

/** Models the archive owner's layout-phase handoff, after the pane's layout effects in the same commit. */
function ArchiveOwner({ session, covered }: { session: Session; covered: boolean }) {
  const pane = useRef<TerminalPaneHandle>(null);
  const wasCovered = useRef(covered);
  useLayoutEffect(() => {
    if (wasCovered.current && !covered) pane.current?.handOff();
    wasCovered.current = covered;
  }, [covered]);
  return <ThemeProvider>
    <button data-archive>Archive</button>
    <TerminalPane ref={pane} session={session} behindArchive={covered} onResume={async () => true} onAddAgent={() => {}} />
  </ThemeProvider>;
}

function prepare() {
  Socket.all = [];
  onTestFinished(() => { vi.unstubAllGlobals(); });
  fakeTimers();
  windowAttention();
  vi.stubGlobal("WebSocket", Socket);
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {} }));
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  onTestFinished(() => canvas.mockRestore());
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  return testDaemon({ terminalUrl: (id) => `ws://daemon/ws/term/${id}` });
}

/** StrictMode must enclose the provider at the root to replay initial effects in React 19. */
function mount(children: ReactNode, daemon: Daemon, strict = false) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (children: ReactNode) => {
    const content = <TestHost daemon={daemon}>{children}</TestHost>;
    act(() => root.render(strict ? <StrictMode>{content}</StrictMode> : content));
  };
  render(children);
  return Object.assign(container, { render });
}

async function parseAndPaint() {
  // A parse schedules the first paint on a frame. Let React commit between them.
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  await act(async () => { await vi.advanceTimersByTimeAsync(FAKE_FRAME_MS); });
}

it("never sends the closing archive's focus report to the live session being left", async () => {
  const daemon = prepare();
  const host = mount(<ArchiveOwner session={oldSession} covered={false} />, daemon);
  await parseAndPaint();
  const old = Socket.all.at(-1)!;
  act(() => { old.open(); old.receive("\x1b[?1004h"); });
  await parseAndPaint();
  host.render(<ArchiveOwner session={oldSession} covered />);
  act(() => host.querySelector<HTMLButtonElement>("[data-archive]")!.focus());
  old.sent = [];

  host.render(<ArchiveOwner session={newSession} covered={false} />);
  expect(old.input()).toEqual([]);
  expect(document.activeElement).toBe(host.querySelector("textarea"));
  const current = Socket.all.at(-1)!;
  expect(current.url).toContain("/new?");
  act(() => { current.open(); current.receive("\x1b[?1004h"); });
  await parseAndPaint();
  act(() => {
    host.querySelector<HTMLButtonElement>("[data-archive]")!.focus();
    host.querySelector<HTMLTextAreaElement>("textarea")!.focus();
  });
  expect(current.input()).toContain("\x1b[I");
  expect(old.input()).toEqual([]);
});

it.each([false, true])("connects its first session and survives StrictMode replay (%s)", async (strict) => {
  const daemon = prepare();
  const host = mount(<ArchiveOwner session={oldSession} covered={false} />, daemon, strict);
  await parseAndPaint();
  const active = Socket.all.at(-1)!;
  expect(active.url).toContain("/old?");
  expect(active.readyState).toBe(Socket.CONNECTING);
  expect(host.querySelectorAll("textarea")).toHaveLength(1);
  expect(Socket.all).toHaveLength(strict ? 2 : 1);
  act(() => { active.open(); active.receive("\x1b[?1004h"); });
  await parseAndPaint();
  act(() => {
    host.querySelector<HTMLButtonElement>("[data-archive]")!.focus();
    host.querySelector<HTMLTextAreaElement>("textarea")!.focus();
  });
  expect(active.input()).toContain("\x1b[I");
  expect(Socket.all.slice(0, -1).every((socket) => socket.readyState === 3)).toBe(true);
});

it("keeps only the current saved-output connection after StrictMode replays an interrupted session", async () => {
  const daemon = prepare();
  mount(<ArchiveOwner session={{ ...oldSession, status: "interrupted" }} covered={false} />, daemon, true);
  await parseAndPaint();
  expect(Socket.all).toHaveLength(2);
  expect(Socket.all[0].readyState).toBe(3);
  expect(Socket.all[1].url).toBe("ws://daemon/ws/term/old");
  expect(Socket.all[1].readyState).toBe(Socket.CONNECTING);
});
