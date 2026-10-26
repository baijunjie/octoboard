// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { press } from "../components/detailsTestSupport";
import { projectOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Agent, ModelsUnavailable, ProjectAccount } from "../protocol";
import { type Pending, work, personal, render, input, type, settle, click, answer, chooseSource, buttons, buttonText, accountPicker, pickerNamed, sentOf, fillDirectory, stored } from "./projectDialogTestSupport";

const PLACEHOLDER = "Same as console, The agent's own setting";

const choose = (option: string) =>
  click(Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === option) as HTMLElement);

const clearButton = (name: string) => pickerNamed(name).querySelector<HTMLElement>('[data-slot="select-clear-button"]');

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
  expect(accountPicker()?.textContent).toContain("Same as console, Work");
  await click(buttonText("Advanced"));
  expect(pickerNamed("Model").textContent).toContain(PLACEHOLDER);
  expect(pickerNamed("Reasoning effort").textContent).toContain(PLACEHOLDER);
});

it("offers no account picker for an agent with no account of its own", async () => {
  const pending = render();
  await fillDirectory(pending);
  expect(accountPicker()).toBeUndefined();
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
  await choose("Same as console, Work");
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
    "Same as console, Work",
    "Default",
    "Work",
    "Personal",
  ]);
});

it("hides a turned-off account from the list", async () => {
  const pending = render(undefined, [work, { ...personal, disabled: true }]);
  await fillDirectory(pending);
  await click(accountPicker());
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual([
    "Same as console, Work",
    "Default",
    "Work",
  ]);
});

it("keeps the turned-off account a project still pins in the list, marked Disabled", async () => {
  render(stateOf({ kind: "account", id: "a-personal" }), [work, { ...personal, disabled: true }]);
  await click(accountPicker());
  const options = Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));
  const off = options.find((o) => o.textContent?.includes("Personal"))!;
  expect(off.textContent).toContain("Disabled");
  expect(off.getAttribute("aria-disabled")).not.toBe("true");
});

it.each([
  ["inheriting", undefined, "Same as console, Work"],
  ["inheriting, written out", { kind: "inherit" } as const, "Same as console, Work"],
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
  await choose("Same as console, Work");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).not.toHaveProperty("claude_account_id");
});

it.each([
  ["inheriting to the default account", undefined, "Default", { kind: "default" }],
  ["a named account to the default account", { kind: "account", id: "a-work" } as const, "Default", { kind: "default" }],
  ["the default account to a named account", { kind: "default" } as const, "Personal", { kind: "account", id: "a-personal" }],
  ["the default account back to inheriting", { kind: "default" } as const, "Same as console, Work", null],
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

it("follows the console's agent for Same as console", async () => {
  const pending = render(projectOf("p-2", "c-1", "Site"), [work, personal]);
  expect(accountPicker()).toBeDefined();
  await click(buttons().find((b) => b.textContent?.includes("Same as console, Claude Code")));
  await choose("Codex");
  expect(accountPicker()).toBeUndefined();
  expect(pending).toHaveLength(0);
});

it("names the console's project session agent on Same as console, not its console session agent", async () => {
  render(projectOf("p-2", "c-1", "Site"), [], { default_agent: "claude", project_session_agent: "grok" });
  expect(buttons().find((b) => b.textContent?.includes("Same as console, Grok Build"))).toBeDefined();
  expect(buttons().find((b) => b.textContent?.includes("Same as console, Claude Code"))).toBeUndefined();
});

it("names the console session agent on Same as console while the console has no project session agent of its own", () => {
  render(projectOf("p-2", "c-1", "Site"), [], { default_agent: "codex" });
  expect(buttons().find((b) => b.textContent?.includes("Same as console, Codex"))).toBeDefined();
});

it("leaves the agents known to be missing out of the default agent picker", async () => {
  render(projectOf("p-2", "c-1", "Site"), [], {}, { codex: "unavailable" });
  await click(buttons().find((b) => b.textContent?.includes("Same as console")));
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual([
    "Same as console, Claude Code",
    "Claude Code",
    "Grok Build",
  ]);
});

it("keeps the project's own missing agent in the picker as not installed until another is chosen", async () => {
  render(projectOf("p-2", "c-1", "Site", { default_agent: "codex" }), [], {}, { codex: "unavailable" });
  expect(buttons().some((b) => b.textContent?.includes("Codex, Not installed"))).toBe(true);
  await click(buttons().find((b) => b.textContent?.includes("Codex, Not installed")));
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual([
    "Same as console, Claude Code",
    "Claude Code",
    "Grok Build",
    "Codex, Not installed",
  ]);
  await choose("Claude Code");
  await click(buttons().find((b) => b.textContent?.includes("Claude Code")));
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).not.toContain("Codex, Not installed");
});

it("lists only the added agents in the picker, in the order they were added", async () => {
  render(undefined, [], {}, {}, ["grok", "claude"]);
  await click(buttons().find((b) => b.textContent?.includes("Same as console")));
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent)).toEqual([
    "Same as console, Claude Code",
    "Grok Build",
    "Claude Code",
  ]);
});

it("keeps the project's own agent that is not added in the picker, marked, until another is chosen", async () => {
  render(projectOf("p-2", "c-1", "Site", { default_agent: "codex" }), [], {}, {}, ["claude"]);
  const trigger = buttons().find((b) => b.textContent?.includes("Codex, Not added"));
  expect(trigger).toBeDefined();
  await click(trigger);
  expect(Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent).filter((text) => text?.includes("Codex"))).toEqual(["Codex, Not added"]);
});

it("offers no account and no launch settings while the source is a parent directory with Detect from files", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  await click(accountPicker());
  await choose("Personal");
  await chooseSource("A parent directory");
  expect(accountPicker()).toBeUndefined();
  // Advanced holds the git workflow settings alone.
  await click(buttonText("Advanced"));
  expect(pickerNamed("Model")).toBeUndefined();
  expect(pickerNamed("Where sessions develop")).toBeDefined();
  await click(buttonText("Add"));
  // What was set before is out of sight and not sent.
  expect(sentOf(pending, "add_project")).not.toHaveProperty("claude_account_id");
});

it("keeps Advanced open as the source moves between Detect from files and a single agent", async () => {
  render(undefined, [work, personal]);
  await click(buttonText("Advanced"));
  const open = () => buttonText("Advanced")?.getAttribute("aria-expanded");
  expect(open()).toBe("true");
  await chooseSource("A parent directory");
  expect(pickerNamed("Model")).toBeUndefined();
  expect(open()).toBe("true");
  await click(buttons().find((b) => b.textContent?.includes("A parent directory")));
  await choose("A single directory");
  expect(pickerNamed("Model")).toBeDefined();
  expect(open()).toBe("true");
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

const detectionHelp = () => document.querySelector<HTMLElement>('button[aria-label="About detecting the agent"]');

it("keeps what the detection leaves to the user described, and the mechanism in a help tip, for a single agent picker", async () => {
  render();
  await chooseSource("A parent directory");
  const picker = pickerNamed("Default agent");
  const described = picker.getAttribute("aria-describedby")?.split(" ").map((id) => document.getElementById(id));
  const hint = described?.find((el) => el?.textContent?.includes("applies to every repository"));
  expect(hint?.textContent).toBe("An agent chosen here applies to every repository.");
  expect(detectionHelp()).not.toBeNull();
  await click(detectionHelp() ?? undefined);
  const popover = document.querySelector('[role="dialog"][aria-label="About detecting the agent"]');
  expect(popover?.textContent).toContain("looks for");
  expect(Array.from(popover!.querySelectorAll("code")).map((el) => el.textContent)).toEqual([
    "CLAUDE.md", "AGENTS.md", ".claude", ".codex",
  ]);
  await act(async () => void popover!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await click(buttons().find((b) => b.textContent?.includes("A parent directory")));
  await choose("A single directory");
  expect(pickerNamed("Default agent")).toBe(picker);
  expect(picker.hasAttribute("aria-describedby")).toBe(false);
  expect(document.getElementById(hint!.id)).toBeNull();
  expect(detectionHelp()).toBeNull();
});

it("describes the joined agent and account group by what the parent-directory choice leaves to the user", async () => {
  render(undefined, [work, personal]);
  await chooseSource("A parent directory");
  await click(buttons().find((b) => b.textContent?.includes("Detect from files")));
  await choose("Claude Code");
  const described = group("Default agent").getAttribute("aria-describedby");
  expect(document.getElementById(described ?? "")?.textContent).toBe("An agent chosen here applies to every repository.");
  expect(detectionHelp()).not.toBeNull();
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
  expect(optionTexts()).toEqual(["Alpha", "Beta", "retired, Not in the list"]);
});

it("offers the stored model alone and the reason when the agent cannot list its models", async () => {
  const pending = render(projectOf("p-2", "c-1", "Site", { default_agent: "claude", claude_session_model: "retired" }));
  await click(buttonText("Advanced"));
  expect(document.body.textContent).toContain("Loading the models this agent offers");
  await answerModels(pending, { code: "models_timed_out", params: { agent: "claude" }, message: "" });
  // The reason is a mark inside the Model field, not text under it, and nothing describes the field by it.
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  const mark = pickerNamed("Model").closest("[data-slot=select]")!.querySelector<HTMLElement>("button[data-status-mark]")!;
  expect(mark.getAttribute("aria-label")).toBe("Could not load the models");
  expect(pickerNamed("Model").getAttribute("aria-describedby")).toBeNull();
  await settle(200);
  const status = document.body.querySelector('[role="status"]')?.textContent;
  press(mark);
  const sentence = document.querySelector("[data-status-mark-popover] p")!;
  expect(sentence.textContent).toBe("claude took too long to list its models.");
  const agent = sentence.querySelector("[data-inline-reference]")!;
  expect(agent.textContent).toBe("claude");
  expect(agent.getAttribute("dir")).toBe("auto");
  expect(agent.querySelector("code")).toBeNull();
  expect(agent.textContent).not.toMatch(/[\u2066-\u2069]/);
  expect(status).toBe(`${isolate("claude")} took too long to list its models.`);
  act(() => void document.querySelector("[data-status-mark-popover]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["retired, Not in the list"]);
});

it("names the console's project session value on the unset launch pickers, each value on its own", async () => {
  render(projectOf("p-2", "c-1", "Site"), [], { claude_project_session_model: "opus" });
  await click(buttonText("Advanced"));
  expect(pickerNamed("Model").textContent).toContain("Same as console, opus");
  expect(pickerNamed("Reasoning effort").textContent).toContain("Same as console, The agent's own setting");
});

it("lets the project's own value replace the inherited one and names the console's pair for the agent shown", async () => {
  render(
    projectOf("p-2", "c-1", "Site", { claude_session_effort: "high" }),
    [],
    { claude_project_session_effort: "low", codex_project_session_model: "gpt-x" },
  );
  await click(buttonText("Advanced"));
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
  expect(pickerNamed("Reasoning effort").textContent).not.toContain("Same as console");
  expect(pickerNamed("Model").textContent).toContain("Same as console, The agent's own setting");
});
