// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { projectOf, sessionOf } from "../fixtures/builders";
import type { DialogRequest } from "../dialogs/dialogRequest";
import type { RequestHandler } from "../requests";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useRowRequests } from "./useRowRequests";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(acknowledgeAllAlerts);

const free = sessionOf("free", "c", "p", "Free", "idle");
const owner = sessionOf("owner", "c", "p", "Owner", "idle");

function mountRequests(request: RequestHandler, openDialog: (dialog: DialogRequest) => void = () => {}, showNotice: (message: string) => void = () => {}) {
  let requests!: ReturnType<typeof useRowRequests>;
  function Probe() {
    requests = useRowRequests(openDialog);
    return null;
  }
  mountHost(<Probe />, { daemon: testDaemon({ request, showNotice }) });
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

const website = projectOf("p", "c", "Website");

it.each([
  [{ kind: "up_to_date" }, "The current branch is already up to date."],
  [{ kind: "fast_forwarded" }, "Pulled the current branch."],
  [{ kind: "merged" }, "Pulled the current branch."],
] as const)("says what a manual pull that ended %j did, and opens no dialog", async (outcome, notice) => {
  const request = vi.fn<RequestHandler>(async () => ({ type: "project_branch_pulled", id: null, outcome }));
  const openDialog = vi.fn();
  const showNotice = vi.fn();
  const requests = mountRequests(request, openDialog, showNotice);
  await act(async () => requests().stablePullProjectBranch(website));
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "pull_project_branch", project: "p" });
  expect(showNotice).toHaveBeenCalledExactlyOnceWith(notice);
  expect(openDialog).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("asks to confirm a merge for a pull that finds the branch diverged, with no notice", async () => {
  const outcome = { kind: "needs_merge", ahead: 2, behind: 3 } as const;
  const openDialog = vi.fn();
  const showNotice = vi.fn();
  const requests = mountRequests(async () => ({ type: "project_branch_pulled", id: null, outcome }), openDialog, showNotice);
  await act(async () => requests().stablePullProjectBranch(website));
  expect(openDialog).toHaveBeenCalledExactlyOnceWith({ kind: "pull-merge-project-branch", project: website, ahead: 2, behind: 3 });
  expect(showNotice).not.toHaveBeenCalled();
});

it("raises an alert titled for the failed pull, with its remedy, when the daemon refuses it", async () => {
  const showNotice = vi.fn();
  const requests = mountRequests(async () => {
    throw new CodedError("This branch has no upstream to pull from.", "no_upstream_to_pull", {});
  }, undefined, showNotice);
  await act(async () => requests().stablePullProjectBranch(website));
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not pull the branch", error: expect.objectContaining({ code: "no_upstream_to_pull" }), sharedRecovery: "noUpstream" })]);
  expect(showNotice).not.toHaveBeenCalled();
});
