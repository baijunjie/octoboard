import { CodedError } from "./errors/codedError";
import { t } from "./i18n/language";
import { PROTOCOL_GENERATION, type ClientRequest, type Event } from "./protocol";
import { REPLY_TYPES, type RequestSender, type RequestType } from "./requests";

type Listener = (event: Event) => void;
type BatchListener = (events: Event[]) => void;
type PendingReply = { resolve: (event: Event) => void; reject: (error: Error) => void };
type QueuedRequest = { payload: string; timer: ReturnType<typeof setTimeout> };

export const MAX_RECONNECT_ATTEMPTS = 2;
export const RECONNECT_BASE_DELAY_MS = 500;
/** The first and the longest wait between two attempts of a client that retries until it connects
 * (`ENDLESS_RECONNECT`). */
export const ENDLESS_RECONNECT_BASE_DELAY_MS = 1000;
export const ENDLESS_RECONNECT_MAX_DELAY_MS = 30_000;

/** The longest Retry waits for its policy's `beforeRetry` before it attempts all the same: longer than the desktop
 * shell may take to make an SSH connection, connecting and signing in, while an attempt under way holds it. */
export const BEFORE_RETRY_TIMEOUT_MS = 45_000;

/** How a lost connection is retried: how many automatic attempts follow one another (`Infinity`: until one connects),
 * the wait before attempt number `attempt`, counting from 0, and what Retry waits for before it attempts. */
export interface ReconnectPolicy {
  readonly attempts: number;
  readonly delay: (attempt: number) => number;
  /** Settles once Retry may attempt; a failure is passed over. */
  readonly beforeRetry?: () => Promise<void>;
}

/** A couple of quick attempts, then Retry: a window on the daemon of its own machine. */
export const BRIEF_RECONNECT: ReconnectPolicy = {
  attempts: MAX_RECONNECT_ATTEMPTS,
  delay: (attempt) => RECONNECT_BASE_DELAY_MS * 2 ** attempt,
};

/** Attempts until one connects, the wait doubling from `ENDLESS_RECONNECT_BASE_DELAY_MS` up to
 * `ENDLESS_RECONNECT_MAX_DELAY_MS`: a remote daemon's
 * window, whose connection goes over SSH and comes back once the network does (see "A dropped connection" in
 * `docs/product/remote-daemons.md`). */
export const ENDLESS_RECONNECT: ReconnectPolicy = {
  attempts: Infinity,
  delay: (attempt) => Math.min(ENDLESS_RECONNECT_BASE_DELAY_MS * 2 ** attempt, ENDLESS_RECONNECT_MAX_DELAY_MS),
};

/** How long a request may sit queued (socket not open yet, or a reconnect in flight) before giving
 * up on it. Without this, a caller awaiting a request made right as the connection drops — most
 * importantly `shutdown` during the quit flow — would hang forever once the reconnect budget above
 * is spent and no further `close` event ever arrives to reject it. */
export const QUEUED_REQUEST_TIMEOUT_MS = 5000;
/** How long a queued broadcast may wait for an animation frame before it is delivered anyway. A
 * hidden window gets no frames, and its state must not fall behind for as long as it stays hidden. */
export const BROADCAST_FLUSH_FALLBACK_MS = 250;
/** The heartbeat (see "The heartbeat" in `apps/daemon/PROTOCOL.md`): a `ping` request goes out every
 * `HEARTBEAT_INTERVAL_MS`, and a connection that has delivered no frame at all for `HEARTBEAT_TIMEOUT_MS` is lost. A
 * connection can die with no close on either side — a network path dropping, a machine sleeping — and nothing else
 * would notice it. The check runs on a finer tick than the pings so that a dead connection is found close to the
 * timeout, not up to an interval after it. */
export const HEARTBEAT_INTERVAL_MS = 15_000;
export const HEARTBEAT_TIMEOUT_MS = 45_000;
export const HEARTBEAT_TICK_MS = 5_000;
/** A tick that comes this late means the page's timers were held back (a hidden page, a sleeping machine), not that
 * the daemon fell silent: the silence is measured afresh from then on. */
const HEARTBEAT_SUSPENDED_GAP_MS = 3 * HEARTBEAT_TICK_MS;
/** Heartbeat ids stand apart from request ids, so their acks are recognised and dropped. */
const HEARTBEAT_ID_PREFIX = "heartbeat-";

/** Whether `reply` is of a type `request` is answered with. */
function answers(request: RequestType, reply: Event): boolean {
  const expected: Event["type"] | readonly Event["type"][] = REPLY_TYPES[request];
  return typeof expected === "string" ? reply.type === expected : expected.includes(reply.type);
}

/** The failure of a request answered with a reply of another type than it is answered with. */
function unexpectedReply(request: RequestType, reply: Event): CodedError {
  return new CodedError(t("error.unexpectedReply"), "unexpected_reply", { request, reply: reply.type });
}

/** `connecting` is the first attempt, before the socket has ever been open — distinct from
 * `reconnecting`, so a window that has never connected does not claim to be re-trying. `open` is a connection whose
 * hello was of this client's generation. `incompatible` is final: the
 * daemon speaks another protocol generation, and the client closed the socket for good. `keyRefused` is final too: the
 * daemon that served the page no longer accepts its key, so no attempt can succeed. */
export type ConnectionState = "connecting" | "open" | "reconnecting" | "closed" | "incompatible" | "keyRefused";

/**
 * The `/ws/control` connection for the lifetime of the window. Every mutating action (`request`)
 * is correlated to its reply by a generated `id`: the daemon answers on the same socket with `ack`
 * or `error` carrying that `id` (or the reply `REPLY_TYPES` names for it), and state changes it
 * broadcasts to every client — including ones this client itself caused — arrive with no `id` at
 * all and are only ever delivered through `onEvent`.
 *
 * This is the only thing in the application that talks to the daemon's control channel; no Tauri
 * IPC is involved anywhere in this path (see "Why the daemon is split out" in
 * `docs/architecture.md`).
 *
 * A dropped connection is retried as the client's `ReconnectPolicy` says, and the daemon re-sends a full `snapshot` on
 * every new control connection, so a reconnect needs no replay logic of its own. By default (`BRIEF_RECONNECT`) that is a
 * couple of quick, backed-off attempts rather than treating the drop as final: where the shell runs the daemon as its
 * sidecar, the daemon dies together with the application, so most drops are either a brief hiccup or the whole process
 * being gone, not something retrying forever would fix. Once the budget is spent, `reconnect()` is the way back —
 * called from a user-facing Retry affordance rather than retried automatically forever. A remote daemon's window is
 * given `ENDLESS_RECONNECT` instead: its drops are the network's, which comes back on its own, so it keeps trying, and
 * `reconnect()` there only tries at once. While the attempt a `reconnect()` started is under way, the client says so
 * (`onRetryPendingChange`), for Retry to show it.
 *
 * A connection that goes silent past the heartbeat's timeout is taken as dropped, with no close event to wait for: a
 * dead connection may never deliver one.
 *
 * Every connection opens with the daemon's `hello`. A daemon of another protocol generation, or one that sends its
 * `snapshot` with no hello before it (older than the hello), is not read on a guess: the client closes for good and
 * goes `incompatible` (see "Versions and compatibility" in `apps/daemon/PROTOCOL.md`).
 *
 * A page the daemon itself served is given `keyCheck`, which says whether the daemon refuses the page's key. It is asked
 * each time a connection is lost, and a refusal ends the client for good as `keyRefused`, with nothing retried: a daemon
 * that restarted with another key refuses every attempt, and retrying only hides why.
 */
export class DaemonClient {
  private readonly url: string;
  private readonly keyCheck?: () => Promise<boolean>;
  private readonly policy: ReconnectPolicy;
  private socket?: WebSocket;
  /** Bumped on every `connect()`, and when a heartbeat timeout disowns its socket; a socket's listeners no-op once
   * their generation is stale. */
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
  private retryListeners = new Set<(pending: boolean) => void>();
  /** Whether the attempt a `reconnect()` started has not opened or failed yet. */
  private retryPending = false;
  /** The generation of the socket that attempt opened, once it has. */
  private retryGeneration?: number;
  private pending = new Map<string, PendingReply>();
  /** Requests made before the socket reaches `OPEN` and its hello is read are queued rather than dropped or thrown, each
   * with the timer that gives up on it while it waits. Once sent, a request waits for its reply as
   * long as it takes, as one sent on an open socket does, so the timer is cleared on the flush. */
  private sendQueue: QueuedRequest[] = [];
  private reconnectAttempts = 0;
  private reconnectTimer?: ReturnType<typeof setTimeout>;
  private connectionState: ConnectionState = "connecting";
  /** Set by `close()` so a deliberate shutdown never schedules a reconnect. */
  private closedPermanently = false;
  /** Whether the current connection's `hello` has been read, and was of this client's generation. */
  private greeted = false;
  /** The last request id handed out. Ids only have to be unique among this client's requests, and a counter needs no
   * secure context, which `crypto.randomUUID` does and the desktop webview's page is not reliably counted as. */
  private lastRequestId = 0;
  /** The current connection's heartbeat tick, from its hello until it is lost or replaced. */
  private heartbeatTimer?: ReturnType<typeof setInterval>;
  /** When the current connection last delivered a frame, or when its silence started being measured. */
  private lastHeard = 0;
  /** The last heartbeat id handed out, numbered apart from request ids. */
  private heartbeatSeq = 0;

  constructor(url: string, keyCheck?: () => Promise<boolean>, policy: ReconnectPolicy = BRIEF_RECONNECT) {
    this.url = url;
    this.keyCheck = keyCheck;
    this.policy = policy;
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
      this.failPending(t("connection.error.connectionReplaced"));
      // What waited there for the hello was failed with the rest, so it must not go out on the new socket either.
      for (const queued of this.sendQueue.splice(0)) clearTimeout(queued.timer);
    }
    this.stopHeartbeat();
    this.socket?.close();
    const socket = new WebSocket(this.url);
    this.socket = socket;
    // A fresh connection opens with a hello again, and nothing is sent on it before that hello is read.
    this.greeted = false;
    socket.addEventListener("message", (ev) => {
      if (!current() || this.closedPermanently) return;
      this.lastHeard = Date.now();
      const event = JSON.parse(ev.data as string) as Event;
      if (event.type === "ack" && event.id?.startsWith(HEARTBEAT_ID_PREFIX)) return;
      if (event.type === "hello") {
        // Into the store first, so the screen that says the versions differ has the daemon's.
        this.receive(event);
        if (event.generation !== PROTOCOL_GENERATION) {
          this.refuse();
          return;
        }
        // The connection is the client's to use only now: a request sent before the hello was read could reach a
        // daemon that would take it in another generation's meaning.
        this.greeted = true;
        for (const queued of this.sendQueue.splice(0)) {
          clearTimeout(queued.timer);
          socket.send(queued.payload);
        }
        this.startHeartbeat(socket);
        this.setConnectionState("open");
        this.endRetry();
        return;
      }
      if (event.type === "snapshot" && !this.greeted) {
        this.refuse();
        return;
      }
      // The budget is given back only once the connection has proved itself by delivering its
      // snapshot, not on `open`: the daemon accepts the socket before it builds the snapshot, and
      // one that cannot build it closes the socket right after, so resetting on `open` would retry
      // that daemon forever.
      if (event.type === "snapshot") this.reconnectAttempts = 0;
      this.receive(event);
    });
    socket.addEventListener("close", () => {
      if (!current()) return;
      this.lose();
    });
  }

  /** The current connection is gone, whether its socket closed or its heartbeat timed out: what it had outstanding
   * fails, and a reconnect is scheduled while the policy's attempts last. */
  private lose(): void {
    this.stopHeartbeat();
    // What arrived before the drop is delivered ahead of the state change it precedes.
    this.flushBroadcasts();
    // Whatever this socket had outstanding cannot be answered by a reconnect's new socket — the
    // daemon has no memory of a prior connection's in-flight requests — so these are rejected
    // unconditionally rather than held pending a retry.
    this.failPending(t("connection.error.connectionClosed"));
    // Rejecting and retrying are contradictory: a payload still queued here would otherwise be
    // flushed on the next connection and run an action the caller was already told had failed.
    for (const queued of this.sendQueue.splice(0)) clearTimeout(queued.timer);
    if (this.closedPermanently) return;
    if (this.generation === this.retryGeneration) this.endRetry();
    this.checkKey();
    if (this.retryPending) {
      // Retry's own attempt is being prepared (`beforeRetry`) and follows: no automatic one is made before it.
      this.setConnectionState("reconnecting");
      return;
    }
    if (this.reconnectAttempts < this.policy.attempts) {
      const delay = this.policy.delay(this.reconnectAttempts);
      this.reconnectAttempts += 1;
      this.setConnectionState("reconnecting");
      this.reconnectTimer = setTimeout(() => this.connect(), delay);
    } else {
      this.setConnectionState("closed");
    }
  }

  private startHeartbeat(socket: WebSocket): void {
    this.stopHeartbeat();
    let lastTick = Date.now();
    let lastPing = lastTick;
    this.lastHeard = lastTick;
    this.heartbeatTimer = setInterval(() => {
      const now = Date.now();
      // A clock set back is taken like held-back timers, so that no deadline is pushed into the future.
      if (now - lastTick > HEARTBEAT_SUSPENDED_GAP_MS || now < lastTick) {
        this.lastHeard = now;
        lastPing = Math.min(lastPing, now);
      }
      lastTick = now;
      if (now - this.lastHeard >= HEARTBEAT_TIMEOUT_MS) {
        // The socket's own close may never come, or come long after; its listeners are disowned by moving to the next
        // generation, and the loss is taken here instead.
        this.generation += 1;
        socket.close();
        this.lose();
        return;
      }
      if (now - lastPing >= HEARTBEAT_INTERVAL_MS) {
        lastPing = now;
        const ping: ClientRequest = { type: "ping", id: `${HEARTBEAT_ID_PREFIX}${++this.heartbeatSeq}` };
        socket.send(JSON.stringify(ping));
      }
    }, HEARTBEAT_TICK_MS);
  }

  private stopHeartbeat(): void {
    clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = undefined;
  }

  /** Gives up on a daemon of another protocol generation: what it sends next cannot be read, and a reconnect would meet
   * the same daemon, so the socket is closed for good and nothing is retried. */
  private refuse(): void {
    this.giveUp("incompatible");
  }

  /** Asks `keyCheck`, alongside the reconnect the loss schedules, whether the daemon refuses the key. An answer that
   * arrives after another attempt began is dropped: that attempt's own loss asks again. */
  private checkKey(): void {
    if (!this.keyCheck) return;
    const generation = this.generation;
    void this.keyCheck().then((refused) => {
      if (refused && generation === this.generation && !this.closedPermanently) this.giveUp("keyRefused");
    });
  }

  /** Ends the client for good in the final state `state`, failing what it had outstanding. */
  private giveUp(state: "incompatible" | "keyRefused"): void {
    this.close();
    this.endRetry();
    this.failPending(t("connection.error.connectionClosed"));
    for (const queued of this.sendQueue.splice(0)) clearTimeout(queued.timer);
    this.setConnectionState(state);
  }

  /** Retries right away, with the automatic attempts' waits started over — the user-facing Retry calls this, after
   * the automatic attempts were spent or, with attempts that do not end, during a wait for the next one; nothing calls
   * it automatically. Does nothing once the client is closed for good. */
  reconnect(): void {
    if (this.closedPermanently || this.retryPending) return;
    this.reconnectAttempts = 0;
    this.retryGeneration = undefined;
    this.setRetryPending(true);
    // The banner says a retry is under way until the socket opens or drops again.
    this.setConnectionState("reconnecting");
    const attempt = () => {
      if (this.closedPermanently || !this.retryPending) return;
      // An automatic attempt that connected meanwhile is kept.
      if (this.connectionState === "open") {
        this.endRetry();
        return;
      }
      this.connect();
      this.retryGeneration = this.generation;
    };
    const before = this.policy.beforeRetry;
    if (!before) {
      attempt();
      return;
    }
    // No automatic attempt is made while Retry's is prepared: the wait under way ends here, and a loss meanwhile
    // schedules none (`lose`). A preparation that does not settle is not waited for past `BEFORE_RETRY_TIMEOUT_MS`.
    clearTimeout(this.reconnectTimer);
    this.reconnectTimer = undefined;
    let bound: ReturnType<typeof setTimeout> | undefined;
    const prepared = before().catch(() => {});
    const bounded = new Promise<void>((resolve) => (bound = setTimeout(resolve, BEFORE_RETRY_TIMEOUT_MS)));
    void Promise.race([prepared, bounded]).then(() => {
      clearTimeout(bound);
      attempt();
    });
  }

  /** Says the daemon can be reached again (for a remote daemon's window, its SSH connection is back): a wait for the
   * next automatic attempt ends at once, and the waits start over. An attempt under way is left to finish. Not the
   * user's Retry: nothing is shown pending. */
  reachable(): void {
    if (this.closedPermanently || this.connectionState === "open") return;
    this.reconnectAttempts = 0;
    if (this.reconnectTimer === undefined) return;
    this.connect();
  }

  /** Subscribes to whether the attempt a `reconnect()` started is under way, reporting the current value straight
   * away. */
  onRetryPendingChange(listener: (pending: boolean) => void): () => void {
    this.retryListeners.add(listener);
    listener(this.retryPending);
    return () => this.retryListeners.delete(listener);
  }

  private endRetry(): void {
    this.retryGeneration = undefined;
    this.setRetryPending(false);
  }

  private setRetryPending(pending: boolean): void {
    if (this.retryPending === pending) return;
    this.retryPending = pending;
    for (const listener of this.retryListeners) listener(pending);
  }

  /** Fails every request still waiting for an answer. A reconnect's socket is a fresh connection
   * with no memory of them, so they can only ever be reported as failed. */
  private failPending(reason: string): void {
    for (const pending of this.pending.values()) {
      pending.reject(new CodedError(reason, "connection_closed", {}));
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
      if (event.type === "error") pending.reject(new CodedError(event.message, event.code, event.params));
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

  /** Sends a request and resolves with its correlated reply, or rejects with the `error` message (as a `CodedError`,
   * carrying its `code` and `params`). A reply of a type the request is not answered with (`REPLY_TYPES`) rejects it
   * as `unexpected_reply`. */
  readonly request: RequestSender = (body) => {
    const id = String(++this.lastRequestId);
    const payload: ClientRequest = { ...body, id };
    const json = JSON.stringify(payload);
    return new Promise((resolve, reject) => {
      const settle = (event: Event) => {
        // `answers` has checked that the event is of the type `ReplyOf` names for this request.
        if (answers(body.type, event)) resolve(event as never);
        else reject(unexpectedReply(body.type, event));
      };
      if (this.closedPermanently) {
        reject(new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {}));
        return;
      }
      const socket = this.socket;
      if (socket?.readyState === WebSocket.OPEN && this.greeted) {
        this.pending.set(id, { resolve: settle, reject });
        socket.send(json);
        return;
      }
      // A socket a heartbeat timeout gave up on may stay `CLOSING` for long, its close never answered.
      if (socket !== undefined && socket.readyState >= WebSocket.CLOSING && !this.reconnectTimer) {
        // The socket is dead and no retry is scheduled (the reconnect budget is spent) — queuing
        // would hang the caller forever, which a quit's `shutdown` request must never do.
        reject(new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {}));
        return;
      }
      // Still connecting, waiting for the hello, or a reconnect is scheduled: queue it, but bounded — the daemon might
      // never come back, and nothing else will reject this once `sendQueue` is the only place it
      // lives.
      this.pending.set(id, { resolve: settle, reject });
      const queued: QueuedRequest = {
        payload: json,
        timer: setTimeout(() => {
          if (!this.pending.delete(id)) return;
          const queuedIndex = this.sendQueue.indexOf(queued);
          if (queuedIndex !== -1) this.sendQueue.splice(queuedIndex, 1);
          reject(new CodedError(t("connection.error.timedOut"), "request_timed_out", {}));
        }, QUEUED_REQUEST_TIMEOUT_MS),
      };
      this.sendQueue.push(queued);
    });
  };

  close(): void {
    this.closedPermanently = true;
    this.stopHeartbeat();
    this.flushBroadcasts();
    clearTimeout(this.reconnectTimer);
    this.reconnectTimer = undefined;
    this.socket?.close();
  }
}
