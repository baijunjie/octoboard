// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import type { Event } from "./protocol";
import { DaemonClient, DaemonRequestError } from "./daemon-client";
import { setLanguage } from "./i18n/language";

class FakeSocket extends EventTarget {
  static OPEN = 1;
  static CLOSED = 3;
  static last: FakeSocket;
  readyState = FakeSocket.OPEN;
  sent: string[] = [];
  constructor() {
    super();
    FakeSocket.last = this;
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
  vi.useRealTimers();
  vi.unstubAllGlobals();
  frame = undefined;
});

const upserted = (id: string): Event => ({ type: "session_upserted", session: sessionOf(id, "c", "p", id, "working") });

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
    expect(error).toBeInstanceOf(DaemonRequestError);
    expect(error).toMatchObject({ code: "connection_closed", message: "守护进程控制连接在收到回复前已重新建立。" });
  } finally {
    setLanguage("en");
  }
});
