// @vitest-environment jsdom
import { expect, it } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { daemonMessage } from "./daemonMessage";
import { classifyError } from "./errorRecovery";
import { format } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";

it.each(["en", "zh-Hans"] as const)("distinguishes automatic submit retry from legacy manual recovery in %s", (language) => {
  const agent = "Claude Code";
  const params = { agent, submit_retry: "once" };
  expect(daemonMessage(language, "message_not_sent", params, "diagnostic")).toBe(
    format(language, "daemon.message_not_sent.retry", { agent: isolate(agent) }),
  );
  expect(daemonMessage(language, "message_not_sent", { agent }, "diagnostic")).toBe(
    format(language, "daemon.message_not_sent", { agent: isolate(agent) }),
  );
  expect(classifyError(new DaemonRequestError("", "message_not_sent", params))).toEqual({
    kind: "actionable", recovery: "submitRetry",
  });
  expect(classifyError(new DaemonRequestError("", "message_not_sent", params), "none")).toEqual({
    kind: "actionable", recovery: "none",
  });
  expect(classifyError(new DaemonRequestError("", "message_not_sent", { agent }))).toEqual({
    kind: "actionable", recovery: "unsentMessage",
  });
  expect(format(language, "error.recovery.submitRetry", { enter: "Enter" })).toContain("Enter");
});
