import { expect, it } from "vitest";

import type { Event } from "../protocol";
import { sessionRequestOf, withRequest } from "./sessionRequests";

const event: Extract<Event, { type: "session_request" }> = {
  type: "session_request", request_id: "r", session: "requester", action: "handoff", kind: "project", console: "c", project: "p", agent: "receiving", target: "original", owner: null, reopened_owners: [], account: null, conversation: null, branches: ["feat/a"], model: "model", effort: "high", compatibility: { first: false, second: true }, requested_at: 2,
};
it("carries the handoff event's compatibility, including false, into the form request", () => {
  expect(sessionRequestOf(event)).toEqual({ requestId: "r", session: "requester", action: "handoff", kind: "project", console: "c", project: "p", agent: "receiving", target: "original", account: null, branches: ["feat/a"], model: "model", effort: "high", compatibility: { first: false, second: true }, requestedAt: 2 });
});
it.each(["handoff", "switch", "resume", "reopen"] as const)("does not offer a %s without its target", (action) => {
  expect(sessionRequestOf({ ...event, action, target: null })).toBeUndefined();
});
it("requires a record's conversation and rejects unknown actions", () => {
  expect(sessionRequestOf({ ...event, action: "resume_record", conversation: null })).toBeUndefined();
  expect(sessionRequestOf({ ...event, action: "future" as never })).toBeUndefined();
});
it.each(["resume", "reopen", "resume_record", "switch", "start"] as const)("retains the fields needed to approve %s", (action) => {
  const request = sessionRequestOf({ ...event, action, conversation: "conversation", owner: "owner", reopened_owners: ["ancestor"] });
  expect(request).toMatchObject({ action, requestId: "r", branches: ["feat/a"] });
  if (action === "resume" || action === "reopen") expect(request).toMatchObject({ target: "original", owner: "owner", reopenedOwners: ["ancestor"] });
  if (action === "resume_record") expect(request).toMatchObject({ conversation: "conversation", account: null });
  if (action === "switch") expect(request).toMatchObject({ target: "original", account: null });
});
it("orders requests by when they were made without modifying the original list", () => {
  const newer = sessionRequestOf(event)!; const older = { ...newer, requestId: "older", requestedAt: 1 };
  const initial = [newer]; expect(withRequest(initial, older)).toEqual([older, newer]); expect(initial).toEqual([newer]);
});
