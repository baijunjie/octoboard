// @vitest-environment jsdom
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "./fixtures/builders";
import { PROTOCOL_GENERATION, type Event } from "./protocol";
import {
  BROADCAST_FLUSH_FALLBACK_MS, DaemonClient, HEARTBEAT_INTERVAL_MS, HEARTBEAT_TICK_MS, HEARTBEAT_TIMEOUT_MS, MAX_RECONNECT_ATTEMPTS,
  QUEUED_REQUEST_TIMEOUT_MS, RECONNECT_BASE_DELAY_MS,
} from "./daemon-client";
import { CodedError } from "./errors/codedError";
import { setLanguage } from "./i18n/language";
import { fakeTimers } from "./test/clock";

class FakeSocket extends EventTarget {
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static last: FakeSocket;
  static all: FakeSocket[] = [];
  readyState = FakeSocket.OPEN;
  sent: string[] = [];
  constructor() {
    super();
    FakeSocket.last = this;
    FakeSocket.all.push(this);
  }
  send(payload: string): void {
    this.sent.push(payload);
  }
  close(): void {}
  receive(event: object): void {
    this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(event) }));
  }
}

/** The wait before automatic reconnect attempt number `attempt` (counting from 0). */
const backoff = (attempt: number) => RECONNECT_BASE_DELAY_MS * 2 ** attempt;
/** Fake time that outlasts every automatic reconnect: the waits of attempts 0 up to and including the one past the budget. */
const PAST_ANY_RETRY_MS = Array.from({ length: MAX_RECONNECT_ATTEMPTS + 1 }, (_, attempt) => backoff(attempt)).reduce((sum, wait) => sum + wait, 0);

let frame: (() => void) | undefined;
const clients: DaemonClient[] = [];

/** A client that is closed when the test ends, so that no reconnect timer is left to open a socket after the stubs are gone. */
function newClient(keyCheck?: () => Promise<boolean>): DaemonClient {
  const client = new DaemonClient("ws://test", keyCheck);
  clients.push(client);
  return client;
}

beforeEach(() => {
  // Registered before `fakeTimers`, so it runs after its run-out: the timers still pending meet the fakes, not a real socket.
  onTestFinished(() => {
    FakeSocket.all = [];
    vi.unstubAllGlobals();
    frame = undefined;
  });
  fakeTimers({ beforeDrain: () => clients.splice(0).forEach((client) => client.close()) });
  // After `fakeTimers`, which fakes the frame functions too.
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => ((frame = callback), 1));
  vi.stubGlobal("cancelAnimationFrame", () => (frame = undefined));
});

const upserted = (id: string): Event => ({ type: "session_upserted", session: sessionOf(id, "c", "p", id, "working") });
const open = (socket: FakeSocket) => socket.dispatchEvent(new globalThis.Event("open"));
const drop = (socket: FakeSocket) => socket.dispatchEvent(new globalThis.Event("close"));
// The client reads only a snapshot's `type`.
const snapshot = { type: "snapshot" };
const hello = (generation = PROTOCOL_GENERATION): Event => ({ type: "hello", generation, daemon_version: "1.4.0" });
/** A connection opening as the daemon opens it: its hello, then its snapshot. */
const greet = (socket: FakeSocket) => {
  socket.receive(hello());
  socket.receive(snapshot);
};

it("delivers the broadcasts that arrive between two frames as one batch, in arrival order", () => {
  const client = newClient();
  const batches: Event[][] = [];
  const single: string[] = [];
  client.onEvents((events) => batches.push(events));
  client.onEvent((event) => single.push((event as { session: { id: string } }).session.id));
  FakeSocket.last.receive(upserted("a"));
  FakeSocket.last.receive(upserted("b"));
  FakeSocket.last.receive(upserted("c"));
  expect(batches).toEqual([]);
  frame!();
  expect(batches).toHaveLength(1);
  expect(batches[0].map((event) => (event as { session: { id: string } }).session.id)).toEqual(["a", "b", "c"]);
  expect(single).toEqual(["a", "b", "c"]);
});

it("delivers queued broadcasts without a frame once the fallback timer fires, as in a hidden window", () => {
  const client = newClient();
  const batches: Event[][] = [];
  client.onEvents((events) => batches.push(events));
  FakeSocket.last.receive(upserted("a"));
  vi.advanceTimersByTime(BROADCAST_FLUSH_FALLBACK_MS);
  expect(batches).toHaveLength(1);
  // The frame that was scheduled is cancelled with the delivery, so nothing arrives twice.
  frame?.();
  expect(batches).toHaveLength(1);
});

it("answers a request at once, behind the broadcasts that arrived before its reply", async () => {
  const client = newClient();
  const order: string[] = [];
  FakeSocket.last.receive(hello());
  frame!();
  client.onEvents((events) => order.push(...events.map((event) => event.type)));
  const reply = client.request({ type: "shutdown" }).then(() => order.push("resolved"));
  const id = (JSON.parse(FakeSocket.last.sent[0]) as { id: string }).id;
  FakeSocket.last.receive(upserted("a"));
  FakeSocket.last.receive({ type: "ack", id });
  await reply;
  expect(order).toEqual(["session_upserted", "ack", "resolved"]);
});

it("fails a request the reconnect cut off in the UI's language, as a closed connection", async () => {
  setLanguage("zh-Hans");
  try {
    const client = newClient();
    const reply = client.request({ type: "shutdown" });
    client.reconnect();
    const error = await reply.catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(CodedError);
    expect(error).toMatchObject({ code: "connection_closed", message: "守护进程控制连接在收到回复前已重新建立。" });
    // It waited for a hello that never came, and is not sent on the new socket after it was failed.
    FakeSocket.last.receive(hello());
    expect(FakeSocket.all.flatMap((socket) => socket.sent)).toEqual([]);
  } finally {
    setLanguage("en");
  }
});

it("keeps waiting for a request queued while connecting once the socket opens and its hello arrives, and sends it", async () => {
  const client = newClient();
  const socket = FakeSocket.last;
  socket.readyState = 0;
  let settled: unknown;
  const reply = client.request({ type: "shutdown" }).then(
    (event) => (settled = event),
    (error: unknown) => (settled = error),
  );
  socket.readyState = FakeSocket.OPEN;
  open(socket);
  // Nothing goes out before the hello is read.
  expect(socket.sent).toEqual([]);
  socket.receive(hello());
  const id = (JSON.parse(socket.sent[0]) as { id: string }).id;
  vi.advanceTimersByTime(QUEUED_REQUEST_TIMEOUT_MS * 2);
  await Promise.resolve();
  expect(settled).toBeUndefined();
  socket.receive({ type: "ack", id });
  await reply;
  expect(settled).toEqual({ type: "ack", id });
});

it("spends the reconnect budget on a daemon that accepts the socket and closes it before any snapshot", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt += 1) {
    open(FakeSocket.last);
    drop(FakeSocket.last);
    expect(states.at(-1)).toBe("reconnecting");
    vi.advanceTimersByTime(backoff(attempt));
  }
  open(FakeSocket.last);
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("closed");
  // Nothing is retried on its own once the budget is spent.
  const last = FakeSocket.last;
  vi.advanceTimersByTime(PAST_ANY_RETRY_MS);
  expect(FakeSocket.last).toBe(last);
});

it("gives a connection that delivered its snapshot the whole budget again when it drops", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  // One attempt is spent on a socket that closed before its snapshot.
  open(FakeSocket.last);
  drop(FakeSocket.last);
  vi.advanceTimersByTime(backoff(0));
  // The next one works, then drops.
  open(FakeSocket.last);
  greet(FakeSocket.last);
  expect(states.at(-1)).toBe("open");
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("reconnecting");
  // Both automatic attempts are available again.
  vi.advanceTimersByTime(backoff(0));
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("reconnecting");
  vi.advanceTimersByTime(backoff(1));
  open(FakeSocket.last);
  greet(FakeSocket.last);
  expect(states.at(-1)).toBe("open");
});

it("reads a hello of its own generation and goes on to the snapshot", () => {
  const client = newClient();
  const states: string[] = [];
  const received: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  client.onEvent((event) => received.push(event.type));
  open(FakeSocket.last);
  greet(FakeSocket.last);
  frame!();
  expect(received).toEqual(["hello", "snapshot"]);
  expect(states.at(-1)).toBe("open");
});

it("closes for good on a hello of another generation, after handing the hello on, and never reconnects", async () => {
  const client = newClient();
  const states: string[] = [];
  const received: Event[] = [];
  client.onConnectionChange((state) => states.push(state));
  client.onEvent((event) => received.push(event));
  const socket = FakeSocket.last;
  const close = vi.spyOn(socket, "close");
  open(socket);
  socket.receive(hello(PROTOCOL_GENERATION + 1));
  // What follows is of a generation the client cannot read, so it is not handed on.
  socket.receive(snapshot);
  expect(received).toEqual([hello(PROTOCOL_GENERATION + 1)]);
  expect(close).toHaveBeenCalled();
  expect(states.at(-1)).toBe("incompatible");
  drop(socket);
  vi.advanceTimersByTime(PAST_ANY_RETRY_MS);
  client.reconnect();
  expect(FakeSocket.all).toHaveLength(1);
  expect(states.at(-1)).toBe("incompatible");
  const refused = await client.request({ type: "shutdown" }).catch((cause: unknown) => cause);
  expect(refused).toMatchObject({ code: "connection_closed" });
});

it("ends for good as key refused when a lost connection's key check says the daemon refuses the key", async () => {
  const keyCheck = vi.fn(() => Promise.resolve(true));
  const client = newClient(keyCheck);
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  const reply = client.request({ type: "shutdown" }).catch((cause: unknown) => cause);
  const socket = FakeSocket.last;
  drop(socket);
  expect(keyCheck).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(0);
  expect(states.at(-1)).toBe("keyRefused");
  expect(await reply).toMatchObject({ code: "connection_closed" });
  // No attempt follows, automatic or by hand.
  vi.advanceTimersByTime(PAST_ANY_RETRY_MS);
  client.reconnect();
  expect(FakeSocket.all).toHaveLength(1);
  expect(states.at(-1)).toBe("keyRefused");
  const refused = await client.request({ type: "shutdown" }).catch((cause: unknown) => cause);
  expect(refused).toMatchObject({ code: "connection_closed" });
});

it("keeps reconnecting as before when the key check does not say the key was refused", async () => {
  const keyCheck = vi.fn(() => Promise.resolve(false));
  const client = newClient(keyCheck);
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt += 1) {
    drop(FakeSocket.last);
    await vi.advanceTimersByTimeAsync(backoff(attempt));
    expect(states.at(-1)).toBe("reconnecting");
  }
  drop(FakeSocket.last);
  await vi.advanceTimersByTimeAsync(0);
  expect(states.at(-1)).toBe("closed");
  expect(FakeSocket.all).toHaveLength(MAX_RECONNECT_ATTEMPTS + 1);
  expect(keyCheck).toHaveBeenCalledTimes(MAX_RECONNECT_ATTEMPTS + 1);
});

it("drops a key refusal that arrives after the next attempt began", async () => {
  let answer: (refused: boolean) => void = () => {};
  const client = newClient(() => new Promise<boolean>((resolve) => (answer = resolve)));
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  drop(FakeSocket.last);
  vi.advanceTimersByTime(backoff(0));
  expect(FakeSocket.all).toHaveLength(2);
  answer(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(states.at(-1)).toBe("reconnecting");
});

it("never asks a key check of a connection that is up or a client that was closed", async () => {
  const keyCheck = vi.fn(() => Promise.resolve(true));
  const client = newClient(keyCheck);
  open(FakeSocket.last);
  greet(FakeSocket.last);
  client.close();
  drop(FakeSocket.last);
  await vi.advanceTimersByTimeAsync(0);
  expect(keyCheck).not.toHaveBeenCalled();
});

it("takes a snapshot with no hello before it as a daemon of another generation", () => {
  const client = newClient();
  const states: string[] = [];
  const received: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  client.onEvent((event) => received.push(event.type));
  open(FakeSocket.last);
  FakeSocket.last.receive(snapshot);
  frame?.();
  expect(received).toEqual([]);
  expect(states.at(-1)).toBe("incompatible");
});

it("expects a hello again on every new connection", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  open(FakeSocket.last);
  greet(FakeSocket.last);
  drop(FakeSocket.last);
  vi.advanceTimersByTime(backoff(0));
  open(FakeSocket.last);
  FakeSocket.last.receive(snapshot);
  expect(states.at(-1)).toBe("incompatible");
});

it("rejects a reply of another type than its request is answered with as an unexpected reply", async () => {
  const client = newClient();
  FakeSocket.last.receive(hello());
  const reply = client.request({ type: "list_all_processes" });
  const id = (JSON.parse(FakeSocket.last.sent[0]) as { id: string }).id;
  FakeSocket.last.receive({ type: "ack", id });
  const error = await reply.catch((cause: unknown) => cause);
  expect(error).toBeInstanceOf(CodedError);
  expect(error).toMatchObject({ code: "unexpected_reply", params: { request: "list_all_processes", reply: "ack" } });
});

it("fails a request made after close at once, as a closed connection", async () => {
  const client = newClient();
  const socket = FakeSocket.last;
  drop(socket);
  // A reconnect was scheduled before the window closed the client for good.
  client.close();
  let settled: unknown;
  void client.request({ type: "shutdown" }).catch((error: unknown) => (settled = error));
  await Promise.resolve();
  expect(settled).toMatchObject({ code: "connection_closed" });
  expect(socket.sent).toEqual([]);
});

it("reports a Retry as reconnecting until the socket opens with its hello", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt += 1) {
    drop(FakeSocket.last);
    vi.advanceTimersByTime(backoff(attempt));
  }
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("closed");
  client.reconnect();
  expect(states.at(-1)).toBe("reconnecting");
  open(FakeSocket.last);
  expect(states.at(-1)).toBe("reconnecting");
  FakeSocket.last.receive(hello());
  expect(states.at(-1)).toBe("open");
});

it("gives every request its own id, across reconnects", () => {
  const client = newClient();
  FakeSocket.last.receive(hello());
  void client.request({ type: "shutdown" }).catch(() => {});
  client.reconnect();
  FakeSocket.last.receive(hello());
  void client.request({ type: "shutdown" }).catch(() => {});
  const ids = [...FakeSocket.all.flatMap((socket) => socket.sent)].map((sent) => (JSON.parse(sent) as { id: string }).id);
  expect(ids).toHaveLength(2);
  expect(new Set(ids).size).toBe(2);
});

/** The heartbeat pings this socket has sent, by id. */
const pings = (socket: FakeSocket) =>
  socket.sent
    .map((payload) => JSON.parse(payload) as { type: string; id: string })
    .filter((request) => request.type === "ping")
    .map((request) => request.id);

it("pings an open connection and keeps it while the daemon answers, without passing the answers on", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  const socket = FakeSocket.last;
  greet(socket);
  const heard: Event[] = [];
  client.onEvent((event) => heard.push(event));
  for (let beat = 0; beat < 8; beat += 1) {
    vi.advanceTimersByTime(HEARTBEAT_INTERVAL_MS);
    const id = pings(socket).at(-1)!;
    socket.receive({ type: "ack", id });
  }
  vi.advanceTimersByTime(HEARTBEAT_TICK_MS);
  expect(pings(socket)).toHaveLength(8);
  expect(new Set(pings(socket)).size).toBe(8);
  expect(heard.filter((event) => event.type === "ack")).toEqual([]);
  expect(FakeSocket.all).toHaveLength(1);
  expect(states.at(-1)).toBe("open");
});

it("takes a connection that falls silent for the heartbeat timeout as lost, with no close from its socket", async () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  const socket = FakeSocket.last;
  const closed = vi.spyOn(socket, "close");
  greet(socket);
  const reply = client.request({ type: "shutdown" }).catch((cause: unknown) => cause);
  vi.advanceTimersByTime(HEARTBEAT_TIMEOUT_MS - HEARTBEAT_TICK_MS);
  expect(states.at(-1)).toBe("open");
  vi.advanceTimersByTime(HEARTBEAT_TICK_MS);
  expect(states.at(-1)).toBe("reconnecting");
  expect(closed).toHaveBeenCalled();
  expect(await reply).toMatchObject({ code: "connection_closed" });
  // The dead socket's close, should it come before the reconnect, is not taken as a second loss.
  drop(socket);
  expect(states.filter((state) => state === "reconnecting")).toHaveLength(1);
  // The lost-connection path reconnects once, and the new connection reads the state again.
  vi.advanceTimersByTime(HEARTBEAT_TICK_MS);
  expect(FakeSocket.all).toHaveLength(2);
  greet(FakeSocket.last);
  expect(states.at(-1)).toBe("open");
});

it("measures the silence afresh after the page's timers were held back", () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  greet(FakeSocket.last);
  // A machine asleep for a minute: the clock moves on while no timer runs.
  vi.setSystemTime(Date.now() + HEARTBEAT_TIMEOUT_MS + HEARTBEAT_TICK_MS);
  vi.advanceTimersByTime(HEARTBEAT_TICK_MS);
  expect(states.at(-1)).toBe("open");
  vi.advanceTimersByTime(HEARTBEAT_TIMEOUT_MS);
  expect(states.at(-1)).toBe("reconnecting");
});

it("fails a request at once when the reconnects after a heartbeat loss are spent and the dead socket never closed", async () => {
  const client = newClient();
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const socket = FakeSocket.last;
    socket.close = () => (socket.readyState = FakeSocket.CLOSING);
    // Greeted but never sent its snapshot, so the connection never earns its reconnect budget back.
    socket.receive(hello());
    vi.advanceTimersByTime(HEARTBEAT_TIMEOUT_MS);
    vi.advanceTimersByTime(backoff(MAX_RECONNECT_ATTEMPTS));
  }
  expect(states.at(-1)).toBe("closed");
  const error = await client.request({ type: "shutdown" }).catch((cause: unknown) => cause);
  expect(error).toMatchObject({ code: "connection_closed" });
});
