import type { TermControl } from "./protocol";

export type TermSocketStatus = "connecting" | "open" | "closed";

interface TermSocketHandlers {
  onData: (data: ArrayBuffer) => void;
  onStatusChange: (status: TermSocketStatus) => void;
}

/**
 * Wraps `/ws/term/:session`. Reconnecting is a deliberate, user-visible action (T3 is exactly
 * about what the ring-buffer replay looks like across a disconnect/reconnect, so this must be
 * easy to trigger by hand rather than something that happens silently in the background).
 *
 * The very first thing sent on a fresh connection is the current terminal size, before any
 * keystroke can reach the daemon — Grok's TUI exits immediately if it is left at the daemon's
 * default PTY size (120x32) for too long, so the UI reports its real dimensions as early as the
 * socket allows rather than trusting that default.
 */
export class TermSocket {
  private socket?: WebSocket;
  private url: string;
  private handlers: TermSocketHandlers;
  private pendingSize?: { cols: number; rows: number };
  private pendingInput: Uint8Array[] = [];
  /** Detaches the current socket's own listeners; replaced each time `connect()` runs. */
  private detachListeners: () => void = () => {};

  constructor(url: string, handlers: TermSocketHandlers, initialSize: { cols: number; rows: number }) {
    this.url = url;
    this.handlers = handlers;
    this.pendingSize = initialSize;
    this.connect();
  }

  private connect(): void {
    this.handlers.onStatusChange("connecting");
    const socket = new WebSocket(this.url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    // Each handler below checks `this.socket === socket` before touching shared state: within
    // one TermSocket, `reconnect()` can start a new socket while the old one's "close" is still
    // in flight, and without this check that stale event would stamp "closed" over the new
    // connection's "open". `detachListeners` below also removes these once the socket they
    // belong to is superseded, so they stop firing at all rather than relying on the check alone.
    const onOpen = () => {
      if (this.socket !== socket) return;
      if (this.pendingSize) this.sendResize(this.pendingSize.cols, this.pendingSize.rows);
      for (const data of this.pendingInput.splice(0)) socket.send(data);
      this.handlers.onStatusChange("open");
    };
    const onMessage = (ev: MessageEvent) => {
      if (this.socket !== socket) return;
      if (ev.data instanceof ArrayBuffer) this.handlers.onData(ev.data);
    };
    const onClose = () => {
      if (this.socket !== socket) return;
      this.handlers.onStatusChange("closed");
    };
    const onError = () => {
      // "close" always follows; nothing extra to do here.
    };

    socket.addEventListener("open", onOpen);
    socket.addEventListener("message", onMessage);
    socket.addEventListener("close", onClose);
    socket.addEventListener("error", onError);

    this.detachListeners = () => {
      socket.removeEventListener("open", onOpen);
      socket.removeEventListener("message", onMessage);
      socket.removeEventListener("close", onClose);
      socket.removeEventListener("error", onError);
    };
  }

  /** User-triggered reconnect. Relies on the daemon replaying its ring buffer on attach (T3). */
  reconnect(): void {
    this.detachListeners();
    this.socket?.close();
    this.connect();
  }

  /** User-triggered disconnect, to exercise the T3 disconnect/reconnect boundary by hand. */
  disconnect(): void {
    this.socket?.close();
  }

  /**
   * Discards this TermSocket because a caller is replacing it with a new one for a different
   * session (see main.ts's `connectTerm`) — unlike `disconnect()`, nothing here should ever run
   * again, including the "closed" status update, which would otherwise land on the new
   * connection's status label once this socket's own asynchronous close event arrives.
   */
  dispose(): void {
    this.detachListeners();
    this.socket?.close();
    this.socket = undefined;
  }

  sendInput(data: Uint8Array): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(data);
    } else if (this.socket?.readyState === WebSocket.CONNECTING) {
      // `WebSocket.send()` throws while CONNECTING; buffer and flush on "open" instead of
      // dropping the keystroke, same idea as `pendingSize` below. Only this state buffers —
      // CLOSING/CLOSED must drop instead, otherwise keystrokes typed after a deliberate
      // `disconnect()` would accumulate here and all get injected into the agent on the next
      // `reconnect()`.
      this.pendingInput.push(data);
    }
  }

  sendResize(cols: number, rows: number): void {
    this.pendingSize = { cols, rows };
    if (this.socket?.readyState === WebSocket.OPEN) {
      const frame: TermControl = { type: "resize", cols, rows };
      this.socket.send(JSON.stringify(frame));
    }
  }
}
