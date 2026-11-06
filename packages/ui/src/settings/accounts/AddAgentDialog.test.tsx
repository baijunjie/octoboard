// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import type { Agent, AgentAvailability, ClientRequestBody } from "../../protocol";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { availabilityOf } from "../../fixtures/fixtureProviders";
import { TEST_PROVIDERS } from "../../test/testProviders";
import { AddAgentDialog } from "./AddAgentDialog";
import { DEFAULT_GIT_WORKFLOW } from "../../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function render(
  availability: Partial<Record<Agent, AgentAvailability["availability"]>>,
  added: Agent[] | null,
  request: (body: ClientRequestBody) => Promise<unknown> = () => Promise.resolve({ type: "ack", id: null }),
  onClose = () => {},
) {
  const handler = vi.fn(request);
  const daemon = testDaemon({
    state: {
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
      agentAvailability: availabilityOf(TEST_PROVIDERS, availability),
    },
    request: handler as never,
    showNotice: vi.fn(),
  });
  mountHost(<AddAgentDialog onClose={onClose} />, { daemon });
  return { store: daemon.store, request: handler };
}

const rows = () => [...document.querySelectorAll("li")];
const button = (name: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "") === name || b.textContent === name)!;

it("lists every agent with whether it is installed or added", () => {
  render({ claude: "available", codex: "unavailable", grok: "not_determined" }, ["claude"]);
  expect(rows().map((row) => row.textContent)).toEqual(["Claude Code, Added", "Codex, Not installedHow to install", "Grok Build, Checking…Add", "Test Agent, Checking…Add"]);
});

it("offers Add only for an installed agent that is not added yet", () => {
  render({ claude: "available", codex: "available", grok: "unavailable" }, ["claude"]);
  expect(button("Add Codex").disabled).toBe(false);
  expect(button("Add Claude Code")).toBeUndefined();
  expect(button("Add Grok Build")).toBeUndefined();
});

it("cannot add an agent whose installation is not determined yet, or any while the list is undecided", () => {
  render({ claude: "available", codex: "not_determined" }, null);
  expect(button("Add Claude Code").disabled).toBe(true);
  expect(button("Add Codex").disabled).toBe(true);
});

it("links each agent that is not installed to its provider's install instructions", () => {
  render({ claude: "unavailable", codex: "unavailable", grok: "unavailable", test_agent: "unavailable" }, []);
  const links = [...document.querySelectorAll<HTMLAnchorElement>("a")];
  expect(links.map((link) => link.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, ""))).toEqual(["How to install Claude Code", "How to install Codex", "How to install Grok Build", "How to install Test Agent"]);
  expect(links.map((link) => link.getAttribute("href"))).toEqual(TEST_PROVIDERS.map((provider) => new URL(provider.install_url).href));
});

it("sends add_agent, and shows the agent as added once the list follows", async () => {
  const daemon = render({ codex: "available" }, []);
  await act(async () => button("Add Codex").click());
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "add_agent", agent: "codex" });
  act(() => daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: ["codex"] } }));
  expect(rows()[1]!.textContent).toBe("Codex, Added");
});

it("moves focus to Close when the button that was pressed goes with the agent being added", async () => {
  const daemon = render({ codex: "available" }, []);
  const add = button("Add Codex");
  add.focus();
  await act(async () => add.click());
  act(() => daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: ["codex"] } }));
  expect(document.activeElement).toBe([...document.querySelectorAll("button")].find((b) => b.textContent === "Close"));
});

it.each([
  ["Close", () => [...document.querySelectorAll("button")].find((b) => b.textContent === "Close")!],
  ["the close button", () => document.querySelector<HTMLElement>("[data-slot$=close-trigger]")!],
])("hands focus to the pending Add button when it is on %s as the request starts", async (_name, control) => {
  let settle = () => {};
  render({ codex: "available" }, [], () => new Promise((resolve) => (settle = () => resolve({ type: "ack", id: null }))));
  control().focus();
  await act(async () => button("Add Codex").click());
  expect(document.activeElement).toBe(button("Add Codex"));
  await act(async () => settle());
});

it("presents a refusal and keeps the dialog open", async () => {
  const reported = vi.spyOn(errorAlerts, "report");
  onTestFinished(() => reported.mockRestore());
  const onClose = vi.fn();
  render({ codex: "available" }, [], () => Promise.reject(new CodedError("gone", "agent_not_installed", { agent: "Codex" })), onClose);
  await act(async () => button("Add Codex").click());
  await act(async () => {});
  expect(reported).toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
  expect(button("Add Codex").disabled).toBe(false);
});

it("closes from its Close button", () => {
  const onClose = vi.fn();
  render({}, [], undefined, onClose);
  act(() => [...document.querySelectorAll("button")].find((b) => b.textContent === "Close")!.click());
  expect(onClose).toHaveBeenCalled();
});
