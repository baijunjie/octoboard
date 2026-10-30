// Shared by the `ConsoleDialog` test files, which split the dialog's tests by area.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

import { accountOf, consoleOf } from "../gallery/fixtures/builders";
import type { Account, Agent, AgentAvailability, AgentModel, Console, Event, ModelsUnavailable, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConsoleDialog } from "./ConsoleDialog";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";
import { DEFAULT_SETTINGS } from "../defaultSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

export const work: Account = accountOf("a-work", "claude", "Work", "/home/dev/.claude-work");
export const personal: Account = accountOf("a-personal", "claude", "Personal", "/home/dev/.claude-personal");

export const MODELS: AgentModel[] = [
  { id: "alpha", name: "Alpha", efforts: ["low", "high"] },
  { id: "beta", name: "Beta", efforts: ["medium", "high", "max"] },
  { id: "plain", name: "Plain", efforts: [] },
];

/** The reply to a `list_agent_models` request: `models` as listed, or why there are none. */
export const listing = (body: RequestBody, models: AgentModel[], unavailable: ModelsUnavailable | null = null): Event => ({
  type: "agent_models",
  id: null,
  agent: (body as { agent: "claude" }).agent,
  account: (body as { account?: string }).account ?? null,
  models,
  unavailable,
});

/** The store of the daemon the last `render` set up, for a test that changes the state under it. */
export let lastStore: ReturnType<typeof createStateStore>;

/** Renders the dialog over a daemon that answers every request, with `MODELS` unless `respond` says
 * otherwise, and returns the requests sent. */
export function render(
  editing?: Console,
  accounts: Account[] = [],
  respond?: (body: RequestBody) => Promise<Event>,
  availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {},
  added: Agent[] | null = null,
) {
  const request = vi.fn((body: RequestBody) => respond?.(body) ?? Promise.resolve(body.type === "list_agent_models" ? listing(body, MODELS) : ({ type: "ack", id: null } as Event)));
  const daemon: Daemon = {
    store: createStateStore({
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: added, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
      agentAvailability: new Map(
        (["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined", default_account_dir: null }]),
      ),
    }),
    request,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
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

export const buttons = () => Array.from(document.body.querySelectorAll("button"));
export const click = (el: Element | null | undefined) => act(async () => (el as HTMLElement | undefined)?.click());
export const clickButton = (text: string | ((b: HTMLButtonElement) => boolean)) =>
  click(buttons().find((b) => (typeof text === "string" ? b.textContent === text : text(b))));
export const choose = (option: string) =>
  click(Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === option));
/** What the form sent apart from its questions about models and the quota refresh Settings' Agent accounts section
 * asks for when it opens over the dialog. */
export const sent = (request: ReturnType<typeof render>) =>
  request.mock.calls.map((call) => call[0]).filter((body) => body.type !== "list_agent_models" && body.type !== "refresh_account_quotas");
export const asked = (request: ReturnType<typeof render>) => request.mock.calls.map((call) => call[0]).filter((body) => body.type === "list_agent_models");

export function type(label: string, value: string) {
  const input = Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find(
    (el) => el.labels?.[0]?.textContent === label,
  )!;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

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
    .map((el) => el!.textContent ?? "");
/** The Advanced picker of the setting `name` for the sessions of `group`, by the elements that name it. */
export const groupPicker = (group: string, name: string) =>
  buttons().find((b) => {
    const [kind, setting] = nameOf(b);
    return kind === group && setting === name;
  })!;
/** The console session picker of the Advanced setting `name`. */
export const pickerNamed = (name: string) => groupPicker("Console sessions", name);
export const clearButton = (name: string) => pickerNamed(name).querySelector<HTMLElement>('[data-slot="select-clear-button"]');

export const stored = consoleOf("c-1", "Main", {
  launch: { claude: { model: "alpha", effort: "high" }, codex: { model: "retired" } },
});

export const optionTexts = () => Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent);

/** The trigger of a picker named by its own label rather than by a visible one. */
export const picker = (name: string) => buttons().find((b) => b.getAttribute("aria-label") === name)!;

export const tabNames = () => Array.from(document.body.querySelectorAll('[role="tab"]')).map((tab) => tab.textContent);
