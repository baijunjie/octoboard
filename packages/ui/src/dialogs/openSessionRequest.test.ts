import { expect, it } from "vitest";

import { accountOf, consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, AccountQuota, Agent, AgentAvailability, Console, Project } from "../protocol";
import { format, type Translate } from "../i18n/catalog";
import type { NewSessionBinding } from "./dialogRequest";
import { defaultOpenSessionRequest, openSessionRequest, quickCreateUnavailable } from "./openSessionRequest";
import { DEFAULT_SETTINGS } from "../defaultSettings";


const quick = (...[translate, parentConsole, project, binding, accounts, agentAvailability, added, accountQuotas = [], now = 0]: [
  Translate, Console, Project | undefined, NewSessionBinding, Account[], Map<Agent, AgentAvailability>, Agent[] | null, AccountQuota[]?, number?,
]) => defaultOpenSessionRequest(translate, parentConsole, project, binding, { settings: { ...DEFAULT_SETTINGS, accounts, added_agents: added }, agentAvailability, accountQuotas }, now, "light");

/** The request a quick create sends, with no quota readings. */
const opened = (...args: Parameters<typeof quick>) => {
  const result = quick(...args);
  if (!("request" in result)) throw new Error("nothing to open");
  return result.request;
};

const t: Translate = (key, ...args) => format("en", key, args[0]);

const parent = consoleOf("c-1", "Main", { default_agent: "codex", accounts: { claude: "w" } });
const accounts: Account[] = [accountOf("w", "claude", "Work", "/w")];
const project = projectOf("p-1", parent.id, "Search API");
const available = (...unavailable: Agent[]): Map<Agent, AgentAvailability> =>
  new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: unavailable.includes(agent) ? "unavailable" : "available", default_account_dir: null }]));

it("opens untouched on the parent's agent and account, with the localized default name and no model, effort or owner", () => {
  expect(opened(t, parent, project, { kind: "choose" }, accounts, available(), null)).toEqual({
    type: "open_session",
    console_id: "c-1",
    project_id: "p-1",
    bound_to: undefined,
    agent: "codex",
    account: null,
    title: "Search API session",
    terminal_appearance: "light",
  });
});

it("opens a project session on the console's project session agent and a console session on its console session agent", () => {
  const split = consoleOf("c-1", "Main", { default_agent: "codex", project_session_agent: "claude", accounts: { claude: "w" } });
  expect(opened(t, split, project, { kind: "choose" }, accounts, available(), null)).toMatchObject({ agent: "claude", account: "w" });
  expect(opened(t, split, undefined, { kind: "unbound" }, accounts, available(), null)).toMatchObject({ agent: "codex", account: null });
});

it("moves to the first selectable agent when the default is missing, with that agent's account", () => {
  const request = opened(t, parent, undefined, { kind: "unbound" }, accounts, available("codex"), null);
  expect(request).toMatchObject({ agent: "claude", account: "w" });
  expect(request).not.toHaveProperty("project_id");
});

it("binds to the console session only when the binding fixes one", () => {
  const owner = sessionOf("s-hub", parent.id, undefined, "Hub", "idle");
  expect(opened(t, parent, project, { kind: "bound", to: owner }, accounts, available(), null)).toMatchObject({ bound_to: "s-hub" });
  expect(opened(t, parent, undefined, { kind: "bound", to: owner }, accounts, available(), null)).not.toHaveProperty("bound_to");
});

it("localizes default project and console names and preserves custom names", () => {
  const zh: Translate = (key, ...args) => format("zh-Hans", key, args[0]);
  expect(opened(zh, parent, project, { kind: "unbound" }, accounts, available(), null).title).toBe("Search API 会话");
  expect(opened(zh, parent, undefined, { kind: "unbound" }, accounts, available(), null).title).toBe("Main 会话");
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "  Custom  ", appearance: "dark" }).title).toBe("Custom");
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "   ", appearance: "dark" }).title).toBe("Search API 会话");
});

const usedUp = (account: string | null): AccountQuota => ({
  agent: "claude", account, fetched_at: 0, plan: null, unavailable: null,
  windows: [{ kind: "weekly", label: null, used_percent: 100, resets_at: null, window_minutes: null }], email: null,
});

it("opens on the account whose weekly quota resets soonest when automatic selection is on, and on the inherited account otherwise", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const stored: Account[] = [
    accountOf("w", "claude", "Work", "/w"),
    accountOf("w2", "claude", "Work 2", "/w2"),
  ];
  const now = 1_000_000;
  const weekly = (accountId: string, used: number, resetsAt: number): AccountQuota => ({
    agent: "claude", account: accountId, fetched_at: now, plan: null, unavailable: null,
    windows: [{ kind: "weekly", label: null, used_percent: used, resets_at: resetsAt, window_minutes: 7 * 24 * 60 }], email: null,
  });
  const open = (autoSelect: boolean | undefined) => defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, {
    settings: { ...DEFAULT_SETTINGS, accounts: stored, added_agents: null, ...(autoSelect !== undefined && { auto_select_account: autoSelect }) },
    agentAvailability: available(),
    accountQuotas: [weekly("w", 10, now + 3 * 24 * 3_600_000), weekly("w2", 99, now + 3_600_000)],
  }, now, "light");
  expect(open(true)).toMatchObject({ request: { agent: "claude", account: "w2" } });
  expect(open(false)).toMatchObject({ request: { agent: "claude", account: "w" } });
  expect(open(undefined)).toMatchObject({ request: { agent: "claude", account: "w" } });
});

it("opens on another account of the agent when the inherited one is used up, and on nothing when all are", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  expect(quick(t, onClaude, undefined, { kind: "unbound" }, accounts, available(), null, [usedUp("w")], 1)).toMatchObject({ request: { agent: "claude", account: null } });
  expect(quick(t, onClaude, undefined, { kind: "unbound" }, accounts, available(), null, [usedUp("w"), usedUp(null)], 1)).toEqual({ unavailable: true });
});

it("opens on another account of the agent when the inherited one is changing its sharing, and refuses when none is left", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const changing = new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 0, total: 2 } }]]);
  const state = (accountQuotas: AccountQuota[]) => ({ settings: { ...DEFAULT_SETTINGS, accounts, added_agents: null }, agentAvailability: available(), accountQuotas, accountSharing: changing });
  expect(defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, state([]), 1, "light")).toMatchObject({ request: { agent: "claude", account: null } });
  expect(defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, state([usedUp(null)]), 1, "light")).toEqual({ unavailable: true });
});

it("opens on another account of the agent when the inherited one is disabled, and refuses when none is left", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const state = (stored: Account[], disabledDefaults: Agent[] = []) => ({ settings: { ...DEFAULT_SETTINGS, accounts: stored, added_agents: null, disabled_default_accounts: disabledDefaults, default_account_names: { claude: "Personal" } }, agentAvailability: available(), accountQuotas: [] });
  const off = [{ ...accounts[0]!, disabled: true }];
  const open = (stored: Account[], disabledDefaults?: Agent[]) => defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, state(stored, disabledDefaults), 1, "light");
  expect(open(off)).toMatchObject({ request: { agent: "claude", account: null } });
  expect(open(off, ["claude"])).toEqual({ unavailable: true });
  // Every account is off with the default account inherited.
  const onDefault = consoleOf("c-2", "Main", { default_agent: "claude" });
  expect(defaultOpenSessionRequest(t, onDefault, undefined, { kind: "unbound" }, state(off, ["claude"]), 1, "light")).toEqual({ unavailable: true });
});

it("opens on another account of the agent when the inherited one is not logged in, and refuses when none is left", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const signedOut = (account: string | null, code = "quota_not_signed_in"): AccountQuota => ({
    agent: "claude", account, fetched_at: 0, plan: null, windows: [], unavailable: { code, params: {}, message: "x" }, email: null,
  });
  const open = (accountQuotas: AccountQuota[]) => defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, { settings: { ...DEFAULT_SETTINGS, accounts, added_agents: null }, agentAvailability: available(), accountQuotas }, 1, "light");
  expect(open([signedOut("w")])).toMatchObject({ request: { agent: "claude", account: null } });
  expect(open([signedOut("w"), signedOut(null, "quota_config_dir_missing")])).toEqual({ unavailable: true });
  // A reading that failed for a passing reason keeps the inherited account.
  expect(open([signedOut("w", "quota_command_failed")])).toMatchObject({ request: { agent: "claude", account: "w" } });
  const target = { kind: "new-console-session" as const, console: onClaude };
  const unavailable = (accountQuotas: AccountQuota[]) => quickCreateUnavailable(target, { settings: { ...DEFAULT_SETTINGS, accounts, added_agents: null }, agentAvailability: available(), accountQuotas }, 1);
  expect(unavailable([signedOut("w"), signedOut(null)])).toBe(true);
  expect(unavailable([signedOut("w")])).toBe(false);
});

it("counts an agent whose every account is used up or turned off as having nothing for a quick create, a mix of the two included", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude" });
  const target = { kind: "new-console-session" as const, console: onClaude };
  const state = (stored: Account[], disabledDefaults: Agent[], accountQuotas: AccountQuota[]) => ({ settings: { ...DEFAULT_SETTINGS, accounts: stored, added_agents: null, disabled_default_accounts: disabledDefaults, default_account_names: undefined }, agentAvailability: available(), accountQuotas });
  const off = { ...accounts[0]!, disabled: true };
  expect(quickCreateUnavailable(target, state(accounts, [], []), 1)).toBe(false);
  expect(quickCreateUnavailable(target, state([off], ["claude"], []), 1)).toBe(true);
  // The default account is used up and the stored one is off.
  expect(quickCreateUnavailable(target, state([off], [], [usedUp(null)]), 1)).toBe(true);
  // One free account is enough.
  expect(quickCreateUnavailable(target, state([off], [], []), 1)).toBe(false);
});

it("automatically tries other agents in added order only after the preferred agent has no usable account", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const state = {
    settings: { ...DEFAULT_SETTINGS, accounts, added_agents: ["claude", "grok", "codex"] as Agent[], auto_select_account: true },
    agentAvailability: available(), accountQuotas: [usedUp(null), usedUp("w")],
  };
  const target = { kind: "new-console-session" as const, console: onClaude };
  const open = () => defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, state, 1, "light");
  expect(open()).toMatchObject({ request: { agent: "grok", account: null } });
  expect(quickCreateUnavailable(target, state, 1)).toBe(false);
  state.agentAvailability = available("grok");
  expect(open()).toMatchObject({ request: { agent: "codex", account: null } });
  state.settings.added_agents = ["claude"];
  expect(open()).toEqual({ unavailable: true });
  expect(quickCreateUnavailable(target, state, 1)).toBe(true);
  state.settings.added_agents = ["claude", "codex"];
  state.settings.auto_select_account = false;
  expect(open()).toEqual({ unavailable: true });
});

it("skips disabled, signed-out and changing accounts across agents but keeps an unread usable account", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", accounts: { claude: "w" } });
  const state = {
    settings: { ...DEFAULT_SETTINGS, accounts, added_agents: ["claude", "codex", "grok"] as Agent[], auto_select_account: true, disabled_default_accounts: ["claude"] as Agent[] },
    agentAvailability: available(),
    accountQuotas: [{ agent: "codex" as const, account: null, fetched_at: 0, plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: {}, message: "signed out" }, email: null }],
    accountSharing: new Map([["w", { account: "w", progress: { share: true, phase: "changing" as const, done: 0, total: 2 } }]]),
  };
  expect(defaultOpenSessionRequest(t, onClaude, undefined, { kind: "unbound" }, state, 1, "light")).toMatchObject({ request: { agent: "grok", account: null } });
  state.settings.disabled_default_accounts.push("grok");
  expect(quickCreateUnavailable({ kind: "new-console-session", console: onClaude }, state, 1)).toBe(true);
});
