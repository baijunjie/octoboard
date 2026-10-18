import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Account, Agent, AgentAvailability } from "../protocol";
import { defaultOpenSessionRequest } from "./openSessionRequest";

const parent = consoleOf("c-1", "Main", { default_agent: "codex", claude_account_id: "w" });
const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/w" }];
const project = projectOf("p-1", parent.id, "Search API");
const available = (...unavailable: Agent[]): Map<Agent, AgentAvailability> =>
  new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: unavailable.includes(agent) ? "unavailable" : "available" }]));

it("opens untouched on the parent's agent and account, with no name, model, effort or owner", () => {
  expect(defaultOpenSessionRequest(parent, project, { kind: "choose" }, accounts, available())).toEqual({
    type: "open_session",
    console_id: "c-1",
    project_id: "p-1",
    bound_to: undefined,
    agent: "codex",
    account: null,
    title: undefined,
  });
});

it("opens a project session on the console's project session agent and a console session on its console session agent", () => {
  const split = consoleOf("c-1", "Main", { default_agent: "codex", project_session_agent: "claude", claude_account_id: "w" });
  expect(defaultOpenSessionRequest(split, project, { kind: "choose" }, accounts, available())).toMatchObject({ agent: "claude", account: "w" });
  expect(defaultOpenSessionRequest(split, undefined, { kind: "unbound" }, accounts, available())).toMatchObject({ agent: "codex", account: null });
});

it("moves to the first selectable agent when the default is missing, with that agent's account", () => {
  const request = defaultOpenSessionRequest(parent, undefined, { kind: "unbound" }, accounts, available("codex"));
  expect(request).toMatchObject({ agent: "claude", account: "w" });
  expect(request).not.toHaveProperty("project_id");
});

it("binds to the console session only when the binding fixes one", () => {
  const owner = sessionOf("s-hub", parent.id, undefined, "Hub", "idle");
  expect(defaultOpenSessionRequest(parent, project, { kind: "bound", to: owner }, accounts, available())).toMatchObject({ bound_to: "s-hub" });
  expect(defaultOpenSessionRequest(parent, undefined, { kind: "bound", to: owner }, accounts, available())).not.toHaveProperty("bound_to");
});
