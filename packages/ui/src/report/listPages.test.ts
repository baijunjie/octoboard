// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { pageOf } from "../fixtures/builders";
import { applyDaemonEvent, createStateStore, type Daemon } from "../store";
import { listPages } from "./listPages";

const requester = (reply: () => Promise<unknown> = async () => ({ type: "ack", id: null })) => vi.fn(reply) as unknown as Daemon["request"] & ReturnType<typeof vi.fn>;

/** What a window asks with: the request and a store of its own. */
const daemonOf = (request: Daemon["request"]) => ({ request, store: createStateStore() });

it("shares one listing per session and snapshot epoch, in flight or answered", async () => {
  const request = requester();
  await Promise.all([listPages(daemonOf(request), "s", 1), listPages(daemonOf(request), "s", 1)]);
  await listPages(daemonOf(request), "s", 1);
  expect(request).toHaveBeenCalledTimes(1);

  await listPages(daemonOf(request), "other", 1);
  await listPages(daemonOf(request), "s", 2);
  expect(request).toHaveBeenCalledTimes(3);
});

it("lists again after a failure", async () => {
  const failing = requester(async () => {
    throw new Error("offline");
  });
  await expect(listPages(daemonOf(failing), "s", 1)).rejects.toThrow("offline");
  await expect(listPages(daemonOf(failing), "s", 1)).rejects.toThrow("offline");
  expect(failing).toHaveBeenCalledTimes(2);
});

it("keeps a page_submission heard before the list's reply, and applies it once the list lands", async () => {
  const daemon = daemonOf(requester(() => new Promise(() => {})));
  void listPages(daemon, "s", 1);
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "s", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  expect(daemon.store.getState().earlySubmissions.size).toBe(1);
  applyDaemonEvent(daemon.store, { type: "page_list", id: "r", session_id: "s", pages: [pageOf("p", "s", { body: [] }, 0)] });
  expect(daemon.store.getState().pages.get("s")?.[0]).toMatchObject({ submission: { who: "Ada" }, submission_revision: 1 });
  expect(daemon.store.getState().earlySubmissions.size).toBe(0);
});

it("forgets that a list is out when the request fails", async () => {
  const daemon = daemonOf(requester(async () => {
    throw new Error("offline");
  }));
  await expect(listPages(daemon, "s", 1)).rejects.toThrow("offline");
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "s", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  expect(daemon.store.getState().earlySubmissions.size).toBe(0);
});

it("keeps a heard page_submission for the list a newer snapshot asked for when the older listing fails after it", async () => {
  let failFirst!: (error: Error) => void;
  const request = vi.fn()
    .mockReturnValueOnce(new Promise((_, reject) => void (failFirst = reject)))
    .mockReturnValue(new Promise(() => {}));
  const daemon = daemonOf(request as unknown as Daemon["request"]);
  const first = listPages(daemon, "s", 1);
  // A snapshot has come, and the window asks again for its pages.
  void listPages(daemon, "s", 2);
  failFirst(new Error("offline"));
  await expect(first).rejects.toThrow("offline");
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "s", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  expect(daemon.store.getState().earlySubmissions.get("s")?.has("p")).toBe(true);
});

it("takes the note back when the newer listing fails too", async () => {
  const failures: Array<(error: Error) => void> = [];
  const request = vi.fn(() => new Promise((_, reject) => void failures.push(reject)));
  const daemon = daemonOf(request as unknown as Daemon["request"]);
  const first = listPages(daemon, "s", 1);
  const second = listPages(daemon, "s", 2);
  failures[0](new Error("offline"));
  await expect(first).rejects.toThrow("offline");
  failures[1](new Error("offline"));
  await expect(second).rejects.toThrow("offline");
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "s", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  expect(daemon.store.getState().earlySubmissions.size).toBe(0);
});

it("does not take another session's note back when a listing of this one fails", async () => {
  let fail!: (error: Error) => void;
  const request = vi.fn().mockReturnValueOnce(new Promise((_, reject) => void (fail = reject))).mockReturnValue(new Promise(() => {}));
  const daemon = daemonOf(request as unknown as Daemon["request"]);
  const failing = listPages(daemon, "s", 1);
  void listPages(daemon, "t", 1);
  fail(new Error("offline"));
  await expect(failing).rejects.toThrow("offline");
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "t", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  applyDaemonEvent(daemon.store, { type: "page_submission", session_id: "s", page: "p", submission: { who: "Ada" }, submission_revision: 1 });
  expect([...daemon.store.getState().earlySubmissions.keys()]).toEqual(["t"]);
});
