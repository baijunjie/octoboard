// @vitest-environment jsdom
import { onTestFinished, expect, it } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { sessionOf } from "../fixtures/builders";
import { LanguageProvider } from "../i18n/react";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { mountHost } from "../test/mountHost";
import { reportSwitchFailures } from "./accountSwitchFailureAlert";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const refused = (params: Record<string, string>) => new CodedError("Work is not logged in.", "account_not_signed_in", { agent: "Test Agent", account: "Work", ...params });

/** Reports a batch whose sessions were all refused with `errors`, and shows the alert's content. */
function shown(...errors: CodedError[]): string {
  onTestFinished(acknowledgeAllAlerts);
  reportSwitchFailures(errors.map((error, index) => ({ session: sessionOf(`s-${index}`, "console", undefined, `Session ${index}`, "idle"), error })), "Work");
  const content = errorAlerts.getSnapshot()[0]!.content;
  return mountHost(<LanguageProvider>{content}</LanguageProvider>).textContent!;
}

it("says how to log in with the provider's own command, once, when a batch is refused for a signed-out account of a provider with no login flow", () => {
  const text = shown(refused({ agent_id: "test_agent", config_dir: "/accounts/work" }), refused({ agent_id: "test_agent", config_dir: "/accounts/work" }));
  expect(text.match(/To log in, run test-agent login in a terminal, with \/accounts\/work as its configuration directory\./g)).toHaveLength(1);
  expect(text).not.toContain("Choose another account, or log in to this one.");
});

it("keeps the advice to log in for a provider with a login flow, and for an error that names no provider", () => {
  expect(shown(refused({ agent_id: "claude", config_dir: "/accounts/work" }))).toContain("Choose another account, or log in to this one.");
  errorAlerts.acknowledge();
  const older = shown(refused({}));
  expect(older).toContain("Choose another account, or log in to this one.");
  expect(older).not.toContain("To log in, run");
});
