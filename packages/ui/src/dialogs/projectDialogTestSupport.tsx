// Shared by the `ProjectDialog` test files, which split the dialog's tests by area. Importing it fakes the timers
// for each test of the importing file and takes down what the test rendered after it.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, vi } from "vitest";

import { type ConsoleExtra, consoleOf, projectOf } from "../fixtures/builders";
import type { Account, Agent, AgentAvailability, Event, Project, ClientRequestBody } from "../protocol";
import { DaemonProvider } from "../store";
import { buttons, click, inputLabelled, pickerNamed, type, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { personal, work } from "../fixtures/accountTestSupport";
import { ProjectDialog } from "./ProjectDialog";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A daemon whose replies the test hands out itself, in whatever order it likes. */
export type Pending = { body: ClientRequestBody; resolve: (event: Event) => void; reject: (error: Error) => void };
export const roots: Root[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
  vi.useRealTimers();
});

export { work, personal };

export function render(
  editing?: Project,
  accounts: Account[] = [],
  consoleOverride: ConsoleExtra = {},
  availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {},
  added: Agent[] | null = null,
) {
  const pending: Pending[] = [];
  const daemon = testDaemon({
    state: {
      consoles: new Map([["c-1", consoleOf("c-1", "Main", { accounts: { claude: accounts[0]?.id }, ...consoleOverride })]]),
      agentAvailability: new Map(
        (["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined", default_account_dir: null }]),
      ),
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
    },
    request: (body) => new Promise((resolve, reject) => pending.push({ body, resolve, reject })),
    showNotice: vi.fn(),
  });
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

export { buttons, click, pickerNamed, type };
export const input = (label: string) => inputLabelled(label);

export const settle = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
export const answer = (request: Pending | undefined, agent: Agent | null, directoryExists?: boolean) =>
  act(async () => request?.resolve({ type: "agent_detected", id: null, agent, ...(directoryExists !== undefined && { directory_exists: directoryExists }) }));

export async function chooseSource(label: string) {
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent?.includes("A single directory")));
  await click(Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes(label)));
}

export const buttonText = (text: string) => buttons().find((b) => b.textContent === text);
/** The account picker, which is the one named for the agent's account. */
export const accountPicker = () => buttons().find((b) => withoutIsolates(b.getAttribute("aria-label")) === "Claude Code account" || b.getAttribute("aria-labelledby")?.split(" ").some((id) => withoutIsolates(document.getElementById(id)?.getAttribute("aria-label")) === "Claude Code account"));
export const sentOf = (pending: Pending[], type: string) => pending.find((p) => p.body.type === type)?.body;

/** Fills in a directory whose agent is not detected, so the form is open and adds as inheriting. */
export async function fillDirectory(pending: Pending[]) {
  type(input("Directory"), "~/code/x");
  await settle(500);
  await answer(pending[0], null);
}

export const stored = projectOf("p-1", "c-1", "Site", {
  default_agent: "claude",
  accounts: { claude: { kind: "account", id: "a-work" } }, launch: { claude: { model: "alpha", effort: "high" }, codex: { model: "retired" } },
});
