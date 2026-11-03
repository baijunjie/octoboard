// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { accountOf } from "../../fixtures/builders";
import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { availabilityOf as servedAvailability } from "../../fixtures/fixtureProviders";
import type { Account, Agent, AgentAvailability, ClientRequestBody } from "../../protocol";
import { TEST_PROVIDERS } from "../../test/testProviders";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { AccountDialog } from "./AccountDialog";
import { DEFAULT_GIT_WORKFLOW } from "../../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { trackMountRefocus } from "../../components/dialog/dialogTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const work: Account = accountOf("a-work", "claude", "Work", "/home/dev/.claude-work");

function render(
  editing: Account | undefined,
  request: (body: ClientRequestBody) => Promise<unknown>,
  agentAvailability = servedAvailability(TEST_PROVIDERS),
  addedAgents: Agent[] | null = null,
  renameDefault = false,
  options: { accounts?: Account[]; listDirectory?: () => Promise<unknown> } = {},
) {
  const handler = vi.fn((body: ClientRequestBody) => body.type === "list_dir"
    ? options.listDirectory?.() ?? Promise.resolve({ type: "dir_listing", path: "/home/dev", entries: [] })
    : request(body));
  const daemon = testDaemon({
    state: { settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: addedAgents, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: options.accounts ?? [work] }, agentAvailability },
    request: handler as never,
    showNotice: vi.fn(),
  });
  mountHost(<AccountDialog account={editing} agent="claude" renameDefault={renameDefault} onClose={() => {}} />, { daemon });
  return { store: daemon.store, request: handler };
}

const input = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((el) => el.labels?.[0]?.textContent === label)!;

function type(label: string, value: string) {
  const field = input(label);
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, value);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const submit = () =>
  act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Save")!.click());

const availabilityOf = (unavailable: Agent) => new Map(
  (["claude", "codex", "grok"] as Agent[]).map((agent): [Agent, AgentAvailability] => [agent, { agent, availability: agent === unavailable ? "unavailable" : "available", default_account_dir: null }]),
);

it("lists every added agent as selectable when adding, naming the ones known to be missing", async () => {
  render(undefined, () => Promise.resolve(), availabilityOf("codex"));
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const options = Array.from(document.body.querySelectorAll('[role="option"]'));
  expect(options.map((o) => o.textContent)).toEqual(["Claude Code", "Codex, Not installed", "Grok Build", "Test Agent"]);
  expect(options.some((o) => o.hasAttribute("data-disabled") || o.getAttribute("aria-disabled") === "true")).toBe(false);
});

it("lists only the added agents, in the list's order", async () => {
  render(undefined, () => Promise.resolve(), availabilityOf("codex"), ["grok", "codex"]);
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual(["Grok Build", "Codex, Not installed"]);
});

const taken = () =>
  new CodedError("taken", "config_dir_taken", { agent: "Claude Code", name: "Work" });

it("reports a refused directory in the error alert dialog, leaving the field and its value alone", async () => {
  render(work, () => Promise.reject(taken()));
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  // The refusal comes after the submit, so it is no mark on the field.
  expect(errorAlerts.getSnapshot().map((alert) => alert.error)).toEqual([expect.objectContaining({ code: "config_dir_taken" })]);
  const field = input("Config directory");
  expect(field.getAttribute("aria-invalid")).not.toBe("true");
  expect(field.value).toBe("/home/dev/.claude-other");
  expect(document.body.textContent).not.toContain("already uses this config directory");
});

it("keeps a directory picked after a refusal and the dialog open for another try", async () => {
  const request = vi.fn(() => Promise.reject(taken()));
  render(work, request, new Map(), null, false, { listDirectory: () => Promise.resolve({ type: "dir_listing", path: "/home/dev/.claude-picked", entries: [] }) });
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  const press = (text: string) => act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === text)!.click());
  await press("Browse");
  await press("Select this directory");
  expect(input("Config directory").value).toBe("/home/dev/.claude-picked");
  await submit();
  expect(request).toHaveBeenCalledTimes(2);
});

it.each([
  { name: "the absolute directory it holds", typed: "/home/dev/.claude-other", opensAt: "/home/dev/.claude-other" },
  { name: "a home directory", typed: "~/.claude-other", opensAt: "~/.claude-other" },
  { name: "the home directory itself", typed: "~", opensAt: "~" },
  { name: "the home directory for a relative one", typed: "other", opensAt: "~" },
])("opens the directory browser at $name", async ({ typed, opensAt }) => {
  const daemon = render(work, () => Promise.resolve());
  type("Config directory", typed);
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Browse")!.click());
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "list_dir", path: opensAt });
});

it("shows the config directory hint as a visible description", () => {
  render(work, () => Promise.resolve());
  expect(document.body.textContent).toContain("does not need to exist yet.");
  expect(document.body.textContent).not.toContain("To retain login and conversations");
});

it("shows the generic config directory hint alone for a provider whose module has none of its own", () => {
  render({ ...work, agent: "test_agent" }, () => Promise.resolve());
  expect(document.body.textContent).toContain("does not need to exist yet.");
  expect(document.body.textContent).not.toContain("To retain login and conversations");
});

it("adds the Grok home note to the config directory hint for Grok", () => {
  render({ ...work, agent: "grok" as const }, () => Promise.resolve());
  expect(document.body.textContent).toContain("To retain login and conversations, sessions need a Grok home initialized by running the CLI with that directory. This is checked when a session launches.");
});

it("fixes the agent when renaming a default account, so no other agent's default can be renamed", async () => {
  const request = vi.fn(() => Promise.resolve());
  render(undefined, request, new Map(), null, true);
  expect(document.body.textContent).not.toContain("Agent");
  expect(document.body.querySelector("[role=listbox], [role=combobox], select")).toBeNull();
  type("Name", "Main");
  await submit();
  expect(request).toHaveBeenCalledWith({ type: "set_default_account_name", agent: "claude", name: "Main" });
});

const denied = () => new CodedError("Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY", "store_write_failed", { cause: "access", detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" });

it("gives a denied rename of a default account the data folder remedy", async () => {
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  render(undefined, () => Promise.reject(denied()), new Map(), null, true);
  type("Name", "Main");
  await submit();
  expect(errorAlerts.getSnapshot().map((alert) => alert.recovery)).toEqual(["appDataAccess"]);
});

it("leaves a denied edit of an account the remedy of the failure itself", async () => {
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  render(work, () => Promise.reject(denied()));
  type("Name", "Other");
  await submit();
  // The edit holds a configuration directory the person chose, so it keeps the remedy that may choose another one.
  expect(errorAlerts.getSnapshot().map((alert) => alert.recovery)).toEqual([undefined]);
});

it("prefills and submits the first free directory across disk entries and registered accounts", async () => {
  const request = vi.fn(() => Promise.resolve());
  render(undefined, request, new Map(), null, false, {
    accounts: [{ ...work, config_dir: "/home/dev/.claude-2" }],
    listDirectory: () => Promise.resolve({ type: "dir_listing", path: "/home/dev", entries: [
      { name: ".claude-1", path: "/home/dev/.claude-1", is_git_repo: false },
      { name: ".claude-4", path: "/home/dev/.claude-4", is_git_repo: false },
    ] }),
  });
  await act(async () => {});
  expect(input("Config directory").value).toBe("~/.claude-3");
  type("Name", "Personal");
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Add")!.click());
  expect(request).toHaveBeenCalledWith({ type: "create_account", agent: "claude", name: "Personal", config_dir: "~/.claude-3" });
});

it("updates automatic directories on agent changes and preserves manually entered directories", async () => {
  render(undefined, () => Promise.resolve());
  await act(async () => {});
  expect(input("Config directory").value).toBe("~/.claude-1");
  const select = async (label: string) => {
    await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
    await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((option) => option.textContent === label)!.click());
  };
  await select("Codex");
  expect(input("Config directory").value).toBe("~/.codex-1");
  type("Config directory", "~/custom");
  await select("Grok Build");
  expect(input("Config directory").value).toBe("~/custom");
});

it("keeps a manual directory when the home listing answers later", async () => {
  let resolve!: (value: unknown) => void;
  render(undefined, () => Promise.resolve(), new Map(), null, false, {
    listDirectory: () => new Promise((done) => { resolve = done; }),
  });
  type("Config directory", "~/custom");
  await act(async () => resolve({ type: "dir_listing", path: "/home/dev", entries: [] }));
  expect(input("Config directory").value).toBe("~/custom");
});

it("keeps failed suggestions retryable and returns retry focus to the filled directory", async () => {
  const failure = new CodedError("unreadable", "directory_unreadable", { path: "/home/dev", detail: "denied" });
  const listDirectory = vi.fn().mockRejectedValueOnce(failure).mockRejectedValueOnce(failure)
    .mockResolvedValue({ type: "dir_listing", path: "/home/dev", entries: [] });
  render(undefined, () => Promise.resolve(), new Map(), null, false, { listDirectory });
  await trackMountRefocus()();
  expect(input("Config directory").value).toBe("");
  expect(document.body.textContent).toContain("Could not list this directory");
  const field = input("Config directory");
  const failureId = field.getAttribute("aria-describedby")!.split(" ").find((id) => document.getElementById(id)?.textContent?.includes("Could not list this directory"))!;
  expect(failureId).toBeDefined();
  const announced = document.getElementById(failureId);
  const retry = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Try again")!;
  act(() => retry.focus());
  await act(async () => retry.click());
  expect(document.getElementById(failureId)).not.toBe(announced);
  expect(document.activeElement).toBe(retry);
  await act(async () => retry.click());
  expect(input("Config directory").value).toBe("~/.claude-1");
  expect(document.activeElement).toBe(field);
  expect(field.getAttribute("aria-describedby")).not.toContain(failureId);
  expect(document.body.textContent).not.toContain("Could not list this directory");
});
