import { describe, expect, it } from "vitest";

import { agentOpeningChoice, currentChoice, settledInitialChoice } from "./openingChoice";
import { choiceGroups, choiceKey } from "./choiceEntries";
import { consoleOf, projectOf } from "../fixtures/builders";
import type { AccountQuota, Agent, QuotaWindow } from "../protocol";
import { availability, account, accounts, initialChoice, NOW, HOUR, DAY, windowOf, reading, signedOut } from "./accountChoicesTestSupport";
import { TEST_PROVIDERS } from "../test/testProviders";

describe("settledInitialChoice", () => {
  const owner = consoleOf("c-1", "Main", { accounts: { codex: "c" } });
  const project = (agent?: Agent) => projectOf("p-1", "c-1", "App", { default_agent: agent });

  it.each([
    ["the console's default agent, on the default account when it pins nothing", undefined, {}, { agent: "claude", account: null }],
    ["the project's agent over the console's, on the account the console holds for it", "codex", {}, { agent: "codex", account: "c" }],
    ["the first agent that may be picked when the resolved one is known to be missing", "claude", { claude: "unavailable" }, { agent: "codex", account: "c" }],
    ["the resolved agent while availability is not determined", "claude", {}, { agent: "claude", account: null }],
    ["the resolved agent when none may be picked", "claude", { claude: "unavailable", codex: "unavailable", grok: "unavailable", test_agent: "unavailable" }, { agent: "claude", account: null }],
  ] as const)("opens on %s", (_, projectAgent, states, expected) => {
    expect(initialChoice(owner, project(projectAgent), accounts, availability(states), null)).toEqual(expected);
  });

  it("opens on the first added agent that may be picked when the resolved one is not added", () => {
    expect(initialChoice(owner, project(), accounts, availability({}), ["codex", "grok"])).toEqual({ agent: "codex", account: "c" });
  });

  it("opens a project session on the console's project session agent when the project names none", () => {
    const split = consoleOf("c-1", "Main", { project_session_agent: "codex", accounts: { codex: "c" } });
    expect(initialChoice(split, project(), accounts, availability({}), null)).toEqual({ agent: "codex", account: "c" });
    expect(initialChoice(split, project("grok"), accounts, availability({}), null)).toEqual({ agent: "grok", account: null });
  });

  it("opens a console session on the console session agent, whatever the project session agent is", () => {
    const split = consoleOf("c-1", "Main", { project_session_agent: "codex" });
    expect(initialChoice(split, undefined, accounts, availability({}), null)).toEqual({ agent: "claude", account: null });
  });

  it("opens a project session on the console session agent for a console stored without the setting", () => {
    const legacy = consoleOf("c-1", "Main", { default_agent: "codex" });
    expect(initialChoice(legacy, project(), accounts, availability({}), null)).toEqual({ agent: "codex", account: null });
  });

  it("opens on the project's account for the agent it moved to when the first is missing", () => {
    const moved = projectOf("p-1", "c-1", "App", { accounts: { codex: { kind: "account", id: "c2" } } });
    const stored = [...accounts, account("c2", "codex", "Codex 2")];
    expect(initialChoice(owner, moved, stored, availability({ claude: "unavailable" }), null)).toEqual({ agent: "codex", account: "c2" });
  });
});

describe("settledInitialChoice when no account can be used", () => {
  const owner = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const stored = [account("w", "claude", "Work"), account("b", "claude", "B")];
  const settleAuto = (quotas: AccountQuota[], accountList = stored) =>
    settledInitialChoice(owner, undefined, accountList, availability({}), ["claude"], TEST_PROVIDERS, choiceGroups(accountList, availability({}), ["claude"], TEST_PROVIDERS, () => "Default", { quotas, now: NOW }), { now: NOW });
  const used = (account: string | null, kind: QuotaWindow["kind"], resetsAt: number | null) =>
    reading(account, [windowOf({ kind, used_percent: 100, resets_at: resetsAt, window_minutes: kind === "session" ? 300 : 7 * 24 * 60 })]);

  it("opens on the account whose blocking window resets soonest", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), reading("b", [windowOf({ resets_at: NOW + 6 * DAY }), windowOf({ kind: "session", used_percent: 100, resets_at: NOW + 30 * 60_000, window_minutes: 300 })])];
    expect(settleAuto(quotas).choice).toEqual({ agent: "claude", account: "b" });
  });

  it("goes by the window that resets last when an account has several used up, and counts one without a reset as never", () => {
    const both = reading("w", [windowOf({ used_percent: 100, resets_at: NOW + 4 * DAY }), windowOf({ kind: "session", used_percent: 100, resets_at: NOW + HOUR, window_minutes: 300 })]);
    expect(settleAuto([used(null, "weekly", null), both, used("b", "weekly", NOW + 5 * DAY)]).choice.account).toBe("w");
    expect(settleAuto([used(null, "weekly", null), used("w", "weekly", null), used("b", "weekly", NOW + 5 * DAY)]).choice.account).toBe("b");
  });

  it("keeps the candidate order on equal times, the default account first", () => {
    expect(settleAuto([used(null, "weekly", NOW + DAY), used("w", "weekly", NOW + DAY), used("b", "weekly", NOW + DAY)]).choice.account).toBeNull();
  });

  it("passes over an account that is not logged in, even when it would be usable sooner", () => {
    const quotas = [signedOut("claude", null), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + 6 * DAY)];
    expect(settleAuto(quotas).choice.account).toBe("w");
  });

  it("keeps the inherited account when automatic selection is off", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + DAY)];
    const groups = choiceGroups(stored, availability({}), ["claude"], TEST_PROVIDERS, () => "Default", { quotas, now: NOW });
    expect(settledInitialChoice(owner, undefined, stored, availability({}), ["claude"], TEST_PROVIDERS, groups).choice.account).toBe("w");
  });

  it("prefers a usable account of another agent over the account that becomes usable soonest", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + DAY)];
    const both = [...stored, account("c", "codex", "Codex work")];
    const groups = choiceGroups(both, availability({}), null, TEST_PROVIDERS, () => "Default", { quotas, now: NOW });
    expect(settledInitialChoice(owner, undefined, both, availability({}), null, TEST_PROVIDERS, groups, { now: NOW }).choice.agent).toBe("codex");
  });

  it("opens a re-picked agent on the account that becomes usable soonest, as the dialog's first choice does", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + DAY)];
    const groups = choiceGroups(stored, availability({}), ["claude"], TEST_PROVIDERS, () => "Default", { quotas, now: NOW });
    expect(agentOpeningChoice(owner, undefined, "claude", stored, groups, { now: NOW })).toEqual({ agent: "claude", account: "b" });
    expect(agentOpeningChoice(owner, undefined, "claude", stored, groups)).toEqual({ agent: "claude", account: "w" });
  });

  it("keeps the inherited account when none is kept off by a used-up quota alone", () => {
    expect(settleAuto([signedOut("claude", null), signedOut("claude", "w"), signedOut("claude", "b")]).choice.account).toBe("w");
  });
});

describe("settledInitialChoice with automatic account selection", () => {
  const owner = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const quotas = [
    reading("w", [windowOf({ used_percent: 10, resets_at: NOW + 3 * DAY })]),
    reading("soon", [windowOf({ used_percent: 99, resets_at: NOW + HOUR })]),
  ];
  const groups = (listed: AccountQuota[] = quotas) => choiceGroups(stored, availability({}), null, TEST_PROVIDERS, () => "", { quotas: listed, now: NOW });

  it("breaks automatic selection ties in the saved account order, as the daemon does", () => {
    const readings = [reading(null, [windowOf()]), reading("w", [windowOf()]), reading("soon", [windowOf()])];
    const reordered = choiceGroups(stored, availability({}), null, TEST_PROVIDERS, () => "", { quotas: readings, now: NOW }, undefined, undefined, { claude: ["soon", "w", null] });
    expect(reordered.find((group) => group.agent === "claude")!.entries.map((entry) => entry.account)).toEqual(["soon", "w", null]);
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, TEST_PROVIDERS, reordered, { now: NOW }).choice).toEqual({ agent: "claude", account: "soon" });
    const defaultFirst = choiceGroups(stored, availability({}), null, TEST_PROVIDERS, () => "", { quotas: readings, now: NOW }, undefined, undefined, { claude: [null, "w", "soon"] });
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, TEST_PROVIDERS, defaultFirst, { now: NOW }).choice).toEqual({ agent: "claude", account: null });
  });

  it("keeps the inherited account when automatic selection is not asked for", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, TEST_PROVIDERS, groups()).choice).toEqual({ agent: "claude", account: "w" });
  });

  it("keeps the inherited account when nothing ranks", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, TEST_PROVIDERS, groups([]), { now: NOW }).choice).toEqual({ agent: "claude", account: "w" });
  });

  it("opens on the preferred account even when the inherited one is not used up", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, TEST_PROVIDERS, groups(), { now: NOW }).choice).toEqual({ agent: "claude", account: "soon" });
  });
});

describe("currentChoice", () => {
  const initial = { agent: "claude", account: null } as const;
  const picked = { agent: "codex", account: "c" } as const;

  it.each([
    ["the user's pick", {}, picked],
    ["the initial entry when the pick's agent has since been found missing", { codex: "unavailable" }, initial],
  ] as const)("shows %s", (_, states, expected) => {
    expect(currentChoice(picked, initial, choiceGroups(accounts, availability(states), null, TEST_PROVIDERS, () => "Default"))).toEqual(expected);
  });

  it("shows the initial entry when the pick's account is no longer stored, or there is no pick", () => {
    const groups = choiceGroups([], availability({}), null, TEST_PROVIDERS, () => "Default");
    expect(currentChoice(picked, initial, groups)).toEqual(initial);
    expect(currentChoice(undefined, initial, groups)).toEqual(initial);
  });
});

it("gives each agent's entries keys no other agent's can share", () => {
  expect(choiceKey({ agent: "claude", account: null })).not.toBe(choiceKey({ agent: "codex", account: null }));
});
