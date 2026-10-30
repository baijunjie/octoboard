import { expect, it } from "vitest";

import { accountOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, AccountQuota, Agent, AgentAvailability, QuotaWindow } from "../protocol";
import { sessionQuotaBarOf, type SessionQuotaSource } from "./sessionQuota";

const session = (extra = {}) => sessionOf("s-1", "c-1", undefined, "Fix", "idle", extra);
const window: QuotaWindow = { kind: "session", label: null, used_percent: 30, resets_at: null, window_minutes: 300 };
const reading = (account: string | null, patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent: "claude", account, fetched_at: 0, plan: null, unavailable: null, windows: [window], email: null, ...patch,
});
const work: Account = accountOf("a-work", "claude", "Work", "/accounts/work");
const source = (patch: Partial<SessionQuotaSource> = {}): SessionQuotaSource => ({
  accounts: [work],
  agentAvailability: new Map<Agent, AgentAvailability>([["claude", { agent: "claude", availability: "available", default_account_dir: null }]]),
  accountQuotas: [reading(null), reading("a-work")],
  ...patch,
});

it("draws the bar of the account the session runs on", () => {
  expect(sessionQuotaBarOf(session(), source())).toMatchObject({ main: window, thin: undefined, naming: { agent: "claude" } });
  expect(sessionQuotaBarOf(session({ account_id: "a-work", config_dir: work.config_dir }), source())).toMatchObject({ main: window, thin: undefined, naming: { agent: "claude" } });
});

it("draws nothing without a session or for an agent that is not available", () => {
  expect(sessionQuotaBarOf(undefined, source())).toBeUndefined();
  expect(sessionQuotaBarOf(session(), source({ agentAvailability: new Map() }))).toBeUndefined();
});

it("draws nothing once the session's account was removed or its directory changed", () => {
  expect(sessionQuotaBarOf(session({ account_id: "a-gone" }), source())).toBeUndefined();
  expect(sessionQuotaBarOf(session({ account_id: "a-work", config_dir: "/accounts/old" }), source())).toBeUndefined();
  expect(sessionQuotaBarOf(session({ config_dir: "/accounts/old" }), source())).toBeUndefined();
});

it("draws nothing for a reading that is out of date", () => {
  expect(sessionQuotaBarOf(session(), source({ accountQuotas: [reading(null, { unavailable: { code: "quota_failed", message: "" } as never })] }))).toBeUndefined();
});

it("draws a turned-off account's bar only while the session is running on it", () => {
  const off: Account = { ...work, disabled: true };
  const owned = { account_id: "a-work", config_dir: work.config_dir };
  expect(sessionQuotaBarOf(session({ ...owned, status: "idle" }), source({ accounts: [off] }))).toMatchObject({ main: window });
  expect(sessionQuotaBarOf(session({ ...owned, status: "working" }), source({ accounts: [off] }))).toMatchObject({ main: window });
  expect(sessionQuotaBarOf(session({ ...owned, status: "interrupted" }), source({ accounts: [off] }))).toBeUndefined();
  expect(sessionQuotaBarOf(session({ ...owned, status: "archived" }), source({ accounts: [off] }))).toBeUndefined();
  // The same session on an account that is on keeps its bar whatever its status.
  expect(sessionQuotaBarOf(session({ ...owned, status: "interrupted" }), source())).toMatchObject({ main: window });
});

it("follows the agent's turned-off default account the same way", () => {
  const disabled = source({ disabledDefaultAccounts: ["claude"] });
  expect(sessionQuotaBarOf(session({ status: "idle" }), disabled)).toMatchObject({ main: window });
  expect(sessionQuotaBarOf(session({ status: "interrupted" }), disabled)).toBeUndefined();
  expect(sessionQuotaBarOf(session({ status: "interrupted" }), source({ disabledDefaultAccounts: ["codex"] }))).toMatchObject({ main: window });
});
