import { expect, it } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Account, AccountQuota, Agent, AgentAvailability, QuotaWindow } from "../protocol";
import { sessionQuotaBarOf, type SessionQuotaSource } from "./sessionQuota";

const session = (extra = {}) => sessionOf("s-1", "c-1", undefined, "Fix", "idle", extra);
const window: QuotaWindow = { kind: "session", label: null, used_percent: 30, resets_at: null, window_minutes: 300 };
const reading = (account: string | null, patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent: "claude", account, fetched_at: 0, plan: null, unavailable: null, windows: [window], ...patch,
});
const work: Account = { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" };
const source = (patch: Partial<SessionQuotaSource> = {}): SessionQuotaSource => ({
  accounts: [work],
  agentAvailability: new Map<Agent, AgentAvailability>([["claude", { agent: "claude", availability: "available" }]]),
  accountQuotas: [reading(null), reading("a-work")],
  ...patch,
});

it("draws the bar of the account the session runs on", () => {
  expect(sessionQuotaBarOf(session(), source())).toEqual({ main: window, thin: undefined });
  expect(sessionQuotaBarOf(session({ account_id: "a-work", config_dir: work.config_dir }), source())).toEqual({ main: window, thin: undefined });
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
