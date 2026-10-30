import { describe, expect, it } from "vitest";

import { agentOpeningChoice, avoidUnavailableAccount, choiceSignedOut, choiceChangingSharing, switchEntryBlock, choiceDisabled, choiceExhaustion, choiceGroups, choiceKey, consoleAccount, currentChoice, expiringWeekly, handoffBlock, incompleteAccount, inheritedAccount, preferExpiringWeekly, projectAccount, settledInitialChoice, switchEntries, type QuotaReadings } from "./accountChoices";
import { consoleOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Account, AccountQuota, AccountSharing, Agent, AgentAvailability, Console, Project, QuotaWindow } from "./protocol";

const availability = (entries: Partial<Record<Agent, AgentAvailability["availability"]>>): Map<Agent, AgentAvailability> =>
  new Map(
    (["claude", "codex", "grok"] as Agent[]).map((agent) => [
      agent,
      { agent, availability: entries[agent] ?? "not_determined" },
    ]),
  );

const account = (id: string, agent: Agent, name: string): Account => ({ id, agent, name, config_dir: `/home/${id}` });
const accounts = [account("w", "claude", "Work"), account("c", "codex", "Codex work")];

describe("choiceGroups", () => {
  it("uses the saved order in pickers and switch lists, including the default account", () => {
    const order = { claude: ["w", null] };
    const groups = choiceGroups(accounts, availability({}), null, () => "Default", undefined, undefined, undefined, order);
    expect(groups.find((group) => group.agent === "claude")!.entries.map((entry) => entry.account)).toEqual(["w", null]);
    const session = sessionOf("s", "console", "project", "Session", "idle", { agent: "claude" });
    expect(switchEntries(session, accounts, ((key: string) => key) as Translate, undefined, undefined, undefined, undefined, order).map((entry) => entry.account)).toEqual(["w", null]);
    expect(avoidUnavailableAccount({ agent: "claude", account: null }, groups)).toEqual({ agent: "claude", account: null });
  });

  it("puts the default account first in every agent's group, the stored accounts after it", () => {
    const groups = choiceGroups(accounts, availability({}), null, () => "Default");
    expect(groups.map((g) => [g.agent, g.entries.map((e) => e.name)])).toEqual([
      ["claude", ["Default", "Work"]],
      ["codex", ["Default", "Codex work"]],
      ["grok", ["Default"]],
    ]);
  });

  it("holds the default account alone for every agent on a first-ever start", () => {
    for (const group of choiceGroups([], availability({}), null, () => "Default")) {
      expect(group.entries.map((e) => e.account)).toEqual([null]);
    }
  });

  it.each([
    ["not yet determined", "not_determined", true],
    ["available", "available", true],
    ["unavailable", "unavailable", false],
  ] as const)("leaves a group selectable unless its agent is known to be missing (%s)", (_, state, selectable) => {
    expect(choiceGroups(accounts, availability({ claude: state }), null, () => "Default")[0].selectable).toBe(selectable);
  });
});

describe("choiceGroups and a running sharing change", () => {
  const sharing = new Map<string, AccountSharing>([
    ["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 1, total: 3, item: "skills" } }],
    ["c", { account: "c", in_use: { sessions: 2, signing_in: false } }],
  ]);

  it("marks only the entry whose change runs, never the default account or one that is merely in use", () => {
    const groups = choiceGroups(accounts, availability({}), null, () => "Default", undefined, sharing);
    expect(groups.flatMap((group) => group.entries.map((entry) => [entry.name, entry.changingSharing]))).toEqual([
      ["Default", false], ["Work", true], ["Default", false], ["Codex work", false], ["Default", false],
    ]);
  });

  it("marks nothing without the sharing state", () => {
    expect(choiceGroups(accounts, availability({}), null, () => "Default").every((group) => group.entries.every((entry) => !entry.changingSharing))).toBe(true);
  });
});

describe("an account whose sharing change runs", () => {
  const sharing = new Map<string, AccountSharing>([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 0, total: 2 } }]]);
  const groups = choiceGroups(accounts, availability({}), null, () => "Default", undefined, sharing);
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });

  it("is avoided like a used-up one: the choice moves to the first other account of the agent", () => {
    expect(avoidUnavailableAccount({ agent: "claude", account: "w" }, groups)).toEqual({ agent: "claude", account: null });
    expect(avoidUnavailableAccount({ agent: "claude", account: null }, groups)).toEqual({ agent: "claude", account: null });
  });

  it("stays the choice, flagged, when it is the agent's only account", () => {
    const only = choiceGroups([accounts[0]!], availability({}), null, () => "Default", undefined, new Map([["w", sharing.get("w")!], ["", { account: "", progress: sharing.get("w")!.progress }]]));
    expect(choiceChangingSharing({ agent: "claude", account: "w" }, only)).toBe(true);
    expect(choiceChangingSharing({ agent: "claude", account: null }, only)).toBe(false);
  });

  it("opens the session choice on another account, and is never preferred by the weekly quota", () => {
    const settled = settledInitialChoice(owner, undefined, accounts, availability({}), null, groups);
    expect(settled.choice).toEqual({ agent: "claude", account: null });
    expect(settled.changingSharing).toBe(false);
    const entries = groups[0]!.entries;
    expect(preferExpiringWeekly(entries, 0)).toBeUndefined();
  });
});

describe("a disabled account", () => {
  const stored = [{ ...account("w", "claude", "Work"), disabled: true }, account("v", "claude", "Other")];
  const groups = choiceGroups(stored, availability({}), null, () => "Default", undefined, undefined, ["codex"]);
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });

  it("is marked on its entry, the default account of an agent by that agent", () => {
    expect(groups.map((group) => [group.agent, group.entries.map((entry) => entry.disabled)])).toEqual([
      ["claude", [false, true, false]],
      ["codex", [true]],
      ["grok", [false]],
    ]);
    expect(choiceDisabled({ agent: "claude", account: "w" }, groups)).toBe(true);
    expect(choiceDisabled({ agent: "claude", account: "v" }, groups)).toBe(false);
    expect(choiceDisabled({ agent: "codex", account: null }, groups)).toBe(true);
  });

  it("marks nothing without the disabled state", () => {
    expect(choiceGroups(stored.map((item) => ({ ...item, disabled: undefined })), availability({}), null, () => "Default")[0]!.entries.every((entry) => !entry.disabled)).toBe(true);
  });

  it("is avoided like a used-up one: the choice moves to the first other account of the agent", () => {
    expect(avoidUnavailableAccount({ agent: "claude", account: "w" }, groups)).toEqual({ agent: "claude", account: null });
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, groups).choice).toEqual({ agent: "claude", account: null });
  });

  it("stays the choice, flagged, when every account of the agent is disabled", () => {
    expect(avoidUnavailableAccount({ agent: "codex", account: null }, groups)).toEqual({ agent: "codex", account: null });
  });

  it("is never preferred by the weekly quota, even when it resets soonest", () => {
    const quotas = [reading("w", [windowOf({ used_percent: 10, resets_at: NOW + HOUR })]), reading("v", [windowOf({ used_percent: 10, resets_at: NOW + DAY })])];
    const ranked = choiceGroups(stored, availability({}), null, () => "Default", { quotas, now: NOW })[0]!.entries;
    expect(preferExpiringWeekly(ranked, NOW)?.account).toBe("v");
    expect(preferExpiringWeekly(ranked.map((entry) => ({ ...entry, disabled: entry.account === "v" ? true : entry.disabled })), NOW)).toBeUndefined();
  });

  it("is flagged on the Switch account entries, a disabled default account too, each other account still counted", () => {
    const on = sessionOf("s", "c-1", "p-1", "Title", "idle", { agent: "claude", account_id: "v", config_dir: "/home/v" });
    expect(switchEntries(on, stored, t, undefined, undefined, undefined, ["claude"]).map((entry) => [entry.account, entry.disabled])).toEqual([[null, true], ["w", true], ["v", false]]);
  });
});

describe("choiceGroups and the added agents", () => {
  it("leaves selectable only the groups of added agents", () => {
    const groups = choiceGroups(accounts, availability({}), ["codex"], () => "Default");
    expect(groups.map((g) => [g.agent, g.selectable])).toEqual([["claude", false], ["codex", true], ["grok", false]]);
  });
});

/** The state `settledInitialChoice` is given for `accounts`, `states` and `added` (quotas judged by `readings`). */
const settle = (owner: Console, project: Project | undefined, accounts: Account[], agents: ReturnType<typeof availability>, added: Agent[] | null, readings?: QuotaReadings) =>
  settledInitialChoice(owner, project, accounts, agents, added, choiceGroups(accounts, agents, added, () => "", readings));

/** The agent and account the dialog opens on. */
const initialChoice = (...args: Parameters<typeof settle>) => settle(...args).choice;

describe("settledInitialChoice", () => {
  const owner = consoleOf("c-1", "Main", { codex_account_id: "c" });
  const project = (agent?: Agent) => projectOf("p-1", "c-1", "App", { default_agent: agent });

  it.each([
    ["the console's default agent, on the default account when it pins nothing", undefined, {}, { agent: "claude", account: null }],
    ["the project's agent over the console's, on the account the console holds for it", "codex", {}, { agent: "codex", account: "c" }],
    ["the first agent that may be picked when the resolved one is known to be missing", "claude", { claude: "unavailable" }, { agent: "codex", account: "c" }],
    ["the resolved agent while availability is not determined", "claude", {}, { agent: "claude", account: null }],
    ["the resolved agent when none may be picked", "claude", { claude: "unavailable", codex: "unavailable", grok: "unavailable" }, { agent: "claude", account: null }],
  ] as const)("opens on %s", (_, projectAgent, states, expected) => {
    expect(initialChoice(owner, project(projectAgent), accounts, availability(states), null)).toEqual(expected);
  });

  it("opens on the first added agent that may be picked when the resolved one is not added", () => {
    expect(initialChoice(owner, project(), accounts, availability({}), ["codex", "grok"])).toEqual({ agent: "codex", account: "c" });
  });

  it("opens a project session on the console's project session agent when the project names none", () => {
    const split = consoleOf("c-1", "Main", { project_session_agent: "codex", codex_account_id: "c" });
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
    const moved = projectOf("p-1", "c-1", "App", { codex_account_id: { kind: "account", id: "c2" } });
    const stored = [...accounts, account("c2", "codex", "Codex 2")];
    expect(initialChoice(owner, moved, stored, availability({ claude: "unavailable" }), null)).toEqual({ agent: "codex", account: "c2" });
  });
});

const NOW = 1_000_000;
const usedUp = (agent: Agent, account: string | null): AccountQuota => ({
  agent, account, fetched_at: NOW, plan: null, unavailable: null,
  windows: [{ kind: "session", label: null, used_percent: 100, resets_at: NOW + 60_000, window_minutes: 300 }],
});

const signedOut = (agent: Agent, account: string | null, code = "quota_not_signed_in"): AccountQuota => ({
  agent, account, fetched_at: NOW, plan: null, windows: [], unavailable: { code, params: { agent }, message: "x" },
});

describe("an account whose last reading found it not logged in", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });
  const stored = [...accounts, account("w2", "claude", "Work 2")];
  const readings = (quotas: AccountQuota[]) => ({ quotas, now: NOW });

  it.each(["quota_not_signed_in", "quota_config_dir_missing"])("marks its entry, and no other, for %s", (code) => {
    const groups = choiceGroups(stored, availability({}), null, () => "Default", readings([signedOut("claude", "w", code)]));
    const claude = groups.find((group) => group.agent === "claude")!;
    expect(claude.entries.map((entry) => entry.signedOut)).toEqual([false, true, false]);
    expect(choiceSignedOut({ agent: "claude", account: "w" }, groups)).toBe(true);
  });

  it("does not mark an account whose reading failed for a passing reason", () => {
    const groups = choiceGroups(stored, availability({}), null, () => "Default", readings([signedOut("claude", "w", "quota_command_failed")]));
    expect(groups.flatMap((group) => group.entries).some((entry) => entry.signedOut)).toBe(false);
  });

  it("moves the opening choice to another account of the agent, and keeps it when none is left", () => {
    const open = (quotas: AccountQuota[]) => settle(owner, undefined, stored, availability({}), null, readings(quotas));
    expect(open([signedOut("claude", "w")]).choice).toEqual({ agent: "claude", account: null });
    const all = open([signedOut("claude", "w"), signedOut("claude", null), signedOut("claude", "w2")]);
    expect(all.choice).toEqual({ agent: "claude", account: "w" });
    expect(all.signedOut).toBe(true);
  });

  it("is never the account automatic selection prefers", () => {
    const withWeekly = (quota: AccountQuota): AccountQuota => ({ ...quota, windows: [{ kind: "weekly", label: null, used_percent: 10, resets_at: NOW + 1000, window_minutes: 10080 }] });
    const groups = choiceGroups(stored, availability({}), null, () => "Default", readings([withWeekly(signedOut("claude", "w")), usedUp("claude", null)]));
    expect(preferExpiringWeekly(groups.find((group) => group.agent === "claude")!.entries, NOW)).toBeUndefined();
  });

  it("is listed in the Switch account menu with why, and not offered", () => {
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle"), stored, t, undefined, readings([signedOut("claude", "w")]));
    expect(entries.map((entry) => [entry.current, entry.signedOut])).toEqual([[true, false], [false, true], [false, false]]);
    expect(switchEntryBlock(t, entries[1], { switching: false, ownChanging: false })?.reason).toEqual({ state: "Not logged in" });
    expect(switchEntryBlock(t, entries[2], { switching: false, ownChanging: false })).toBeUndefined();
  });

  it("says why over a switch holding the session, which ends on its own and says nothing", () => {
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle"), stored, t, undefined, readings([signedOut("claude", "w")]));
    expect(switchEntryBlock(t, entries[1], { switching: true, ownChanging: false })?.reason).toEqual({ state: "Not logged in" });
    expect(switchEntryBlock(t, entries[2], { switching: true, ownChanging: false })).toEqual({ detail: "Unavailable while this session is switching its account" });
  });
});

describe("an account whose quota is used up", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });
  const stored = [...accounts, account("w2", "claude", "Work 2")];
  const open = (quotas: AccountQuota[]) => settle(owner, undefined, stored, availability({}), null, { quotas, now: NOW });

  it("marks its entry with the window that used it up", () => {
    const groups = choiceGroups(stored, availability({}), null, () => "Default", { quotas: [usedUp("claude", "w")], now: NOW });
    const claude = groups.find((group) => group.agent === "claude")!;
    expect(claude.entries.map((entry) => entry.exhaustedBy?.kind)).toEqual([undefined, "session", undefined]);
    expect(choiceExhaustion({ agent: "claude", account: "w" }, groups)?.kind).toBe("session");
  });

  it("moves the opening choice to the first other account of the agent, the default account first", () => {
    expect(open([usedUp("claude", "w")])).toEqual({ choice: { agent: "claude", account: null }, exhaustedBy: undefined, changingSharing: false, signedOut: false, disabled: false });
    expect(open([usedUp("claude", "w"), usedUp("claude", null)]).choice).toEqual({ agent: "claude", account: "w2" });
  });

  it("keeps the choice, and says what used it up, when every account of the agent is used up", () => {
    const settled = open([usedUp("claude", "w"), usedUp("claude", null), usedUp("claude", "w2")]);
    expect(settled.choice).toEqual({ agent: "claude", account: "w" });
    expect(settled.exhaustedBy?.kind).toBe("session");
  });

  it("never moves to another agent", () => {
    const groups = choiceGroups(stored, availability({}), null, () => "Default", { quotas: [usedUp("claude", "w"), usedUp("claude", null), usedUp("claude", "w2")], now: NOW });
    expect(avoidUnavailableAccount({ agent: "claude", account: "w" }, groups).agent).toBe("claude");
  });

  it("does not move a choice whose agent may not be picked", () => {
    const groups = choiceGroups(stored, availability({ claude: "unavailable" }), null, () => "Default", { quotas: [usedUp("claude", "w")], now: NOW });
    expect(avoidUnavailableAccount({ agent: "claude", account: "w" }, groups)).toEqual({ agent: "claude", account: "w" });
  });

  it("is not avoided once the window has reset", () => {
    expect(open([usedUp("claude", "w")]).choice.account).toBeNull();
    expect(settle(owner, undefined, stored, availability({}), null, { quotas: [usedUp("claude", "w")], now: NOW + 120_000 }).choice.account).toBe("w");
  });
});

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const windowOf = (patch: Partial<QuotaWindow> = {}): QuotaWindow => ({
  kind: "weekly", label: null, used_percent: 0, resets_at: NOW + DAY, window_minutes: 7 * 24 * 60, ...patch,
});
const reading = (accountId: string | null, windows: QuotaWindow[], agent: Agent = "claude"): AccountQuota => ({
  agent, account: accountId, fetched_at: NOW, plan: null, unavailable: null, windows,
});

describe("preferExpiringWeekly", () => {
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const entries = (quotas: AccountQuota[]) =>
    choiceGroups(stored, availability({}), null, () => "Default", { quotas, now: NOW }).find((group) => group.agent === "claude")!.entries;

  it("prefers the soonest reset over a larger remainder", () => {
    const ranked = preferExpiringWeekly(entries([
      reading("w", [windowOf({ used_percent: 10, resets_at: NOW + 3 * DAY })]),
      reading("soon", [windowOf({ used_percent: 99, resets_at: NOW + HOUR })]),
    ]), NOW);
    expect(ranked?.account).toBe("soon");
  });

  it("prefers more remaining when the reset is the same", () => {
    const ranked = preferExpiringWeekly(entries([
      reading("w", [windowOf({ used_percent: 80, resets_at: NOW + DAY })]),
      reading("soon", [windowOf({ used_percent: 10, resets_at: NOW + DAY })]),
    ]), NOW);
    expect(ranked?.account).toBe("soon");
    expect(expiringWeekly(reading("w", [
      windowOf({ used_percent: 80, resets_at: NOW + DAY }),
      windowOf({ used_percent: 10, resets_at: NOW + DAY }),
    ]), NOW)).toEqual({ resetsAt: NOW + DAY, remaining: 90 });
  });

  it("keeps the earlier entry when the reset and the remainder are equal, the default account first", () => {
    const ranked = preferExpiringWeekly(entries([
      reading(null, [windowOf({ used_percent: 40, resets_at: NOW + DAY })]),
      reading("w", [windowOf({ used_percent: 40, resets_at: NOW + DAY })]),
    ]), NOW);
    expect(ranked?.account).toBeNull();
  });

  it.each([
    ["a model-specific weekly window", windowOf({ kind: "weekly_model", label: "Fable" })],
    ["a monthly credit period", windowOf({ kind: "credit_period", window_minutes: 30 * 24 * 60 })],
    ["a session window", windowOf({ kind: "session", window_minutes: 300 })],
    ["a window that is used up", windowOf({ used_percent: 100 })],
    ["a window whose reset is not still ahead", windowOf({ resets_at: NOW })],
    ["a window whose reset is in the past", windowOf({ resets_at: NOW - 1 })],
    ["a window with no reset", windowOf({ resets_at: null })],
    ["a window whose used percent is not a number", windowOf({ used_percent: Number.NaN })],
  ] as const)("ignores %s", (_, window) => {
    expect(preferExpiringWeekly(entries([reading("w", [window])]), NOW)).toBeUndefined();
    expect(expiringWeekly(reading("w", [window]), NOW)).toBeUndefined();
  });

  it("ranks a credit period of 6 to 8 days and ignores one a minute outside that", () => {
    const week = (minutes: number) => reading("w", [windowOf({ kind: "credit_period", window_minutes: minutes, used_percent: 25, resets_at: NOW + HOUR })]);
    expect(expiringWeekly(week(6 * 24 * 60), NOW)).toEqual({ resetsAt: NOW + HOUR, remaining: 75 });
    expect(expiringWeekly(week(8 * 24 * 60), NOW)).toEqual({ resetsAt: NOW + HOUR, remaining: 75 });
    expect(expiringWeekly(week(6 * 24 * 60 - 1), NOW)).toBeUndefined();
    expect(expiringWeekly(week(8 * 24 * 60 + 1), NOW)).toBeUndefined();
    expect(expiringWeekly(week(28 * 24 * 60), NOW)).toBeUndefined();
  });

  it("takes the soonest weekly window of one account, ignoring a session window that resets sooner", () => {
    expect(expiringWeekly(reading("w", [
      windowOf({ kind: "session", window_minutes: 300, used_percent: 1, resets_at: NOW + HOUR }),
      windowOf({ used_percent: 40, resets_at: NOW + 2 * DAY }),
      windowOf({ used_percent: 80, resets_at: NOW + DAY }),
    ]), NOW)).toEqual({ resetsAt: NOW + DAY, remaining: 20 });
  });

  it("skips an entry a shorter window has used up, even when its weekly quota resets sooner", () => {
    const spent = reading("soon", [
      windowOf({ kind: "session", used_percent: 100, resets_at: NOW + HOUR, window_minutes: 300 }),
      windowOf({ used_percent: 1, resets_at: NOW + HOUR }),
    ]);
    const later = reading("w", [windowOf({ used_percent: 50, resets_at: NOW + 3 * DAY })]);
    expect(preferExpiringWeekly(entries([spent, later]), NOW)?.account).toBe("w");
  });

  it("returns nothing when no window ranks", () => {
    expect(preferExpiringWeekly(entries([]), NOW)).toBeUndefined();
    expect(expiringWeekly(undefined, NOW)).toBeUndefined();
  });
});

describe("preferExpiringWeekly and the session window", () => {
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const session = (patch: Partial<QuotaWindow> = {}) => windowOf({ kind: "session", window_minutes: 300, used_percent: 10, resets_at: NOW + 3 * HOUR, ...patch });
  const pick = (quotas: AccountQuota[]) =>
    preferExpiringWeekly(choiceGroups(stored, availability({}), null, () => "Default", { quotas, now: NOW }).find((group) => group.agent === "claude")!.entries, NOW)?.account;

  it("prefers a running session window when the weekly resets are within a session window of each other", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 26 * HOUR }), session()]),
    ])).toBe("soon");
  });

  it("leaves a weekly reset that is far later out of the tie", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 4 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 6 * DAY }), session()]),
    ])).toBe("w");
  });

  it("ties a weekly reset less than a session window after the soonest, but not exactly one", () => {
    const soonest = reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]);
    expect(pick([soonest, reading("soon", [windowOf({ resets_at: NOW + 29 * HOUR - 1 }), session()])])).toBe("soon");
    expect(pick([soonest, reading("soon", [windowOf({ resets_at: NOW + 29 * HOUR }), session()])])).toBe("w");
  });

  it("measures the tie against the soonest reset rather than chaining", () => {
    const third = [...stored, account("far", "claude", "Far")];
    const entries = choiceGroups(third, availability({}), null, () => "Default", {
      quotas: [
        reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
        reading("soon", [windowOf({ resets_at: NOW + 27 * HOUR })]),
        reading("far", [windowOf({ resets_at: NOW + 31 * HOUR }), session()]),
      ],
      now: NOW,
    }).find((group) => group.agent === "claude")!.entries;
    expect(preferExpiringWeekly(entries, NOW)?.account).toBe("w");
  });

  it("ranks the sooner session reset first, then the sooner weekly reset", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR }), session({ resets_at: NOW + 3 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 25 * HOUR }), session({ resets_at: NOW + 2 * HOUR })]),
    ])).toBe("soon");
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 25 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 24 * HOUR })]),
    ])).toBe("soon");
  });

  it.each([
    ["used up", session({ used_percent: 100 })],
    ["already reset", session({ resets_at: NOW })],
    ["without a reset", session({ resets_at: null })],
  ])("does not count a session window that is %s as running", (_, window) => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 25 * HOUR }), window]),
    ])).toBe("w");
  });
});

describe("settledInitialChoice when no account can be used", () => {
  const owner = consoleOf("c-1", "Main", { default_agent: "claude", claude_account_id: "w" });
  const stored = [account("w", "claude", "Work"), account("b", "claude", "B")];
  const settleAuto = (quotas: AccountQuota[], accountList = stored) =>
    settledInitialChoice(owner, undefined, accountList, availability({}), ["claude"], choiceGroups(accountList, availability({}), ["claude"], () => "Default", { quotas, now: NOW }), { now: NOW });
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
    const groups = choiceGroups(stored, availability({}), ["claude"], () => "Default", { quotas, now: NOW });
    expect(settledInitialChoice(owner, undefined, stored, availability({}), ["claude"], groups).choice.account).toBe("w");
  });

  it("prefers a usable account of another agent over the account that becomes usable soonest", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + DAY)];
    const both = [...stored, account("c", "codex", "Codex work")];
    const groups = choiceGroups(both, availability({}), null, () => "Default", { quotas, now: NOW });
    expect(settledInitialChoice(owner, undefined, both, availability({}), null, groups, { now: NOW }).choice.agent).toBe("codex");
  });

  it("opens a re-picked agent on the account that becomes usable soonest, as the dialog's first choice does", () => {
    const quotas = [used(null, "weekly", NOW + 6 * DAY), used("w", "weekly", NOW + 5 * DAY), used("b", "weekly", NOW + DAY)];
    const groups = choiceGroups(stored, availability({}), ["claude"], () => "Default", { quotas, now: NOW });
    expect(agentOpeningChoice(owner, undefined, "claude", stored, groups, { now: NOW })).toEqual({ agent: "claude", account: "b" });
    expect(agentOpeningChoice(owner, undefined, "claude", stored, groups)).toEqual({ agent: "claude", account: "w" });
  });

  it("keeps the inherited account when none is kept off by a used-up quota alone", () => {
    expect(settleAuto([signedOut("claude", null), signedOut("claude", "w"), signedOut("claude", "b")]).choice.account).toBe("w");
  });
});

describe("settledInitialChoice with automatic account selection", () => {
  const owner = consoleOf("c-1", "Main", { default_agent: "claude", claude_account_id: "w" });
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const quotas = [
    reading("w", [windowOf({ used_percent: 10, resets_at: NOW + 3 * DAY })]),
    reading("soon", [windowOf({ used_percent: 99, resets_at: NOW + HOUR })]),
  ];
  const groups = (listed: AccountQuota[] = quotas) => choiceGroups(stored, availability({}), null, () => "", { quotas: listed, now: NOW });

  it("breaks automatic selection ties in the saved account order, as the daemon does", () => {
    const readings = [reading(null, [windowOf()]), reading("w", [windowOf()]), reading("soon", [windowOf()])];
    const reordered = choiceGroups(stored, availability({}), null, () => "", { quotas: readings, now: NOW }, undefined, undefined, { claude: ["soon", "w", null] });
    expect(reordered.find((group) => group.agent === "claude")!.entries.map((entry) => entry.account)).toEqual(["soon", "w", null]);
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, reordered, { now: NOW }).choice).toEqual({ agent: "claude", account: "soon" });
    const defaultFirst = choiceGroups(stored, availability({}), null, () => "", { quotas: readings, now: NOW }, undefined, undefined, { claude: [null, "w", "soon"] });
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, defaultFirst, { now: NOW }).choice).toEqual({ agent: "claude", account: null });
  });

  it("keeps the inherited account when automatic selection is not asked for", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, groups()).choice).toEqual({ agent: "claude", account: "w" });
  });

  it("keeps the inherited account when nothing ranks", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, groups([]), { now: NOW }).choice).toEqual({ agent: "claude", account: "w" });
  });

  it("opens on the preferred account even when the inherited one is not used up", () => {
    expect(settledInitialChoice(owner, undefined, stored, availability({}), null, groups(), { now: NOW }).choice).toEqual({ agent: "claude", account: "soon" });
  });
});

describe("inheritedAccount", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w" });
  const stored = [...accounts, account("w2", "claude", "Work 2")];

  it("is the default account when neither the project nor the console pins one", () => {
    expect(inheritedAccount(consoleOf("c-1", "Main"), projectOf("p-1", "c-1", "App"), "grok", accounts)).toBeNull();
  });

  it("is the project's account over the console's", () => {
    const project = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "w2" } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w2");
  });

  it("is the console's account when the project's reference is no longer stored", () => {
    const project = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "gone" } });
    expect(inheritedAccount(owner, project, "claude", stored)).toBe("w");
  });
});

describe("a project on the agent's default account", () => {
  const owner = consoleOf("c-1", "Main", { claude_account_id: "w", codex_account_id: "c" });
  const onDefault = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "default" } });

  it("resolves to the default account without falling through to the console's", () => {
    expect(inheritedAccount(owner, onDefault, "claude", accounts)).toBeNull();
  });

  it("is said for its own agent only", () => {
    expect(inheritedAccount(owner, onDefault, "codex", accounts)).toBe("c");
  });

  it("opens the session dialog on the default account while the console pins one", () => {
    const project = projectOf("p-1", "c-1", "App", { default_agent: "claude", claude_account_id: { kind: "default" } });
    expect(initialChoice(owner, project, accounts, availability({}), null)).toEqual({ agent: "claude", account: null });
  });

  it("is told from inheriting by projectAccount", () => {
    expect(projectAccount(onDefault, "claude", accounts)).toEqual({ kind: "default" });
    expect(projectAccount(projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "inherit" } }), "claude", accounts)).toEqual({ kind: "inherit" });
  });

  it("reads a pinned account that is not stored as inheriting", () => {
    const gone = projectOf("p-1", "c-1", "App", { claude_account_id: { kind: "account", id: "gone" } });
    expect(projectAccount(gone, "claude", accounts)).toEqual({ kind: "inherit" });
  });
});

describe("consoleAccount", () => {
  it("reads a reference to an account that is not stored, or is another agent's, as the default account", () => {
    const owner = consoleOf("c-1", "Main", { claude_account_id: "w", codex_account_id: "w", grok_account_id: "gone" });
    expect(consoleAccount(owner, "claude", accounts)).toBe("w");
    expect(consoleAccount(owner, "codex", accounts)).toBeNull();
    expect(consoleAccount(owner, "grok", accounts)).toBeNull();
  });
});

describe("currentChoice", () => {
  const initial = { agent: "claude", account: null } as const;
  const picked = { agent: "codex", account: "c" } as const;

  it.each([
    ["the user's pick", {}, picked],
    ["the initial entry when the pick's agent has since been found missing", { codex: "unavailable" }, initial],
  ] as const)("shows %s", (_, states, expected) => {
    expect(currentChoice(picked, initial, choiceGroups(accounts, availability(states), null, () => "Default"))).toEqual(expected);
  });

  it("shows the initial entry when the pick's account is no longer stored, or there is no pick", () => {
    const groups = choiceGroups([], availability({}), null, () => "Default");
    expect(currentChoice(picked, initial, groups)).toEqual(initial);
    expect(currentChoice(undefined, initial, groups)).toEqual(initial);
  });
});

it("gives each agent's entries keys no other agent's can share", () => {
  expect(choiceKey({ agent: "claude", account: null })).not.toBe(choiceKey({ agent: "codex", account: null }));
});

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

describe("switchEntries and a running sharing change", () => {
  it("marks the account whose change runs, and no other, a removed account's directory never", () => {
    const sharing = new Map<string, AccountSharing>([["w", { account: "w", progress: { share: false, phase: "changing" as const, done: 0, total: 2 } }], ["gone", { account: "gone", progress: { share: false, phase: "changing" as const, done: 0, total: 2 } }]]);
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle", { account_id: "gone", config_dir: "/home/old" }), accounts, t, undefined, undefined, sharing);
    expect(entries.map((entry) => [entry.name, entry.changingSharing])).toEqual([["Default", false], ["Work", true], ["/home/old", false]]);
  });
});

describe("switchEntries", () => {
  const session = (extra = {}) => sessionOf("s", "c-1", "p-1", "Title", "idle", extra);
  const summary = (entries: ReturnType<typeof switchEntries>) =>
    entries.map((entry) => `${entry.name}${entry.current ? " (current)" : ""}`);

  it.each([
    ["the default account first, then the agent's stored accounts, marking the one it is on", {}, ["Default (current)", "Work"]],
    ["a stored account as current", { account_id: "w", config_dir: "/home/w" }, ["Default", "Work (current)"]],
    ["a removed account as current, named by the directory the session recorded", { account_id: "gone", config_dir: "/home/old" }, ["Default", "Work", "/home/old (current)"]],
    ["a removed account that recorded no directory as nothing to name", { account_id: "gone" }, ["Default", "Work"]],
    ["only the session's own agent's accounts", { agent: "grok" }, ["Default (current)"]],
  ] as const)("lists %s", (_, extra, expected) => {
    expect(summary(switchEntries(session(extra), accounts, t, undefined))).toEqual(expected);
  });

  it("marks an account whose quota is used up at the time given, and not once it has reset", () => {
    const exhausted = (now: number) => switchEntries(session(), accounts, t, undefined, { quotas: [usedUp("claude", "w")], now }).map((entry) => entry.exhaustedBy?.kind);
    expect(exhausted(NOW)).toEqual([undefined, "session"]);
    expect(exhausted(NOW + 120_000)).toEqual([undefined, undefined]);
  });
});

describe("incompleteAccount", () => {
  const half = { ...account("h", "claude", "Half"), share_default_config_incomplete: true };
  const running: AccountSharing = { account: "h", progress: { share: true, phase: "changing", done: 1, total: 4 } };

  it("names an account whose last sharing change did not finish and none runs", () => {
    expect(incompleteAccount([half], undefined, "h")).toBe(half);
  });

  it.each([
    ["a complete account", [account("h", "claude", "Half")], undefined, "h"],
    ["the default account", [half], undefined, null],
    ["no account", [half], undefined, undefined],
    ["an account that is no longer stored", [half], undefined, "gone"],
    ["an account whose sharing change runs, which the daemon refuses anyway", [half], new Map([["h", running]]), "h"],
  ] as const)("names nothing for %s", (_, stored, sharing, id) => {
    expect(incompleteAccount([...stored], sharing, id)).toBeUndefined();
  });
});

describe("handoffBlock", () => {
  const t = ((key: string) => key) as Translate;
  const base = { status: "idle", locked: false, switching_account: false } as const;

  it("says a session still starting is not ready, not that it is mid-turn", () => {
    expect(handoffBlock(t, { ...base, status: "starting" }, false)).toEqual({ detail: "accountChoice.handoff.starting" });
    expect(handoffBlock(t, { ...base, status: "working" }, false)?.detail).toBe("accountChoice.handoff.working");
    expect(handoffBlock(t, base, false)).toBeUndefined();
  });
});
