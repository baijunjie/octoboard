import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";

export type TermStatus = "connecting" | "open" | "closed" | "not_running";

export interface TerminalControllerHandlers {
  onStatusChange: (status: TermStatus) => void;
}

/**
 * Owns the terminal pane end to end: the one `xterm.js` instance, the active session's socket,
 * the connection status, and keyboard focus — all four change together inside `attach()`, which
 * is the only place any of them is written. Splitting them across independent variables was tried
 * and produced two concrete defects: clicking anything in the UI moved DOM focus off xterm's hidden
 * textarea so keystrokes stopped reaching the agent while output kept flowing, and a superseded
 * socket's `close` event — arriving asynchronously after a session switch — stamped "closed" over
 * the new connection's "open". Routing every state change
 * through one method with a `generation` guard (below) closes off both: nothing reads or writes
 * `status`/`socket`/`sessionId` outside `attach()` and the listeners it installs, and a stale
 * socket's events are discarded by comparing against the generation they were opened for.
 *
 * This class only knows about one session's socket; it has no notion of whether that session's
 * process is still meant to be running. Automatic reconnection after a dropped socket is therefore
 * `TerminalPane`'s job (it has the `Session` record with the status this needs), not this one's.
 */
export class TerminalController {
  readonly term: Terminal;
  private readonly fitAddon: FitAddon;
  private handlers: TerminalControllerHandlers;

  private container?: HTMLElement;
  /** False until a frame has passed since `open()`; see `mount`. */
  private touchable = false;
  private openFrame?: number;
  private socket?: WebSocket;
  private sessionId?: string;
  private status: TermStatus = "closed";
  /** Bumped on every `attach()`; a socket's event handlers no-op once their generation is stale. */
  private generation = 0;
  private pendingInput: Uint8Array[] = [];

  constructor(handlers: TerminalControllerHandlers) {
    this.handlers = handlers;
    this.term = new Terminal({
      scrollback: 10000,
      // A monospace stack with CJK coverage: these agents render box-drawing TUIs and sometimes
      // CJK status text, and the default `courier-new` xterm.js falls back to has neither.
      fontFamily: '"SF Mono", Menlo, Consolas, "Noto Sans Mono CJK SC", "PingFang SC", monospace',
      theme: {
        background: "#1e1f22",
        foreground: "#e4e4e6",
        cursor: "#e4e4e6",
        selectionBackground: "#3a3b40",
      },
    });
    this.fitAddon = new FitAddon();
    this.term.loadAddon(this.fitAddon);

    // `Ctrl+C` is swallowed by WKWebView above xterm.js while every other modifier combination
    // passes through (see "Known pitfalls of the Tauri / Rust approach" in docs/mvp.md) — it is
    // the most-used key in a terminal, so it is intercepted here and its raw byte (ETX, 0x03) is
    // written directly, bypassing xterm's own key-to-data pipeline entirely.
    this.term.attachCustomKeyEventHandler((event) => {
      if (
        event.type === "keydown" &&
        event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "c"
      ) {
        event.preventDefault();
        this.sendInput(new Uint8Array([0x03]));
        return false;
      }
      return true;
    });

    this.term.onData((data) => this.sendInput(new TextEncoder().encode(data)));
    // Mouse reports are not UTF-8: in the default mouse protocol a coordinate is `32 + n`, so past
    // column 95 the encoded byte exceeds 127 and `onData` never sees it — only `onBinary` does,
    // as a JS string of raw code units (one per byte), hence the mask back to a byte rather than a
    // UTF-8 encode (see "Known pitfalls of the Tauri / Rust approach" in docs/mvp.md).
    this.term.onBinary((data) => this.sendInput(Uint8Array.from(data, (c) => c.charCodeAt(0) & 0xff)));
  }

  mount(container: HTMLElement): void {
    this.container = container;
    this.term.open(container);
    // Observed: fitting or clearing the terminal in the same task as `open()` throws from inside
    // xterm's own scheduled work, where no caller's `try` can catch it — and an uncaught throw
    // there takes the whole window down with it. Deferring by a frame avoids it. This is anchored
    // to that observation rather than to any internal state of the library, which is not ours to
    // assume.
    this.openFrame = requestAnimationFrame(() => {
      this.openFrame = undefined;
      this.touchable = true;
      this.fit();
    });
  }

  dispose(): void {
    // The scheduled frame would otherwise fit a disposed terminal, and a throw inside it is
    // exactly the uncatchable kind this class defers around in the first place.
    if (this.openFrame !== undefined) cancelAnimationFrame(this.openFrame);
    this.generation++;
    this.socket?.close();
    this.socket = undefined;
    this.term.dispose();
  }

  get currentStatus(): TermStatus {
    return this.status;
  }

  get currentSessionId(): string | undefined {
    return this.sessionId;
  }

  /**
   * The one state transition. Tears down any existing socket, resets the screen, connects to the
   * new session's terminal stream, and — only when `userInitiated` — restores keyboard focus to
   * xterm; atomically enough that nothing outside this method ever observes a mix of old and new
   * session/socket/status/focus.
   *
   * `userInitiated` separates a selection or a manual Reconnect click (focus belongs on the
   * terminal) from a backoff timer healing a dropped connection in the background (focus must stay
   * wherever it already was — a user mid-keystroke in an open dialog must not be yanked out of it
   * by a reconnect they did not ask for).
   *
   * Call this only for a session that is actually running (`working` / `waiting_user` / `idle`);
   * an interrupted or archived session must be resumed first (`resume_session`), per
   * `daemon/PROTOCOL.md` — "attaching to a session whose process is not running closes the socket
   * immediately". That immediate close is still handled below (as `not_running`) as a safety net
   * for the race where a session stops between the click and the socket connecting, not as the
   * normal path for opening a dormant session.
   */
  attach(sessionId: string, wsUrl: string, userInitiated: boolean): void {
    const generation = ++this.generation;
    this.socket?.close();
    this.socket = undefined;
    this.sessionId = sessionId;
    this.pendingInput = [];
    // Reset unconditionally rather than appending the replay to what is already on screen: the
    // ring buffer always replays its full retained window from the start, not just what changed
    // since the drop, so appending would duplicate whatever overlaps. Losing scrollback older than
    // the ring buffer's retention window on every reconnect is the accepted cost of not risking
    // corrupted-looking duplicated output.
    this.resetScreen();
    this.setStatus("connecting");

    const socket = new WebSocket(wsUrl);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    let everOpened = false;

    socket.addEventListener("open", () => {
      if (generation !== this.generation) return;
      everOpened = true;
      this.sendResize(this.term.cols, this.term.rows);
      for (const data of this.pendingInput.splice(0)) socket.send(data);
      this.setStatus("open");
      if (userInitiated) this.term.focus();
    });
    socket.addEventListener("message", (ev) => {
      if (generation !== this.generation) return;
      if (ev.data instanceof ArrayBuffer) this.term.write(new Uint8Array(ev.data));
    });
    socket.addEventListener("close", () => {
      if (generation !== this.generation) return;
      // The daemon closes immediately, without ever reaching "open", when the session's process
      // is not running — that is a status to display and offer resume for, not a dropped
      // connection (daemon/PROTOCOL.md, "GET /ws/term/:session"). It is also what a *reconnect*
      // gets if it loses a race against the session genuinely ending, so callers must not treat
      // this alone as proof the process is gone — only the session's own stored status says that.
      this.setStatus(everOpened ? "closed" : "not_running");
    });

    if (userInitiated) {
      // Focus follows the session switch even while still connecting, so keystrokes typed during
      // the brief "connecting" window are queued (see sendInput below) instead of landing nowhere.
      this.term.focus();
    }
  }

  /** Detaches without connecting a new session — used when nothing is selected, or the selected
   * session is dormant (interrupted/archived) and waiting to be resumed. */
  detach(): void {
    this.generation++;
    this.socket?.close();
    this.socket = undefined;
    this.sessionId = undefined;
    this.resetScreen();
    this.setStatus("closed");
  }

  resize(cols: number, rows: number): void {
    this.sendResize(cols, rows);
  }

  /** Skipped while the terminal is untouchable (see `mount`); there is nothing on screen to clear
   * that early anyway. */
  private resetScreen(): void {
    if (!this.touchable) return;
    this.term.reset();
  }

  /** Skipped while the terminal is untouchable (see `mount`) and while the pane has no size: a
   * `ResizeObserver` reports the container's first, zero-sized layout pass, and the next
   * observation fits for real. */
  fit(): void {
    if (
      !this.touchable ||
      !this.container ||
      this.container.offsetWidth === 0 ||
      this.container.offsetHeight === 0
    ) {
      return;
    }
    this.fitAddon.fit();
  }

  private setStatus(status: TermStatus): void {
    this.status = status;
    this.handlers.onStatusChange(status);
  }

  private sendInput(data: Uint8Array): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(data);
    } else if (this.socket?.readyState === WebSocket.CONNECTING) {
      // `WebSocket.send()` throws before OPEN; buffer and flush there instead of dropping the
      // keystroke. CLOSING/CLOSED intentionally do not buffer, or input typed after the pane goes
      // idle would all replay into whatever session connects next.
      this.pendingInput.push(data);
    }
  }

  private sendResize(cols: number, rows: number): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify({ type: "resize", cols, rows }));
    }
  }
}
