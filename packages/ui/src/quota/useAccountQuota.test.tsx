// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { availabilityOf } from "../fixtures/fixtureProviders";
import type { AccountQuota, Agent } from "../protocol";
import { mountHost } from "../test/mountHost";
import { TEST_PROVIDERS } from "../test/testProviders";
import { testDaemon } from "../test/testDaemon";
import { useAccountQuota, type QuotaReading } from "./useAccountQuota";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const reading = (agent: Agent, patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent, account: null, fetched_at: 1, plan: null, windows: [], unavailable: null, email: null, ...patch,
});
const SIGNED_OUT = { code: "quota_not_signed_in", params: {}, message: "x" } as const;

/** Mounts a component that records what `useAccountQuota` returns for `agent`'s default account. */
function mountReading(agent: Agent, accountQuotas: AccountQuota[]) {
  const daemon = testDaemon({ state: { accountQuotas, agentAvailability: availabilityOf(TEST_PROVIDERS, { claude: "available", test_agent: "available" }) } });
  const seen: QuotaReading[] = [];
  function Probe() {
    seen.push(useAccountQuota(agent, null));
    return null;
  }
  mountHost(<Probe />, { daemon });
  return { seen, set: (quotas: AccountQuota[]) => act(() => daemon.store.setState({ accountQuotas: quotas })) };
}

it("waits for the first reading of an available account of a provider that has a plan quota", () => {
  const { seen } = mountReading("claude", []);
  expect(seen.at(-1)).toBe("loading");
});

it("never waits for a reading of an account of a provider that declares no plan quota", () => {
  const { seen } = mountReading("test_agent", []);
  expect(seen.length).toBeGreaterThan(0);
  expect(seen).not.toContain("loading");
  expect(seen.at(-1)).toBeUndefined();
});

it("returns the entry of a provider with no plan quota when it says signed out, or signed in with an email", () => {
  const out = reading("test_agent", { unavailable: SIGNED_OUT });
  const signedIn = reading("test_agent", { email: "dev@example.com" });
  const { seen, set } = mountReading("test_agent", [out]);
  expect(seen.at(-1)).toEqual(out);
  set([signedIn]);
  expect(seen.at(-1)).toEqual(signedIn);
  expect(seen).not.toContain("loading");
});

it("returns nothing for an entry of a provider with no plan quota that says neither, as when the sign-in could not be told", () => {
  const { seen, set } = mountReading("test_agent", [reading("test_agent")]);
  expect(seen.at(-1)).toBeUndefined();
  set([reading("test_agent", { email: "dev@example.com" })]);
  expect(seen.at(-1)).toMatchObject({ email: "dev@example.com" });
  set([reading("test_agent")]);
  expect(seen.at(-1)).toBeUndefined();
});

it("returns nothing for a failed reading of a provider with no plan quota, whose failure would otherwise be worded as usage", () => {
  const failed = reading("test_agent", { unavailable: { code: "quota_shell_environment_failed", params: {}, message: "x" } });
  expect(mountReading("test_agent", [failed]).seen.at(-1)).toBeUndefined();
});

it("returns an entry of a provider that has a plan quota whatever it says", () => {
  const empty = reading("claude");
  const { seen } = mountReading("claude", [empty]);
  expect(seen.at(-1)).toEqual(empty);
});
