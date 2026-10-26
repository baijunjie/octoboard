import { t } from "./i18n/language";
import type { Event, MessageParams, Request, RequestBody } from "./protocol";

type Listener = (event: Event) => void;
type BatchListener = (events: Event[]) => void;
type PendingReply = { resolve: (event: Event) => void; reject: (error: Error) => void };

const MAX_RECONNECT_ATTEMPTS = 2;
const RECONNECT_BASE_DELAY_MS = 500;
/** How long a request may sit queued (socket not open yet, or a reconnect in flight) before giving
 * up on it. Without this, a caller awaiting a request made right as the connection drops — most
 * importantly `shutdown` during the quit flow — would hang forever once the reconnect budget above
 * is spent and no further `close` event ever arrives to reject it. */
const QUEUED_REQUEST_TIMEOUT_MS = 5000;
/** How long a queued broadcast may wait for an animation frame before it is delivered anyway. A
 * hidden window gets no frames, and its state must not fall behind for as long as it stays hidden. */
const BROADCAST_FLUSH_FALLBACK_MS = 250;

/** `connecting` is the first attempt, before the socket has ever been open — distinct from
 * `reconnecting`, so a window that has never connected does not claim to be re-trying. */
export type ConnectionState = "connecting" | "open" | "reconnecting" | "closed";

/** An `error` reply: its `code` and `params` (see "Coded messages" in `apps/daemon/PROTOCOL.md`)
 * for a caller that branches on the failure, and a `message` to show. The client puts the daemon's
 * English text there, and the store replaces it with the wording in the current language. */
export class DaemonRequestError extends Error {
  readonly code: string;
  readonly params: MessageParams;
  constructor(message: string, code: string, params: MessageParams) {
    super(message);
    this.code = code;
    this.params = params;
  }
}

/**
 * The `/ws/control` connection for the lifetime of the window. Every mutating action (`request`)
 * is correlated to its reply by a generated `id`: the daemon answers on the same socket with `ack`
 * or `error` carrying that `id` (or, for `list_dir`, `dir_listing`), and state changes it
 * broadcasts to every client — including ones this client itself caused — arrive with no `id` at
 * all and are only ever delivered through `onEvent`.
 *
 * This is the only thing in the application that talks to the daemon's control channel; no Tauri
 * IPC is involved anywhere in this path (see "Why the daemon is split out" in
 * `docs/architecture.md`).
 *
 * A dropped connection gets a couple of quick, backed-off reconnect attempts rather than being
 * treated as final: where the shell runs the daemon as its sidecar, the daemon dies together with
 * the application (so most drops are either a brief hiccup or the whole process being gone, not
 * something retrying forever would fix), and it re-sends a full `snapshot` on every new control
 * connection, so a reconnect needs no replay logic of its own. Once the budget is spent, `reconnect()` is the way back —
 * called from a user-facing Retry affordance rather than retried automatically forever.
 */
export class DaemonClient {
  private readonly url: string;
  private socket?: WebSocket;
  /** Bumped on every `connect()`; a socket's listeners no-op once their generation is stale. */
  private generation = 0;
  private listeners = new Set<Listener>();
  private batchListeners = new Set<BatchListener>();
  /** Broadcasts received since the last delivery, in arrival order. With many sessions at work the
   * daemon's events come in bursts: they are delivered together once per frame, so a burst costs
   * one state update and one render, not one each. */
  private queuedBroadcasts: Event[] = [];
  private flushFrame?: number;
  private flushTimer?: ReturnType<typeof setTimeout>;
  private connectionListeners = new Set<(state: ConnectionState) => void>();
  private pending = new Map<string, PendingReply>();
  /** Requests made before the socket reaches `OPEN` are queued rather than dropped or thrown. */
  private sendQueue: string[] = [];
  private reconnectAttempts = 0;
  private reconnectTimer?: ReturnType<typeof setTimeout>;
  private connectionState: ConnectionState = "connecting";
  /** Set by `close()` so a deliberate shutdown never schedules a reconnect. */
  private closedPermanently = false;

  constructor(url: string) {
    this.url = url;
    this.connect();
  }

  private connect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
    // Every connection gets a generation, and a superseded socket's events are ignored. Without it
    // a second `connect()` — a Retry pressed while the first attempt is still connecting — leaves
    // the first socket live with all its listeners: every broadcast would arrive twice, and the
    // orphan's eventual `close` would reject the healthy connection's pending requests.
    const generation = ++this.generation;
    const current = () => generation === this.generation;
    // There is no socket yet on the first call, from the constructor. A superseded socket's own
    // `close` handler will no-op on its stale generation, so anything it had in flight is failed
    // here instead — only when it was open, since requests queued against a socket that never
    // opened are flushed by the new one.
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.failPending(t("connection.error.connectionClosed"));
    }
    this.socket?.close();
    const socket = new WebSocket(this.url);
    this.socket = socket;
    socket.addEventListener("open", () => {
      if (!current()) return;
      this.reconnectAttempts = 0;
      for (const payload of this.sendQueue.splice(0)) socket.send(payload);
      this.setConnectionState("open");
    });
    socket.addEventListener("message", (ev) => {
      if (!current()) return;
      this.receive(JSON.parse(ev.data as string) as Event);
    });
    socket.addEventListener("close", () => {
      if (!current()) return;
      // What arrived before the drop is delivered ahead of the state change it precedes.
      this.flushBroadcasts();
      // Whatever this socket had outstanding cannot be answered by a reconnect's new socket — the
      // daemon has no memory of a prior connection's in-flight requests — so these are rejected
      // unconditionally rather than held pending a retry.
      this.failPending(t("connection.error.connectionClosed"));
      // Rejecting and retrying are contradictory: a payload still queued here would otherwise be
      // flushed on the next connection and run an action the caller was already told had failed.
      this.sendQueue.length = 0;
      if (this.closedPermanently) return;
      if (this.reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
        const delay = RECONNECT_BASE_DELAY_MS * 2 ** this.reconnectAttempts;
        this.reconnectAttempts += 1;
        this.setConnectionState("reconnecting");
        this.reconnectTimer = setTimeout(() => this.connect(), delay);
      } else {
        this.setConnectionState("closed");
      }
    });
  }

  /** Retries right away after the automatic reconnect budget was spent — the user-facing Retry
   * affordance calls this; nothing calls it automatically. */
  reconnect(): void {
    this.reconnectAttempts = 0;
    this.connect();
  }

  /** Fails every request still waiting for an answer. A reconnect's socket is a fresh connection
   * with no memory of them, so they can only ever be reported as failed. */
  private failPending(reason: string): void {
    for (const pending of this.pending.values()) {
      pending.reject(new DaemonRequestError(reason, "connection_closed", {}));
    }
    this.pending.clear();
  }

  private setConnectionState(state: ConnectionState): void {
    this.connectionState = state;
    for (const listener of this.connectionListeners) listener(state);
  }

  /** A broadcast waits for the next frame; a reply to a request settles its request and goes out at
   * once, behind the broadcasts that arrived before it, so it is never answered ahead of the state
   * it follows. */
  private receive(event: Event): void {
    const id = "id" in event ? event.id : undefined;
    const pending = id ? this.pending.get(id) : undefined;
    if (id && pending) {
      this.flushBroadcasts();
      this.pending.delete(id);
      if (event.type === "error") pending.reject(new DaemonRequestError(event.message, event.code, event.params));
      else pending.resolve(event);
      this.deliver([event]);
      return;
    }
    this.queuedBroadcasts.push(event);
    if (this.flushFrame === undefined) {
      this.flushFrame = requestAnimationFrame(() => this.flushBroadcasts());
      this.flushTimer = setTimeout(() => this.flushBroadcasts(), BROADCAST_FLUSH_FALLBACK_MS);
    }
  }

  private flushBroadcasts(): void {
    if (this.flushFrame !== undefined) cancelAnimationFrame(this.flushFrame);
    clearTimeout(this.flushTimer);
    this.flushFrame = undefined;
    this.flushTimer = undefined;
    if (this.queuedBroadcasts.length === 0) return;
    this.deliver(this.queuedBroadcasts.splice(0));
  }

  private deliver(events: Event[]): void {
    for (const listener of this.batchListeners) listener(events);
    for (const event of events) {
      for (const listener of this.listeners) listener(event);
    }
  }

  /** Subscribes to every event the daemon sends, including broadcasts with no correlation id. */
  onEvent(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Like `onEvent`, but hears the events that arrived together as one array, in arrival order,
   * so a subscriber can apply them in a single update. */
  onEvents(listener: BatchListener): () => void {
    this.batchListeners.add(listener);
    return () => this.batchListeners.delete(listener);
  }

  /** Subscribes to connection state, reporting the current state straight away: a subscriber that
   * arrives after the socket has already opened would otherwise wait for the next transition,
   * which on a connection that simply stays up never comes. */
  onConnectionChange(listener: (state: ConnectionState) => void): () => void {
    this.connectionListeners.add(listener);
    listener(this.connectionState);
    return () => this.connectionListeners.delete(listener);
  }

  /** Sends a request and resolves with its correlated reply, or rejects with the `error` message
   * (as a `DaemonRequestError`, carrying its `code` and `params`). */
  request(body: RequestBody): Promise<Event> {
    const id = crypto.randomUUID();
    const payload: Request = { ...body, id };
    const json = JSON.stringify(payload);
    return new Promise((resolve, reject) => {
      const socket = this.socket;
      if (socket?.readyState === WebSocket.OPEN) {
        this.pending.set(id, { resolve, reject });
        socket.send(json);
        return;
      }
      if (socket?.readyState === WebSocket.CLOSED && !this.reconnectTimer) {
        // The socket is dead and no retry is scheduled (the reconnect budget is spent) — queuing
        // would hang the caller forever, which a quit's `shutdown` request must never do.
        reject(new DaemonRequestError(t("connection.error.connectionIsClosed"), "connection_closed", {}));
        return;
      }
      // Still connecting, or a reconnect is scheduled: queue it, but bounded — the daemon might
      // never come back, and nothing else will reject this once `sendQueue` is the only place it
      // lives.
      this.pending.set(id, { resolve, reject });
      this.sendQueue.push(json);
      setTimeout(() => {
        if (!this.pending.delete(id)) return;
        const queuedIndex = this.sendQueue.indexOf(json);
        if (queuedIndex !== -1) this.sendQueue.splice(queuedIndex, 1);
        reject(new DaemonRequestError(t("connection.error.timedOut"), "request_timed_out", {}));
      }, QUEUED_REQUEST_TIMEOUT_MS);
    });
  }

  close(): void {
    this.closedPermanently = true;
    this.flushBroadcasts();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.socket?.close();
  }
}
