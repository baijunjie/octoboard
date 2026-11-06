// Shared by the `ConsoleDialog` test files, which split the dialog's tests by area.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import { consoleOf } from "../fixtures/builders";
import { type Account, type Agent, type AgentAvailability, type AgentModel, type Console, type Event, type ModelsUnavailable, type ClientRequestBody } from "../protocol";
import { TEST_AGENTS } from "../test/testProviders";
import { createStateStore, DaemonProvider } from "../store";
import { buttons, click, inputLabelled, type as typeInto, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { personal, work } from "../fixtures/accountTestSupport";
import { ConsoleDialog } from "./ConsoleDialog";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { partSelector } from "../primitives";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

export { work, personal };

export const MODELS: AgentModel[] = [
  { id: "alpha", name: "Alpha", efforts: ["low", "high"] },
  { id: "beta", name: "Beta", efforts: ["medium", "high", "max"] },
  { id: "plain", name: "Plain", efforts: [] },
];

/** The reply to a `list_agent_models` request: `models` as listed, or why there are none, and the
 * model a session gets when no setting names one. */
export const listing = (
  body: ClientRequestBody,
  models: AgentModel[],
  unavailable: ModelsUnavailable | null = null,
  defaultModel?: string,
): Event => ({
  type: "agent_models",
  id: null,
  agent: (body as { agent: "claude" }).agent,
  account: (body as { account?: string }).account ?? null,
  models,
  ...(defaultModel && { default_model: defaultModel }),
  unavailable,
});

/** The store of the daemon the last `render` set up, for a test that changes the state under it. */
export let lastStore: ReturnType<typeof createStateStore>;

/** Renders the dialog over a daemon that answers every request, with `MODELS` unless `respond` says
 * otherwise, and returns the requests sent. */
export function render(
  editing?: Console,
  accounts: Account[] = [],
  respond?: (body: ClientRequestBody) => Promise<Event>,
  availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {},
  added: Agent[] | null = null,
) {
  const request = vi.fn((body: ClientRequestBody) => respond?.(body) ?? Promise.resolve(body.type === "list_agent_models" ? listing(body, MODELS) : ({ type: "ack", id: null } as Event)));
  const daemon = testDaemon({
    state: {
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
      agentAvailability: new Map(
        TEST_AGENTS.map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined", default_account_dir: null }]),
      ),
    },
    request,
    showNotice: vi.fn(),
  });
  lastStore = daemon.store;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <ConsoleDialog console={editing} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  return request;
}

export { buttons, click };
export const clickButton = (text: string | ((b: HTMLButtonElement) => boolean)) =>
  click(buttons().find((b) => (typeof text === "string" ? b.textContent === text : text(b))));
export const choose = (option: string) =>
  click(Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === option));
/** What the form sent apart from its questions about models and the quota refresh Settings' Agent accounts section
 * asks for when it opens over the dialog. */
export const sent = (request: ReturnType<typeof render>) =>
  request.mock.calls.map((call) => call[0]).filter((body) => body.type !== "list_agent_models" && body.type !== "refresh_account_quotas");
export const asked = (request: ReturnType<typeof render>) => request.mock.calls.map((call) => call[0]).filter((body) => body.type === "list_agent_models");

export const type = (label: string, value: string) => typeInto(inputLabelled(label), value);

/** Opens the Advanced section. The model picker is the first launch setting, the effort the second. */
export async function openAdvanced() {
  await clickButton("Advanced");
}
/** The agent tab named `name` in the Advanced section. */
export const clickTab = (name: string) => click(Array.from(document.body.querySelectorAll('[role="tab"]')).find((tab) => tab.textContent === name));
/** The name of `button` that comes from other elements, put together as `aria-labelledby` says (the
 * value shown in the button is part of the name too, and is left out). */
export const nameOf = (button: Element) =>
  (button.getAttribute("aria-labelledby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id))
    .filter((el) => el && !button.contains(el))
    .map((el) => withoutIsolates(el!.textContent) ?? "");
/** The Advanced picker of the setting `name` for the sessions of `group`, by the elements that name it. */
export const groupPicker = (group: string, name: string) =>
  buttons().find((b) => {
    const [kind, setting] = nameOf(b);
    return kind === group && setting === name;
  })!;
/** The console session picker of the Advanced setting `name`. */
export const pickerNamed = (name: string) => groupPicker("Console sessions", name);
export const clearButton = (name: string) => pickerNamed(name).parentElement!.querySelector<HTMLElement>(partSelector.selectClearButton);

export const stored = consoleOf("c-1", "Main", {
  launch: { claude: { model: "alpha", effort: "high" }, codex: { model: "retired" } },
});

export const optionTexts = () => Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent);

/** The trigger of a picker named by its own label rather than by a visible one. */
export const picker = (name: string) => buttons().find((b) => withoutIsolates(b.getAttribute("aria-label")) === name)!;

export const tabNames = () => Array.from(document.body.querySelectorAll('[role="tab"]')).map((tab) => tab.textContent);
