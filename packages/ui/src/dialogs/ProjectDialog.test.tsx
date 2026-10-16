// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { consoleOf, projectOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Account, Agent, Event, ModelsUnavailable, Project, ProjectAccount, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProjectDialog } from "./ProjectDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A daemon whose replies the test hands out itself, in whatever order it likes. */
type Pending = { body: RequestBody; resolve: (event: Event) => void; reject: (error: Error) => void };
const roots: Root[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
  vi.useRealTimers();
});

const work: Account = { id: "a-work", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" };
const personal: Account = { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/home/dev/.claude-personal" };

function render(editing?: Project, accounts: Account[] = []) {
  const pending: Pending[] = [];
  const daemon: Daemon = {
    store: createStateStore({
      consoles: new Map([["c-1", consoleOf("c-1", "Main", { claude_account_id: accounts[0]?.id })]]),
      settings: { auto_pull_repositories: false, default_clone_dir: "", accounts },
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
  return pending;
}

const input = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((el) => el.labels?.[0]?.textContent === label)!;

/** The hidden native select behind the Default agent field, which holds the chosen value. */
const agentValue = () =>
  Array.from(document.body.querySelectorAll("select")).find((el) => Array.from(el.options).some((o) => o.value === "codex"))!.value;

function type(el: HTMLInputElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const settle = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
const click = (el: HTMLElement | undefined) => act(async () => el?.click());
const answer = (request: Pending | undefined, agent: Agent | null) =>
  act(async () => request?.resolve({ type: "agent_detected", agent }));

async function chooseSource(label: string) {
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent?.includes("A single directory")));
  await click(Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes(label)));
}

it("preselects the agent detected for a typed directory, and only the latest answer counts", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  await settle(500);
  type(input("Directory"), "~/code/b");
  await settle(500);
  expect(pending.map((p) => p.body)).toEqual([
    { type: "detect_directory_agent", path: "~/code/a" },
    { type: "detect_directory_agent", path: "~/code/b" },
  ]);
  await answer(pending[1], "grok");
  await answer(pending[0], "codex");
  expect(agentValue()).toBe("grok");
});

it("sets the refused directory as danger-toned code under the field", async () => {
  const pending = render();
  type(input("Directory"), "~/code/missing");
  await settle(500);
  await act(async () => pending[0].reject(new DaemonRequestError("plain", "path_not_found", { path: "/Users/dev/code/missing" })));
  const code = document.body.querySelector('[role="alert"] code');
  expect(code?.textContent).toBe("/Users/dev/code/missing");
  expect(code?.getAttribute("data-color")).toBe("danger");
});

it("keeps the form locked while the answer for the path as it stands is out", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  // Locked from the keystroke on: the wait before the ask is part of the check.
  expect(input("Name").disabled).toBe(true);
  await settle(500);
  expect(input("Name").disabled).toBe(true);
  await answer(pending[0], null);
  expect(input("Name").disabled).toBe(false);
});

it("keeps the git form locked until the probe succeeds", async () => {
  const pending = render();
  await chooseSource("git repository URL");
  const clone = () => input("Clone into (parent directory)");
  expect(clone().disabled).toBe(true);
  type(input("Repository URL"), "https://example.com/o/r.git");
  await settle(900);
  expect(pending[0].body).toEqual({ type: "probe_git_remote", remote_url: "https://example.com/o/r.git" });
  expect(clone().disabled).toBe(true);
  await answer(pending[0], null);
  expect(clone().disabled).toBe(false);
});

it.each([
  { name: "a detected agent is stored as shown", source: "A single directory", detected: "codex" as Agent | null, sent: { default_agent: "codex", detect_default_agent: false } },
  { name: "no detected agent stays inherited", source: "A single directory", detected: null, sent: { detect_default_agent: false } },
  { name: "a parent directory detects per repository", source: "A parent directory", detected: null, sent: {} },
])("sends add_project with $name", async ({ source, detected, sent }) => {
  const pending = render();
  if (source !== "A single directory") await chooseSource(source);
  type(input("Directory"), "~/code/x");
  if (source === "A single directory") {
    await settle(500);
    await answer(pending[0], detected);
  }
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Add"));
  const added = pending.find((p) => p.body.type === "add_project")?.body;
  expect(added).toMatchObject({ type: "add_project", source: source === "A single directory" ? "local" : "parent", ...sent });
  if (!("default_agent" in sent)) expect(added).not.toHaveProperty("default_agent", expect.anything());
});

const PLACEHOLDER = "The agent's own setting";
const buttons = () => Array.from(document.body.querySelectorAll("button"));
const buttonText = (text: string) => buttons().find((b) => b.textContent === text);
const choose = (option: string) =>
  click(Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === option) as HTMLElement);
/** The account picker, which is the one named for the agent's account. */
const accountPicker = () => buttons().find((b) => b.getAttribute("aria-label") === "Claude Code account" || b.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.getAttribute("aria-label") === "Claude Code account"));
const pickerNamed = (name: string) =>
  buttons().find((b) => b.getAttribute("aria-label") === name || b.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name))!;
const clearButton = (name: string) => pickerNamed(name).querySelector<HTMLElement>('[data-slot="select-clear-button"]');
const sentOf = (pending: Pending[], type: string) => pending.find((p) => p.body.type === type)?.body;

const optionTexts = () => Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent);
const modelRequests = (pending: Pending[]) => pending.filter((p) => p.body.type === "list_agent_models");

/** Answers the latest question about models with a list, or with a reason there is none. */
const answerModels = (pending: Pending[], unavailable: ModelsUnavailable | null = null) =>
  act(async () => {
    const asked = modelRequests(pending).at(-1)!;
    const body = asked.body as { agent: Agent; account?: string };
    asked.resolve({
      type: "agent_models",
      agent: body.agent,
      account: body.account ?? null,
      models: unavailable ? [] : [{ id: "alpha", name: "Alpha", efforts: ["low", "high"] }, { id: "beta", name: "Beta", efforts: [] }],
      unavailable,
    });
  });

/** Fills in a directory whose agent is not detected, so the form is open and adds as inheriting. */
async function fillDirectory(pending: Pending[]) {
  type(input("Directory"), "~/code/x");
  await settle(500);
  await answer(pending[0], null);
}

it("adds without an account or launch settings when none is set", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(buttonText("Add"));
  const added = sentOf(pending, "add_project");
  expect(added).toMatchObject({ type: "add_project", source: "local" });
  for (const field of ["claude_account_id", "claude_session_model", "claude_session_effort"]) expect(added).not.toHaveProperty(field);
});

it("adds with the account, model and effort that were set", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Personal");
  await click(buttonText("Advanced"));
  await answerModels(pending);
  await click(pickerNamed("Model"));
  await choose("Alpha");
  await click(pickerNamed("Reasoning effort"));
  await choose("high");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({
    claude_account_id: { kind: "account", id: "a-personal" },
    claude_session_model: "alpha",
    claude_session_effort: "high",
  });
});

it("shows an unset launch setting as its placeholder, and names what the account inherits", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  // The console refers to the first account.
  expect(accountPicker()?.textContent).toContain("Work (from the console)");
  await click(buttonText("Advanced"));
  expect(pickerNamed("Model").textContent).toContain(PLACEHOLDER);
  expect(pickerNamed("Reasoning effort").textContent).toContain(PLACEHOLDER);
  // The note renders outside a HeroUI field, which its `Description` has to cope with.
  expect(document.body.textContent).toContain("Used by sessions");
});

it("offers no account picker for an agent with no account of its own", async () => {
  const pending = render();
  await fillDirectory(pending);
  expect(accountPicker()).toBeUndefined();
});

const stored = projectOf("p-1", "c-1", "Site", {
  default_agent: "claude",
  claude_account_id: { kind: "account", id: "a-work" },
  claude_session_model: "alpha",
  claude_session_effort: "high",
  codex_session_model: "retired",
});

it("sends only what changed when editing", async () => {
  const pending = render(stored, [work, personal]);
  await click(buttonText("Advanced"));
  await answerModels(pending);
  await click(pickerNamed("Reasoning effort"));
  await choose("low");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({
    type: "update_project",
    project: "p-1",
    name: "Site",
    default_agent: "claude",
    claude_session_effort: "low",
    tags: [],
  });
});

it("sends an explicit null when a picker is returned to inherit or unset", async () => {
  const pending = render(stored, [work, personal]);
  await click(accountPicker());
  await choose("Work (from the console)");
  await click(buttonText("Advanced"));
  await answerModels(pending);
  await click(clearButton("Model") ?? undefined);
  expect(pickerNamed("Model").textContent).toContain(PLACEHOLDER);
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ claude_account_id: null, claude_session_model: null });
  expect(sentOf(pending, "update_project")).not.toHaveProperty("claude_session_effort");
});

// The console refers to the first account, "Work".
const stateOf = (claude_account_id?: ProjectAccount) => projectOf("p-3", "c-1", "Site", { default_agent: "claude", claude_account_id });

it("lists inheriting the console's, the default account, then the named accounts", async () => {
  render(undefined, [work, personal]);
  await click(accountPicker());
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual([
    "Work (from the console)",
    "Default",
    "Work",
    "Personal",
  ]);
});

it.each([
  ["inheriting", undefined, "Work (from the console)"],
  ["inheriting, written out", { kind: "inherit" } as const, "Work (from the console)"],
  ["the default account", { kind: "default" } as const, "Default"],
  ["a named account", { kind: "account", id: "a-personal" } as const, "Personal"],
])("opens a project on %s showing it", async (_, stored, shown) => {
  render(stateOf(stored), [work, personal]);
  expect(accountPicker()?.textContent).toContain(shown);
});

it("sends the default account on create, over a console that pins a named one", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Default");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ claude_account_id: { kind: "default" } });
});

it("sends a named account on create as the account kind", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Personal");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ claude_account_id: { kind: "account", id: "a-personal" } });
});

it("omits the account on create when it is left inheriting, even after being picked and put back", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Default");
  await click(accountPicker());
  await choose("Work (from the console)");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).not.toHaveProperty("claude_account_id");
});

it.each([
  ["inheriting to the default account", undefined, "Default", { kind: "default" }],
  ["a named account to the default account", { kind: "account", id: "a-work" } as const, "Default", { kind: "default" }],
  ["the default account to a named account", { kind: "default" } as const, "Personal", { kind: "account", id: "a-personal" }],
  ["the default account back to inheriting", { kind: "default" } as const, "Work (from the console)", null],
])("sends %s on update", async (_, from, pick, sent) => {
  const pending = render(stateOf(from), [work, personal]);
  await click(accountPicker());
  await choose(pick);
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ claude_account_id: sent });
});

it("leaves the account out of an update that does not touch it, whatever state it is in", async () => {
  const pending = render(stateOf({ kind: "default" }), [work, personal]);
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).not.toHaveProperty("claude_account_id");
});

it("keeps an edit to an agent that is no longer the selected one", async () => {
  const pending = render(stored, [work, personal]);
  await click(accountPicker());
  await choose("Personal");
  await click(buttons().find((b) => b.textContent?.includes("Claude Code")));
  await choose("Codex");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({
    type: "update_project",
    project: "p-1",
    name: "Site",
    default_agent: "codex",
    claude_account_id: { kind: "account", id: "a-personal" },
    tags: [],
  });
});

it("follows the console's agent for Inherit from console", async () => {
  const pending = render(projectOf("p-2", "c-1", "Site"), [work, personal]);
  expect(accountPicker()).toBeDefined();
  await click(buttons().find((b) => b.textContent?.includes("Inherit from console (Claude Code)")));
  await choose("Codex");
  expect(accountPicker()).toBeUndefined();
  expect(pending).toEqual([]);
});

it("offers neither the account nor Advanced while the source is a parent directory with Detect from files", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Personal");
  await chooseSource("A parent directory");
  expect(accountPicker()).toBeUndefined();
  expect(buttonText("Advanced")).toBeUndefined();
  await click(buttonText("Add"));
  // What was set before is out of sight and not sent.
  expect(sentOf(pending, "add_project")).not.toHaveProperty("claude_account_id");
});

const group = (label: string) =>
  Array.from(document.body.querySelectorAll('[role="group"]')).find(
    (el) => document.getElementById(el.getAttribute("aria-labelledby") ?? "")?.textContent === label,
  )!;

it("locks the launch settings and dims the group's label with the rest of the form while the path is checked", async () => {
  const pending = render(undefined, [work, personal]);
  type(input("Directory"), "~/code/a");
  // Folding the section stays possible; what is inside is locked.
  await click(buttonText("Advanced"));
  expect(pickerNamed("Model").hasAttribute("disabled")).toBe(true);
  expect(pickerNamed("Reasoning effort").hasAttribute("disabled")).toBe(true);
  expect(group("Default agent").getAttribute("data-disabled")).toBe("true");
  await settle(500);
  // The model list was asked for as soon as the section opened, ahead of the detection.
  await answer(pending.find((p) => p.body.type === "detect_directory_agent"), null);
  await answerModels(pending);
  expect(pickerNamed("Model").hasAttribute("disabled")).toBe(false);
  expect(group("Default agent").hasAttribute("data-disabled")).toBe(false);
});

it("keeps the dynamic rich detection hint attached to the single agent picker", async () => {
  render();
  await chooseSource("A parent directory");
  const picker = pickerNamed("Default agent");
  const described = picker.getAttribute("aria-describedby")?.split(" ").map((id) => document.getElementById(id));
  const hint = described?.find((el) => el?.textContent?.includes("applies to every repository"));
  expect(hint).toBeDefined();
  expect(Array.from(hint!.querySelectorAll("code")).map((el) => el.textContent)).toEqual([
    "CLAUDE.md", "AGENTS.md", ".claude", ".codex",
  ]);
  await click(buttons().find((b) => b.textContent?.includes("A parent directory")));
  await choose("A single directory");
  expect(pickerNamed("Default agent")).toBe(picker);
  expect(picker.hasAttribute("aria-describedby")).toBe(false);
  expect(document.getElementById(hint!.id)).toBeNull();
});

it("describes the joined agent and account group by the parent-directory hint", async () => {
  render(undefined, [work, personal]);
  await chooseSource("A parent directory");
  await click(buttons().find((b) => b.textContent?.includes("Detect from files")));
  await choose("Claude Code");
  const described = group("Default agent").getAttribute("aria-describedby");
  const hint = document.getElementById(described ?? "");
  expect(hint?.textContent).toContain("applies to every repository");
  expect(Array.from(hint!.querySelectorAll("code")).map((el) => el.textContent)).toEqual([
    "CLAUDE.md", "AGENTS.md", ".claude", ".codex",
  ]);
});

it("asks for the models only once Advanced is opened, under the account the project resolves to", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  expect(modelRequests(pending)).toEqual([]);
  await click(buttonText("Advanced"));
  // Inheriting: the console's account.
  expect(modelRequests(pending).map((p) => p.body)).toEqual([{ type: "list_agent_models", agent: "claude", account: "a-work" }]);
  await answerModels(pending);
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Alpha", "Beta"]);
});

it("asks again when the project's own account is chosen, and for the default account without one", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(buttonText("Advanced"));
  await click(accountPicker());
  await choose("Personal");
  await click(accountPicker());
  await choose("Default");
  expect(modelRequests(pending).map((p) => p.body)).toEqual([
    { type: "list_agent_models", agent: "claude", account: "a-work" },
    { type: "list_agent_models", agent: "claude", account: "a-personal" },
    { type: "list_agent_models", agent: "claude" },
  ]);
});

it("keeps a stored model the list lacks, and says why when there is no list", async () => {
  const pending = render(projectOf("p-2", "c-1", "Site", { default_agent: "claude", claude_session_model: "retired" }));
  await click(buttonText("Advanced"));
  await answerModels(pending);
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Alpha", "Beta", "retired (not in the list)"]);
});

it("offers the stored model alone and the reason when the agent cannot list its models", async () => {
  const pending = render(projectOf("p-2", "c-1", "Site", { default_agent: "claude", claude_session_model: "retired" }));
  await click(buttonText("Advanced"));
  expect(document.body.textContent).toContain("Loading the models this agent offers");
  await answerModels(pending, { code: "models_timed_out", params: { agent: "claude" }, message: "" });
  expect(document.body.textContent).toContain(`${isolate("claude")} took too long to list its models.`);
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["retired (not in the list)"]);
});
