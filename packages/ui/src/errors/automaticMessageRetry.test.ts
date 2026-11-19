// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "./codedError";
import { daemonMessage } from "./daemonMessage";
import { classifyError } from "./errorRecovery";
import { format } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";

const records = { consoles: new Map(), projects: new Map(), sessions: new Map() };

it.each(["en", "zh-CN"] as const)("distinguishes automatic submit retry from legacy manual recovery in %s", (language) => {
  const agent = "Claude Code";
  const params = { agent, submit_retry: "once" };
  expect(daemonMessage(language, "message_not_sent", params, "diagnostic", records)).toBe(
    format(language, "daemon.message_not_sent.retry", { agent: isolate(agent) }),
  );
  expect(daemonMessage(language, "message_not_sent", { agent }, "diagnostic", records)).toBe(
    format(language, "daemon.message_not_sent", { agent: isolate(agent) }),
  );
  expect(classifyError(new CodedError("", "message_not_sent", params))).toEqual({
    kind: "actionable", recovery: "submitRetry",
  });
  expect(classifyError(new CodedError("", "message_not_sent", params), "none")).toEqual({
    kind: "actionable", recovery: "none",
  });
  expect(classifyError(new CodedError("", "message_not_sent", { agent }))).toEqual({
    kind: "actionable", recovery: "unsentMessage",
  });
  expect(format(language, "error.recovery.submitRetry", { enter: "Enter" })).toContain("Enter");
});

it("says the retry presses the submit key only where the input box holds nothing but the message, and differs from the wording with no retry in every language", () => {
  const params = { agent: "Claude Code", appName: "Octoboard" };
  expect(format("en", "daemon.message_not_sent.retry", params)).toContain("only if the input box holds nothing but the message");
  for (const language of ["en", "de", "ja", "zh-CN", "ar"] as const) {
    expect(format(language, "daemon.message_not_sent.retry", params), language).not.toBe(format(language, "daemon.message_not_sent", params));
  }
});
