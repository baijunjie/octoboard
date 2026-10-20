import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, AccountQuota, Agent, AgentAvailability, Console, Project } from "../protocol";
import { format, type Translate } from "../i18n/catalog";
import type { NewSessionBinding } from "./dialogRequest";
import { defaultOpenSessionRequest, openSessionRequest } from "./openSessionRequest";

const quick = (...[translate, parentConsole, project, binding, accounts, agentAvailability, added, accountQuotas = [], now = 0]: [
  Translate, Console, Project | undefined, NewSessionBinding, Account[], Map<Agent, AgentAvailability>, Agent[] | null, AccountQuota[]?, number?,
]) => defaultOpenSessionRequest(translate, parentConsole, project, binding, { settings: { accounts, added_agents: added }, agentAvailability, accountQuotas }, now);

/** The request a quick create sends, with no quota readings. */
const opened = (...args: Parameters<typeof quick>) => {
  const result = quick(...args);
  if (!("request" in result)) throw new Error("nothing to open");
  return result.request;
};

const t: Translate = (key, ...args) => format("en", key, args[0]);

const parent = consoleOf("c-1", "Main", { default_agent: "codex", claude_account_id: "w" });
const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/w" }];
const project = projectOf("p-1", parent.id, "Search API");
const available = (...unavailable: Agent[]): Map<Agent, AgentAvailability> =>
  new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: unavailable.includes(agent) ? "unavailable" : "available" }]));

it("opens untouched on the parent's agent and account, with the localized default name and no model, effort or owner", () => {
  expect(opened(t, parent, project, { kind: "choose" }, accounts, available(), null)).toEqual({
    type: "open_session",
    console_id: "c-1",
    project_id: "p-1",
    bound_to: undefined,
    agent: "codex",
    account: null,
    title: "Search API session",
  });
});

it("opens a project session on the console's project session agent and a console session on its console session agent", () => {
  const split = consoleOf("c-1", "Main", { default_agent: "codex", project_session_agent: "claude", claude_account_id: "w" });
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
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "  Custom  " }).title).toBe("Custom");
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "   " }).title).toBe("Search API 会话");
});

const usedUp = (account: string | null): AccountQuota => ({
  agent: "claude", account, fetched_at: 0, plan: null, unavailable: null,
  windows: [{ kind: "weekly", label: null, used_percent: 100, resets_at: null, window_minutes: null }],
});

it("opens on another account of the agent when the inherited one is used up, and on nothing when all are", () => {
  const onClaude = consoleOf("c-1", "Main", { default_agent: "claude", claude_account_id: "w" });
  expect(quick(t, onClaude, undefined, { kind: "unbound" }, accounts, available(), null, [usedUp("w")], 1)).toMatchObject({ request: { agent: "claude", account: null } });
  expect(quick(t, onClaude, undefined, { kind: "unbound" }, accounts, available(), null, [usedUp("w"), usedUp(null)], 1)).toEqual({ usedUp: true });
});
