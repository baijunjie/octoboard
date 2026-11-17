// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { sessionOf } from "../fixtures/builders";
import type { RequestHandler } from "../requests";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useRowRequests } from "./useRowRequests";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(acknowledgeAllAlerts);

const free = sessionOf("free", "c", "p", "Free", "idle");
const owner = sessionOf("owner", "c", "p", "Owner", "idle");

function mountRequests(request: RequestHandler) {
  let requests!: ReturnType<typeof useRowRequests>;
  function Probe() {
    requests = useRowRequests();
    return null;
  }
  mountHost(<Probe />, { daemon: testDaemon({ request }) });
  return () => requests;
}

it("asks the daemon to bind the session to the owner, with their ids, and raises nothing when it does", async () => {
  const request = vi.fn<RequestHandler>(async () => ({ type: "ack", id: null }));
  const requests = mountRequests(request);
  await act(async () => requests().stableBindSession(free, owner));
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "bind_session", session: "free", owner: "owner" });
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("raises an alert titled for the failed bind, about the session, when the daemon refuses", async () => {
  const requests = mountRequests(async () => {
    throw new CodedError("The session already has an owner.", "session_already_bound", { session: "free" });
  });
  await act(async () => requests().stableBindSession(free, owner));
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not bind the session", session: "free" })]);
});

it("raises the alert for a request that fails without a code, too", async () => {
  const requests = mountRequests(() => Promise.reject(new Error("no daemon")));
  await act(async () => requests().stableBindSession(free, owner));
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not bind the session", session: "free" })]);
});
