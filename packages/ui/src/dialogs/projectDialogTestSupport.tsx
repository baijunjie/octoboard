// Shared by the `ProjectDialog` test files, which split the dialog's tests by area. Importing it fakes the timers
// for each test of the importing file and takes down what the test rendered after it.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, vi } from "vitest";

import { consoleOf, projectOf } from "../gallery/fixtures/builders";
import type { Account, Agent, AgentAvailability, Console, Event, Project, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProjectDialog } from "./ProjectDialog";
import { DEFAULT_WORKTREE_SETTINGS } from "../worktreeSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A daemon whose replies the test hands out itself, in whatever order it likes. */
export type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (error: Error) => void };
export const roots: Root[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
  vi.useRealTimers();
});

export const work: Account = { id: "a-work", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" };
export const personal: Account = { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/home/dev/.claude-personal" };

export function render(
  editing?: Project,
  accounts: Account[] = [],
  consoleOverride: Partial<Console> = {},
  availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {},
  added: Agent[] | null = null,
) {
  const pending: Pending[] = [];
  const daemon: Daemon = {
    store: createStateStore({
      consoles: new Map([["c-1", consoleOf("c-1", "Main", { claude_account_id: accounts[0]?.id, ...consoleOverride })]]),
      agentAvailability: new Map(
        (["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined" }]),
      ),
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", worktrees: DEFAULT_WORKTREE_SETTINGS, accounts },
    }),
    request: (body) => new Promise((resolve, reject) => pending.push({ body, resolve, reject })),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <ProjectDialog consoleId="c-1" project={editing} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  return Object.assign(pending, { store: daemon.store });
}

export const input = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((el) => el.labels?.[0]?.textContent === label)!;

export function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

export const settle = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
export const click = (el: HTMLElement | undefined) => act(async () => el?.click());
export const answer = (request: Pending | undefined, agent: Agent | null) =>
  act(async () => request?.resolve({ type: "agent_detected", agent }));

export async function chooseSource(label: string) {
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent?.includes("A single directory")));
  await click(Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes(label)));
}

export const buttons = () => Array.from(document.body.querySelectorAll("button"));
export const buttonText = (text: string) => buttons().find((b) => b.textContent === text);
/** The account picker, which is the one named for the agent's account. */
export const accountPicker = () => buttons().find((b) => b.getAttribute("aria-label") === "Claude Code account" || b.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.getAttribute("aria-label") === "Claude Code account"));
export const pickerNamed = (name: string) =>
  buttons().find((b) => b.getAttribute("aria-label") === name || b.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name))!;
export const sentOf = (pending: Pending[], type: string) => pending.find((p) => p.body.type === type)?.body;

/** Fills in a directory whose agent is not detected, so the form is open and adds as inheriting. */
export async function fillDirectory(pending: Pending[]) {
  type(input("Directory"), "~/code/x");
  await settle(500);
  await answer(pending[0], null);
}

export const stored = projectOf("p-1", "c-1", "Site", {
  default_agent: "claude",
  claude_account_id: { kind: "account", id: "a-work" },
  claude_session_model: "alpha",
  claude_session_effort: "high",
  codex_session_model: "retired",
});
