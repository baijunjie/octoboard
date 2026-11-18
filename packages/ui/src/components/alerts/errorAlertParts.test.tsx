// @vitest-environment jsdom
import { expect, it } from "vitest";

import { translatorFor } from "../../i18n/catalog";
import { CodedError } from "../../errors/codedError";
import { errorAlertParts } from "./errorAlertParts";
import { errorNotification } from "../../store";

const t = translatorFor("en");

it("titles an alert with the call site's title where the code has none of its own", () => {
  expect(errorAlertParts(t, errorNotification(new Error("boom"), { title: "Could not pull the branch" })).title).toBe("Could not pull the branch");
  expect(errorAlertParts(t, errorNotification(new Error("boom"), { title: undefined })).title).toBeUndefined();
});

it("prefers the code's own alert title over the call site's", () => {
  const error = new CodedError("Trust failed.", "trust_answer_failed", {});
  expect(errorAlertParts(t, errorNotification(error, { title: "Could not do it" })).title).toBe("Could not answer the trust confirmation");
});

it("keeps the call site's title over the code's own where it asks to", () => {
  const error = new CodedError("Trust failed.", "trust_answer_failed", {});
  expect(errorAlertParts(t, errorNotification(error, { title: "Could not do it", keepTitle: true })).title).toBe("Could not do it");
});

it("keeps the remedy in the body for a daemon error that words its own", () => {
  expect(errorAlertParts(t, errorNotification(new CodedError("Archived.", "session_archived", { session: "s-1" }), { title: undefined })).remedyInBody).toBe(true);
});

it("leaves the remedy to the alert where the request brings its own content", () => {
  const request = { ...errorNotification(new CodedError("Archived.", "session_archived", { session: "s-1" }), { title: undefined }), content: <p>Own words</p> };
  expect(errorAlertParts(t, request).remedyInBody).toBe(false);
});

it("keeps the remedy in the body for a failed launch whose cause words its own", () => {
  const error = new CodedError("Launch failed.", "launch_failed", {
    title: "Draft the release notes", reason_code: "config_dir_unreachable", agent: "Claude Code", path: "/Users/dev/.claude-work", reason: "Permission denied",
  });
  expect(errorAlertParts(t, errorNotification(error, { title: undefined })).remedyInBody).toBe(true);
});
