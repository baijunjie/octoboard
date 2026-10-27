// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Daemon } from "../store";
import { parentConsole, renderDialog, hub1, buttons, optionElements, pickerNamed } from "./sessionDialogTestSupport";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

function answerSessionModels(daemon: Daemon) {
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_agent_models"
    ? { type: "agent_models", agent: body.agent, account: body.account ?? null, models: [
      { id: "opus", name: "Opus", efforts: ["low", "high"] },
      { id: "sonnet", name: "Sonnet", efforts: ["low", "high"] },
    ], unavailable: null }
    : { type: "ack" });
}

it.each(["inherit", "pick", "clear"] as const)("opens with session launch settings: %s", async (mode) => {
  const { container, root, daemon } = renderDialog([], {}, {}, { kind: "unbound" }, {
    claude_session_model: "opus", claude_session_effort: "high",
  });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "list_agent_models")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
  if (mode === "pick") {
    await act(async () => pickerNamed("Model").click());
    await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
    await act(async () => pickerNamed("Reasoning effort").click());
    await act(async () => optionElements().find((option) => option.textContent === "low")!.click());
  } else if (mode === "clear") {
    await act(async () => pickerNamed("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  }
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  if (mode === "inherit") {
    expect(sent).not.toHaveProperty("model");
    expect(sent).not.toHaveProperty("effort");
  } else if (mode === "pick") {
    expect(sent).toMatchObject({ model: "sonnet", effort: "low" });
  } else {
    expect(sent).toMatchObject({ model: null });
    expect(sent).not.toHaveProperty("effort");
  }
});

it("queries the selected account and keeps overrides separate when changing agents", async () => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" },
    ] },
  }, { claude_account_id: "a-work" }, { kind: "unbound" }, { codex_session_model: "codex-project" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "claude", account: "a-work" });
  await act(async () => pickerNamed("Model").click());
  await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
  await act(async () => pickerNamed("Claude Code account").click());
  expect(optionElements().map((option) => option.textContent)).toEqual(["Default", "Work"]);
  await act(async () => optionElements().find((option) => option.textContent === "Default")!.click());
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "claude" });
  const agentTrigger = pickerNamed("Agent");
  await act(async () => { agentTrigger.focus(); agentTrigger.click(); });
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "codex")!.click());
  await act(async () => {
    await vi.waitFor(() => expect(document.activeElement).toBe(agentTrigger));
  });
  expect(pickerNamed("Agent")).toBe(agentTrigger);
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "codex" });
  expect(pickerNamed("Model").textContent).toContain("codex-project");
  expect(pickerNamed("Codex account").textContent).toContain("Default");
  await act(async () => pickerNamed("Agent").click());
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "claude")!.click());
  expect(pickerNamed("Model").textContent).toContain("Sonnet");
  expect(pickerNamed("Claude Code account").textContent).toContain("Default");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  expect(sent).toMatchObject({ agent: "claude", account: null, model: "sonnet" });
});

it("keeps the selected agent and the account picker when the last named account is removed", async () => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" },
    ] },
  }, { default_agent: "codex" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  await act(async () => pickerNamed("Agent").click());
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "claude")!.click());
  await act(async () => pickerNamed("Claude Code account").click());
  await act(async () => optionElements().find((option) => option.textContent === "Work")!.click());
  const agentTrigger = pickerNamed("Agent");
  act(() => daemon.store.setState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] } }));
  expect(pickerNamed("Agent")).toBe(agentTrigger);
  expect(agentTrigger.textContent).toContain("Claude Code");
  expect(pickerNamed("Claude Code account").textContent).toContain("Default");
});

it.each(["inherit", "pick", "clear"] as const)("opens console sessions with console launch settings: %s", async (mode) => {
  const { container, root, daemon } = renderDialog([hub1], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [
      { id: "a-console", agent: "claude", name: "Console account", config_dir: "/accounts/console" },
      { id: "a-codex", agent: "codex", name: "Codex work", config_dir: "/accounts/codex" },
    ] },
  }, {
    claude_account_id: "a-console", claude_console_session_model: "opus", claude_console_session_effort: "high",
    codex_account_id: "a-codex", codex_console_session_model: "codex-console", codex_console_session_effort: "medium",
  }, { kind: "choose" }, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe("Open console session");
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain("The session will be opened in the console Main.");
  expect(document.body.textContent).not.toContain("Report to");
  const name = document.body.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  expect(name.placeholder).toBe(`${parentConsole.name} session`);
  expect(pickerNamed("Agent").textContent).toContain("Claude Code");
  expect(pickerNamed("Claude Code account").textContent).toContain("Console account");
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "list_agent_models")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
  expect(document.body.textContent).toContain("Unchanged values follow the console settings.");
  if (mode === "pick") {
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(name, "Release coordination");
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => pickerNamed("Agent").click());
    await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "codex")!.click());
    expect(pickerNamed("Codex account").textContent).toContain("Codex work");
    expect(pickerNamed("Model").textContent).toContain("codex-console");
    expect(pickerNamed("Reasoning effort").textContent).toContain("medium");
    await act(async () => pickerNamed("Codex account").click());
    await act(async () => optionElements().find((option) => option.textContent === "Default")!.click());
    await act(async () => pickerNamed("Model").click());
    await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
    await act(async () => pickerNamed("Reasoning effort").click());
    await act(async () => optionElements().find((option) => option.textContent === "low")!.click());
  } else if (mode === "clear") {
    await act(async () => pickerNamed("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
    await act(async () => pickerNamed("Reasoning effort").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  }
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "open_session")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  expect(sent).not.toHaveProperty("project_id");
  expect(sent).not.toHaveProperty("bound_to");
  if (mode === "inherit") {
    expect(sent).toMatchObject({ agent: "claude", account: "a-console" });
    expect(sent).not.toHaveProperty("model");
    expect(sent).not.toHaveProperty("effort");
    expect(sent).toHaveProperty("title", "Main session");
  } else if (mode === "pick") {
    expect(sent).toMatchObject({ title: "Release coordination", agent: "codex", account: null, model: "sonnet", effort: "low" });
  } else {
    expect(sent).toMatchObject({ model: null, effort: null });
  }
});

it("opens console sessions on the console's default agent and its account", async () => {
  const { container, root, daemon } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [
      { id: "a-codex", agent: "codex", name: "Work", config_dir: "/accounts/codex" },
    ] },
  }, { default_agent: "codex", codex_account_id: "a-codex" }, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" });
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  expect(pickerNamed("Codex account").textContent).toContain("Work");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ agent: "codex", account: "a-codex" }));
});

it("uses the localized default name as its placeholder and submitted title", async () => {
  const { container, root, daemon } = renderDialog([], {}, {}, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const name = document.body.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  expect(name.placeholder).toBe(`${parentConsole.name} session`);
  expect(name.value).toBe("");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", title: "Main session" }));
});

it.each(["Backspace", "Delete"])("clears a launch choice with %s only while its list is closed", async (key) => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {}, {}, undefined, { claude_session_model: "opus" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  const trigger = pickerNamed("Model");
  await act(async () => { trigger.focus(); trigger.click(); });
  await act(async () => { await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("option")); });
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
  expect(trigger.textContent).toContain("Opus");
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => { await vi.waitFor(() => expect(document.activeElement).toBe(trigger)); });
  await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
  expect(trigger.textContent).toContain("The agent's own setting");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", model: null }));
});

it.each([
  ["the project's own value over the console's project session one", { claude_session_model: "opus" }, { claude_project_session_model: "sonnet" }, "Opus", "The agent's own setting"],
  ["the console's project session values where the project sets none", {}, { claude_project_session_model: "sonnet", claude_project_session_effort: "low" }, "Sonnet", "low"],
  ["each value from the project, else from the console", { claude_session_effort: "high" }, { claude_project_session_model: "sonnet", claude_project_session_effort: "low" }, "Sonnet", "high"],
] as const)("opens a project session on %s", async (_, projectValues, consoleValues, model, effort) => {
  const { container, root, daemon } = renderDialog([], {}, consoleValues, { kind: "unbound" }, projectValues);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain(model);
  expect(pickerNamed("Reasoning effort").textContent).toContain(effort);
});

it("opens a console session on the console session value, not the project session one", async () => {
  const { container, root, daemon } = renderDialog([], {}, { claude_console_session_model: "opus", claude_project_session_model: "sonnet" }, { kind: "unbound" }, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
});

it("keeps the Name hint visible", () => {
  const { container, root } = renderDialog([], {}, {}, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  expect(document.body.textContent).toContain("Leave this empty to use the default session name.");
});

it("lists an account whose sharing change runs as unavailable, without a reason, in the account picker", async () => {
  const { container, root } = renderDialog([], {
    settings: {
      auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW,
      accounts: [{ id: "a-1", agent: "claude", name: "Work", config_dir: "/home/me/.claude-work" }],
    },
    accountSharing: new Map([["a-1", { account: "a-1", progress: { share: true, phase: "changing" as const, done: 0, total: 3 } }]]),
  });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await act(async () => pickerNamed("Claude Code account").click());
  const options = [...document.body.querySelectorAll<HTMLElement>('[role="option"]')];
  const work = options.find((option) => option.textContent?.includes("Work"))!;
  // No reason line is drawn, but the option is still announced as unavailable.
  expect(work.textContent).toBe("Work");
  expect(work.getAttribute("aria-describedby")).toBeNull();
  expect(work.getAttribute("aria-disabled")).toBe("true");
  // It cannot be picked: pressing it leaves the account as it was.
  await act(async () => work.click());
  expect(pickerNamed("Claude Code account").textContent).not.toContain("Work");
});

it("shows the account picker with the agent's only account, its usage drawn in the option", () => {
  const { container, root } = renderDialog([], {
    accountQuotas: [{
      agent: "claude", account: null, fetched_at: Date.now(), plan: null, unavailable: null,
      windows: [{ kind: "session", label: null, used_percent: 30, resets_at: Date.now() + 3_600_000, window_minutes: 300 }],
    }],
  });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const account = pickerNamed("Claude Code account");
  expect(account).toBeDefined();
  expect(pickerNamed("Agent").getAttribute("aria-describedby")).toBeNull();
  // The only bar is the one in the account picker's own trigger.
  expect(document.body.querySelectorAll('[class*="quota-meter--"]')).toHaveLength(1);
});
