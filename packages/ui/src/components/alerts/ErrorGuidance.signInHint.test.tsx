// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "../../errors/codedError";
import { LanguageProvider } from "../../i18n/react";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { ErrorGuidance, RecoveryGuidance } from "./ErrorGuidance";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const refused = (params: Record<string, string>) => new CodedError("Work is not logged in.", "account_not_signed_in", { agent: "Test Agent", account: "Work", ...params });
const mount = (ui: React.ReactElement) => mountHost(<LanguageProvider>{ui}</LanguageProvider>, { daemon: testDaemon() }).textContent;
const HINT = "To log in, run test-agent login in a terminal, with /accounts/work as its configuration directory.";
const ADVICE = "Choose another account, or log in to this one.";

it("words an account_not_signed_in recovery by the provider's own way to log in, for a provider with no login flow", () => {
  const error = refused({ agent_id: "test_agent", config_dir: "/accounts/work" });
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" error={error} />)).toBe(HINT);
  expect(mount(<ErrorGuidance error={error} />)).toBe(HINT);
});

it("keeps the advice to log in for a provider with a login flow, an error that names none, no error and another recovery", () => {
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" error={refused({ agent_id: "claude", config_dir: "/accounts/work" })} />)).toBe(ADVICE);
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" error={refused({})} />)).toBe(ADVICE);
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" />)).toBe(ADVICE);
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" error={refused({ agent_id: "not_served" })} />)).toBe(ADVICE);
  // The error of another code never asks for the hint, whatever params it carries.
  const other = new CodedError("Disabled.", "account_disabled", { agent_id: "test_agent", config_dir: "/accounts/work" });
  expect(mount(<RecoveryGuidance recovery="accountDisabled" error={other} />)).not.toContain("To log in");
});

it("falls back to the provider's usual directory where the daemon names none, as for a default account it has not found yet", () => {
  expect(mount(<RecoveryGuidance recovery="accountNotSignedIn" error={refused({ agent_id: "test_agent" })} />)).toContain("with ~/.octoboard-test-agent as its configuration directory.");
});
