import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, Agent, AgentAvailability } from "../protocol";
import { format, type Translate } from "../i18n/catalog";
import { defaultOpenSessionRequest, openSessionRequest } from "./openSessionRequest";

const t: Translate = (key, ...args) => format("en", key, args[0]);

const parent = consoleOf("c-1", "Main", { default_agent: "codex", claude_account_id: "w" });
const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/w" }];
const project = projectOf("p-1", parent.id, "Search API");
const available = (...unavailable: Agent[]): Map<Agent, AgentAvailability> =>
  new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: unavailable.includes(agent) ? "unavailable" : "available" }]));

it("opens untouched on the parent's agent and account, with the localized default name and no model, effort or owner", () => {
  expect(defaultOpenSessionRequest(t, parent, project, { kind: "choose" }, accounts, available(), null)).toEqual({
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
  expect(defaultOpenSessionRequest(t, split, project, { kind: "choose" }, accounts, available(), null)).toMatchObject({ agent: "claude", account: "w" });
  expect(defaultOpenSessionRequest(t, split, undefined, { kind: "unbound" }, accounts, available(), null)).toMatchObject({ agent: "codex", account: null });
});

it("moves to the first selectable agent when the default is missing, with that agent's account", () => {
  const request = defaultOpenSessionRequest(t, parent, undefined, { kind: "unbound" }, accounts, available("codex"), null);
  expect(request).toMatchObject({ agent: "claude", account: "w" });
  expect(request).not.toHaveProperty("project_id");
});

it("binds to the console session only when the binding fixes one", () => {
  const owner = sessionOf("s-hub", parent.id, undefined, "Hub", "idle");
  expect(defaultOpenSessionRequest(t, parent, project, { kind: "bound", to: owner }, accounts, available(), null)).toMatchObject({ bound_to: "s-hub" });
  expect(defaultOpenSessionRequest(t, parent, undefined, { kind: "bound", to: owner }, accounts, available(), null)).not.toHaveProperty("bound_to");
});

it("localizes default project and console names and preserves custom names", () => {
  const zh: Translate = (key, ...args) => format("zh-Hans", key, args[0]);
  expect(defaultOpenSessionRequest(zh, parent, project, { kind: "unbound" }, accounts, available(), null).title).toBe("Search API 会话");
  expect(defaultOpenSessionRequest(zh, parent, undefined, { kind: "unbound" }, accounts, available(), null).title).toBe("Main 会话");
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "  Custom  " }).title).toBe("Custom");
  expect(openSessionRequest(zh, { console: parent, project, choice: { agent: "codex", account: null }, title: "   " }).title).toBe("Search API 会话");
});
