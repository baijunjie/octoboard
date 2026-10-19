// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import type { Account, Agent, AgentAvailability, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { AccountDialog } from "./AccountDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The picker's own listing is not under test; a button stands in for choosing a directory in it.
vi.mock("../dialogs/DirectoryPicker", () => ({
  DirectoryPicker: ({ onPick }: { onPick: (path: string) => void }) => (
    <button type="button" onClick={() => onPick("/home/dev/.claude-picked")}>Pick</button>
  ),
}));

const work: Account = { id: "a-work", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" };

function render(
  editing: Account | undefined,
  request: (body: RequestBody) => Promise<unknown>,
  agentAvailability = new Map<Agent, AgentAvailability>(),
  addedAgents: Agent[] | null = null,
  renameDefault = false,
) {
  const daemon: Daemon = {
    store: createStateStore({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: addedAgents, default_clone_dir: "", accounts: [work] }, agentAvailability }),
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
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <AccountDialog account={editing} agent="claude" renameDefault={renameDefault} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
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
  (["claude", "codex", "grok"] as Agent[]).map((agent): [Agent, AgentAvailability] => [agent, { agent, availability: agent === unavailable ? "unavailable" : "available" }]),
);

it("lists every added agent as selectable when adding, naming the ones known to be missing", async () => {
  render(undefined, () => Promise.resolve(), availabilityOf("codex"));
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const options = Array.from(document.body.querySelectorAll('[role="option"]'));
  expect(options.map((o) => o.textContent)).toEqual(["Claude Code", "Codex (not installed)", "Grok Build"]);
  expect(options.some((o) => o.hasAttribute("data-disabled") || o.getAttribute("aria-disabled") === "true")).toBe(false);
});

it("lists only the added agents, in the list's order", async () => {
  render(undefined, () => Promise.resolve(), availabilityOf("codex"), ["grok", "codex"]);
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual(["Grok Build", "Codex (not installed)"]);
});

const taken = () =>
  new DaemonRequestError("taken", "config_dir_taken", { agent: "Claude Code", name: "Work" });

it("words a refused directory under the directory field, not as the dialog's failure line", async () => {
  render(work, () => Promise.reject(taken()));
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  const field = input("Config directory");
  expect(field.getAttribute("aria-invalid")).toBe("true");
  const message = document.body.textContent ?? "";
  expect(message.match(/already uses this config directory/g)).toHaveLength(1);
});

it("lifts the directory's refusal once the directory is edited", async () => {
  render(work, () => Promise.reject(taken()));
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  type("Config directory", "/home/dev/.claude-another");
  expect(input("Config directory").getAttribute("aria-invalid")).not.toBe("true");
  expect(document.body.textContent).not.toContain("already uses this config directory");
});

it("lifts the directory's refusal when a directory is picked", async () => {
  render(work, () => Promise.reject(taken()));
  type("Config directory", "/home/dev/.claude-other");
  await submit();
  expect(document.body.textContent).toContain("already uses this config directory");
  const press = (text: string) => act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === text)!.click());
  await press("Browse");
  await press("Pick");
  expect(input("Config directory").value).toBe("/home/dev/.claude-picked");
  expect(document.body.textContent).not.toContain("already uses this config directory");
});

it("shows the config directory hint as a visible description", () => {
  render(work, () => Promise.resolve());
  expect(document.body.textContent).toContain("does not require the directory to exist yet.");
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
