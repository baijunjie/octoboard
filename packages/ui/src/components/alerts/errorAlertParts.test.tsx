// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "../../errors/codedError";
import { errorAlertParts } from "./errorAlertParts";
import { errorNotification } from "../../store";

it("titles an alert with the call site's title where the code has none of its own", () => {
  expect(errorAlertParts(errorNotification(new Error("boom"), { title: "Could not pull the branch" })).title).toBe("Could not pull the branch");
  expect(errorAlertParts(errorNotification(new Error("boom"), { title: undefined })).title).toBeUndefined();
});

it("prefers the code's own alert title over the call site's", () => {
  const error = new CodedError("Trust failed.", "trust_answer_failed", {});
  expect(errorAlertParts(errorNotification(error, { title: "Could not do it" })).title).toBe("Could not answer the trust confirmation");
});

it("keeps the call site's title over the code's own where it asks to", () => {
  const error = new CodedError("Trust failed.", "trust_answer_failed", {});
  expect(errorAlertParts(errorNotification(error, { title: "Could not do it", keepTitle: true })).title).toBe("Could not do it");
});

it("keeps the remedy in the body for a daemon error that words its own", () => {
  expect(errorAlertParts(errorNotification(new CodedError("Archived.", "session_archived", { session: "s-1" }), { title: undefined })).remedyInBody).toBe(true);
});

it("leaves the remedy to the alert where the request brings its own content", () => {
  const request = { ...errorNotification(new CodedError("Archived.", "session_archived", { session: "s-1" }), { title: undefined }), content: <p>Own words</p> };
  expect(errorAlertParts(request).remedyInBody).toBe(false);
});

it("keeps the remedy in the body for a failed launch whose cause words its own", () => {
  const error = new CodedError("Launch failed.", "launch_failed", {
    title: "Draft the release notes", reason_code: "config_dir_unreachable", agent: "Claude Code", path: "/Users/dev/.claude-work", reason: "Permission denied",
  });
  expect(errorAlertParts(errorNotification(error, { title: undefined })).remedyInBody).toBe(true);
});
