// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AGENT_INSTALL_URL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import type { Agent, AgentAvailability, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { AddAgentDialog } from "./AddAgentDialog";
import { DEFAULT_WORKTREE_SETTINGS } from "../worktreeSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function render(
  availability: Partial<Record<Agent, AgentAvailability["availability"]>>,
  added: Agent[] | null,
  request: (body: RequestBody) => Promise<unknown> = () => Promise.resolve({ type: "ack" }),
  onClose = () => {},
) {
  const daemon: Daemon = {
    store: createStateStore({
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", worktrees: DEFAULT_WORKTREE_SETTINGS, accounts: [] },
      agentAvailability: new Map(Object.entries(availability).map(([agent, state]) => [agent as Agent, { agent: agent as Agent, availability: state }])),
    }),
    request: vi.fn(request) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><AddAgentDialog onClose={onClose} /></DaemonProvider>));
  return daemon;
}

const rows = () => [...document.querySelectorAll("li")];
const button = (name: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.getAttribute("aria-label") === name || b.textContent === name)!;

it("lists every agent with whether it is installed or added", () => {
  render({ claude: "available", codex: "unavailable", grok: "not_determined" }, ["claude"]);
  expect(rows().map((row) => row.textContent)).toEqual(["Claude Code, Added", "Codex, Not installedHow to install", "Grok Build, Checking…Add"]);
});

it("offers Add only for an installed agent that is not added yet", () => {
  render({ claude: "available", codex: "available", grok: "unavailable" }, ["claude"]);
  expect(button("Add Codex").disabled).toBe(false);
  expect(document.querySelector('button[aria-label="Add Claude Code"]')).toBeNull();
  expect(document.querySelector('button[aria-label="Add Grok Build"]')).toBeNull();
});

it("cannot add an agent whose installation is not determined yet, or any while the list is undecided", () => {
  render({ claude: "available", codex: "not_determined" }, null);
  expect(button("Add Claude Code").disabled).toBe(true);
  expect(button("Add Codex").disabled).toBe(true);
});

it("links each agent that is not installed to its vendor's install instructions", () => {
  render({ claude: "unavailable", codex: "unavailable", grok: "unavailable" }, []);
  const links = [...document.querySelectorAll<HTMLAnchorElement>("a")];
  expect(links.map((link) => link.getAttribute("aria-label"))).toEqual(["How to install Claude Code", "How to install Codex", "How to install Grok Build"]);
  expect(links.map((link) => link.getAttribute("href"))).toEqual([AGENT_INSTALL_URL.claude, AGENT_INSTALL_URL.codex, AGENT_INSTALL_URL.grok].map((url) => new URL(url).href));
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

it("presents a refusal and keeps the dialog open", async () => {
  const reported = vi.spyOn(errorAlerts, "report");
  onTestFinished(() => reported.mockRestore());
  const onClose = vi.fn();
  render({ codex: "available" }, [], () => Promise.reject(new DaemonRequestError("gone", "agent_not_installed", { agent: "Codex" })), onClose);
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
