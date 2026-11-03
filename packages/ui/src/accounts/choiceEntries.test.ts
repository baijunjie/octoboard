import { describe, expect, it } from "vitest";

import { avoidUnavailableAccount, settledInitialChoice } from "./openingChoice";
import { choiceSignedOut, choiceChangingSharing, choiceDisabled, choiceExhaustion, choiceGroups, incompleteAccount } from "./choiceEntries";
import { switchEntryBlock, switchEntries } from "./switchEntries";
import { preferExpiringWeekly } from "./accountRanking";
import { consoleOf, sessionOf } from "../fixtures/builders";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, AccountSharing } from "../protocol";
import { availability, account, accounts, settle, NOW, HOUR, DAY, windowOf, reading, usedUp, signedOut, t } from "./accountChoicesTestSupport";

describe("choiceGroups", () => {
  it("uses the saved order in pickers and switch lists, including the default account", () => {
    const order = { claude: ["w", null] };
    const groups = choiceGroups(accounts, availability({}), null, () => "Default", undefined, undefined, undefined, order);
    expect(groups.find((group) => group.agent === "claude")!.entries.map((entry) => entry.account)).toEqual(["w", null]);
    const session = sessionOf("s", "console", "project", "Session", "idle", { agent: "claude" });
    expect(switchEntries(session, accounts, ((key: string) => key) as Translate, {}, undefined, undefined, undefined, order).map((entry) => entry.account)).toEqual(["w", null]);
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
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w" } });

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
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w" } });

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
    expect(choiceGroups(stored.map((item) => ({ ...item, disabled: false })), availability({}), null, () => "Default")[0]!.entries.every((entry) => !entry.disabled)).toBe(true);
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
    expect(switchEntries(on, stored, t, {}, undefined, undefined, ["claude"]).map((entry) => [entry.account, entry.disabled])).toEqual([[null, true], ["w", true], ["v", false]]);
  });
});

describe("choiceGroups and the added agents", () => {
  it("leaves selectable only the groups of added agents", () => {
    const groups = choiceGroups(accounts, availability({}), ["codex"], () => "Default");
    expect(groups.map((g) => [g.agent, g.selectable])).toEqual([["claude", false], ["codex", true], ["grok", false]]);
  });
});

describe("an account whose last reading found it not logged in", () => {
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w" } });
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
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle"), stored, t, {}, readings([signedOut("claude", "w")]));
    expect(entries.map((entry) => [entry.current, entry.signedOut])).toEqual([[true, false], [false, true], [false, false]]);
    expect(switchEntryBlock(t, entries[1], { switching: false, ownChanging: false })?.reason).toEqual({ state: "Not logged in" });
    expect(switchEntryBlock(t, entries[2], { switching: false, ownChanging: false })).toBeUndefined();
  });

  it("says why over a switch holding the session, which ends on its own and says nothing", () => {
    const entries = switchEntries(sessionOf("s", "c-1", "p-1", "Title", "idle"), stored, t, {}, readings([signedOut("claude", "w")]));
    expect(switchEntryBlock(t, entries[1], { switching: true, ownChanging: false })?.reason).toEqual({ state: "Not logged in" });
    expect(switchEntryBlock(t, entries[2], { switching: true, ownChanging: false })).toEqual({ detail: "Unavailable while this session is switching its account" });
  });
});

describe("an account whose quota is used up", () => {
  const owner = consoleOf("c-1", "Main", { accounts: { claude: "w" } });
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
