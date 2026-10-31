// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { accountOf } from "../gallery/fixtures/builders";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import type { Account, Agent, AgentAvailability, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { AccountDialog } from "./AccountDialog";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";
import { DEFAULT_SETTINGS } from "../defaultSettings";
import { trackMountRefocus } from "../dialogs/dialogTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The picker's own listing is not under test; a button stands in for choosing a directory in it.
vi.mock("../dialogs/DirectoryPicker", () => ({
  DirectoryPicker: ({ onPick }: { onPick: (path: string) => void }) => (
    <button type="button" onClick={() => onPick("/home/dev/.claude-picked")}>Pick</button>
  ),
}));

const work: Account = accountOf("a-work", "claude", "Work", "/home/dev/.claude-work");

function render(
  editing: Account | undefined,
  request: (body: RequestBody) => Promise<unknown>,
  agentAvailability = new Map<Agent, AgentAvailability>(),
  addedAgents: Agent[] | null = null,
  renameDefault = false,
  options: { accounts?: Account[]; listDirectory?: () => Promise<unknown> } = {},
) {
  const daemon: Daemon = {
    store: createStateStore({ settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: addedAgents, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: options.accounts ?? [work] }, agentAvailability }),
    request: vi.fn((body: RequestBody) => body.type === "list_dir"
      ? options.listDirectory?.() ?? Promise.resolve({ type: "dir_listing", path: "/home/dev", entries: [] })
      : request(body)) as Daemon["request"],
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
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <AccountDialog account={editing} agent="claude" renameDefault={renameDefault} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  return daemon;
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
  expect(options.map((o) => o.textContent)).toEqual(["Claude Code", "Codex, Not installed", "Grok Build"]);
  expect(options.some((o) => o.hasAttribute("data-disabled") || o.getAttribute("aria-disabled") === "true")).toBe(false);
});

it("lists only the added agents, in the list's order", async () => {
  render(undefined, () => Promise.resolve(), availabilityOf("codex"), ["grok", "codex"]);
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual(["Grok Build", "Codex, Not installed"]);
});

const taken = () =>
  new DaemonRequestError("taken", "config_dir_taken", { agent: "Claude Code", name: "Work" });

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
  render(work, request);
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  const press = (text: string) => act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === text)!.click());
  await press("Browse");
  await press("Pick");
  expect(input("Config directory").value).toBe("/home/dev/.claude-picked");
  await submit();
  expect(request).toHaveBeenCalledTimes(2);
});

it("shows the config directory hint as a visible description", () => {
  render(work, () => Promise.resolve());
  expect(document.body.textContent).toContain("does not need to exist yet.");
  expect(document.body.textContent).not.toContain("sessions only keep their login");
});

it("adds the Grok home note to the config directory hint for Grok", () => {
  render({ ...work, agent: "grok" as const }, () => Promise.resolve());
  expect(document.body.textContent).toContain("but sessions only keep their login and conversation");
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
  const failure = new DaemonRequestError("unreadable", "directory_unreadable", { path: "/home/dev", detail: "denied" });
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
