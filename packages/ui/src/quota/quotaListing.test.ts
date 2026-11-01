import { expect, it } from "vitest";

import type { Account, Agent } from "../protocol";
import { listedQuotaAccounts } from "./quotaListing";

function account(id: string, agent: Agent, disabled = false): Account {
  return { id, agent, name: id, config_dir: `/${id}`, share_default_config: false, share_default_config_incomplete: false, disabled } as Account;
}

const ids = (listed: ReturnType<typeof listedQuotaAccounts>) => listed.map(({ agent, rows }) => [agent, rows.map(({ account }) => account?.id ?? null)]);

it("lists each agent's accounts in the saved order, the default account among them", () => {
  const accounts = [account("a", "claude"), account("b", "claude"), account("c", "codex")];
  expect(ids(listedQuotaAccounts(["claude", "codex"], { accounts, accountOrder: { claude: ["b", null, "a"] } }))).toEqual([
    ["claude", ["b", null, "a"]],
    ["codex", [null, "c"]],
  ]);
});

it("leaves out an owned account and a default account the user turned off", () => {
  const accounts = [account("a", "claude", true), account("b", "claude")];
  const listed = listedQuotaAccounts(["claude", "codex"], { accounts, disabledDefaultAccounts: ["claude"] });
  expect(ids(listed)).toEqual([["claude", ["b"]], ["codex", [null]]]);
  expect(listed[0].rows[0].disabled).toBe(false);
});

it("keeps a turned-off account that the selected running session is on, marked disabled", () => {
  const accounts = [account("a", "claude", true)];
  const source = { accounts, disabledDefaultAccounts: ["claude"] as Agent[] };
  const owned = listedQuotaAccounts(["claude"], source, { agent: "claude", account: "a" });
  expect(owned[0].rows.map(({ account, disabled }) => [account?.id, disabled])).toEqual([["a", true]]);
  const fallback = listedQuotaAccounts(["claude"], source, { agent: "claude", account: null });
  expect(fallback[0].rows.map(({ account, disabled }) => [account, disabled])).toEqual([[null, true]]);
  expect(listedQuotaAccounts(["claude"], source, { agent: "codex", account: null })).toEqual([]);
});

it("drops an agent whose accounts are all turned off", () => {
  expect(listedQuotaAccounts(["claude"], { accounts: [account("a", "claude", true)], disabledDefaultAccounts: ["claude"] })).toEqual([]);
});
