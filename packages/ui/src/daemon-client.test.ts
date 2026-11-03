// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { sessionOf } from "./fixtures/builders";
import type { Event } from "./protocol";
import { DaemonClient } from "./daemon-client";
import { CodedError } from "./errors/codedError";
import { setLanguage } from "./i18n/language";

class FakeSocket extends EventTarget {
  static OPEN = 1;
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

let frame: (() => void) | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("WebSocket", FakeSocket);
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => ((frame = callback), 1));
  vi.stubGlobal("cancelAnimationFrame", () => (frame = undefined));
});

afterEach(() => {
  FakeSocket.all = [];
  vi.useRealTimers();
  vi.unstubAllGlobals();
  frame = undefined;
});

const upserted = (id: string): Event => ({ type: "session_upserted", session: sessionOf(id, "c", "p", id, "working") });
const open = (socket: FakeSocket) => socket.dispatchEvent(new globalThis.Event("open"));
const drop = (socket: FakeSocket) => socket.dispatchEvent(new globalThis.Event("close"));
// The client reads only a snapshot's `type`.
const snapshot = { type: "snapshot" };

it("delivers the broadcasts that arrive between two frames as one batch, in arrival order", () => {
  const client = new DaemonClient("ws://test");
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
  const client = new DaemonClient("ws://test");
  const batches: Event[][] = [];
  client.onEvents((events) => batches.push(events));
  FakeSocket.last.receive(upserted("a"));
  vi.advanceTimersByTime(250);
  expect(batches).toHaveLength(1);
  // The frame that was scheduled is cancelled with the delivery, so nothing arrives twice.
  frame?.();
  expect(batches).toHaveLength(1);
});

it("answers a request at once, behind the broadcasts that arrived before its reply", async () => {
  const client = new DaemonClient("ws://test");
  const order: string[] = [];
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
    const client = new DaemonClient("ws://test");
    const reply = client.request({ type: "shutdown" });
    client.reconnect();
    const error = await reply.catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(CodedError);
    expect(error).toMatchObject({ code: "connection_closed", message: "守护进程控制连接在收到回复前已重新建立。" });
  } finally {
    setLanguage("en");
  }
});

it("keeps waiting for a request queued while connecting once the socket opens and sends it", async () => {
  const client = new DaemonClient("ws://test");
  const socket = FakeSocket.last;
  socket.readyState = 0;
  let settled: unknown;
  const reply = client.request({ type: "shutdown" }).then(
    (event) => (settled = event),
    (error: unknown) => (settled = error),
  );
  socket.readyState = FakeSocket.OPEN;
  open(socket);
  const id = (JSON.parse(socket.sent[0]) as { id: string }).id;
  vi.advanceTimersByTime(10_000);
  await Promise.resolve();
  expect(settled).toBeUndefined();
  socket.receive({ type: "ack", id });
  await reply;
  expect(settled).toEqual({ type: "ack", id });
});

it("spends the reconnect budget on a daemon that accepts the socket and closes it before any snapshot", () => {
  const client = new DaemonClient("ws://test");
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (const delay of [500, 1000]) {
    open(FakeSocket.last);
    drop(FakeSocket.last);
    expect(states.at(-1)).toBe("reconnecting");
    vi.advanceTimersByTime(delay);
  }
  open(FakeSocket.last);
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("closed");
  // Nothing is retried on its own once the budget is spent.
  const last = FakeSocket.last;
  vi.advanceTimersByTime(60_000);
  expect(FakeSocket.last).toBe(last);
});

it("gives a connection that delivered its snapshot the whole budget again when it drops", () => {
  const client = new DaemonClient("ws://test");
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  // One attempt is spent on a socket that closed before its snapshot.
  open(FakeSocket.last);
  drop(FakeSocket.last);
  vi.advanceTimersByTime(500);
  // The next one works, then drops.
  open(FakeSocket.last);
  FakeSocket.last.receive(snapshot);
  expect(states.at(-1)).toBe("open");
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("reconnecting");
  // Both automatic attempts are available again.
  vi.advanceTimersByTime(500);
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("reconnecting");
  vi.advanceTimersByTime(1000);
  open(FakeSocket.last);
  FakeSocket.last.receive(snapshot);
  expect(states.at(-1)).toBe("open");
});

it("rejects a reply of another type than its request is answered with as an unexpected reply", async () => {
  const client = new DaemonClient("ws://test");
  const reply = client.request({ type: "list_all_processes" });
  const id = (JSON.parse(FakeSocket.last.sent[0]) as { id: string }).id;
  FakeSocket.last.receive({ type: "ack", id });
  const error = await reply.catch((cause: unknown) => cause);
  expect(error).toBeInstanceOf(CodedError);
  expect(error).toMatchObject({ code: "unexpected_reply", params: { request: "list_all_processes", reply: "ack" } });
});

it("fails a request made after close at once, as a closed connection", async () => {
  const client = new DaemonClient("ws://test");
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

it("reports a Retry as reconnecting until the socket opens", () => {
  const client = new DaemonClient("ws://test");
  const states: string[] = [];
  client.onConnectionChange((state) => states.push(state));
  for (const delay of [500, 1000]) {
    drop(FakeSocket.last);
    vi.advanceTimersByTime(delay);
  }
  drop(FakeSocket.last);
  expect(states.at(-1)).toBe("closed");
  client.reconnect();
  expect(states.at(-1)).toBe("reconnecting");
  open(FakeSocket.last);
  expect(states.at(-1)).toBe("open");
});

it("gives every request its own id, across reconnects", () => {
  const client = new DaemonClient("ws://test");
  void client.request({ type: "shutdown" }).catch(() => {});
  client.reconnect();
  void client.request({ type: "shutdown" }).catch(() => {});
  const ids = [...FakeSocket.all.flatMap((socket) => socket.sent)].map((sent) => (JSON.parse(sent) as { id: string }).id);
  expect(ids).toHaveLength(2);
  expect(new Set(ids).size).toBe(2);
});
