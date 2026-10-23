// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import type { Account } from "../protocol";
import { trackMountRefocus } from "./dialogTestSupport";
import { work, lastStore, render, buttons, click, clickButton, choose, sent, asked, type, openAdvanced, clickTab, groupPicker, optionTexts, picker, tabNames } from "./consoleDialogTestSupport";

const codexWork: Account = { id: "a-codex", agent: "codex", name: "Codex work", config_dir: "/home/dev/.codex-work" };

it("names the agent project sessions follow on the Same as entry, and keeps it current", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  expect(picker("Project sessions").textContent).toContain("Same as console sessions, Codex");
  await click(picker("Console sessions"));
  await choose("Grok Build");
  expect(picker("Project sessions").textContent).toContain("Same as console sessions, Grok Build");
});

it("starts a new console on Same as console sessions and sends no project session agent", async () => {
  const request = render();
  type("Name", "Main");
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude" }]);
});

it("creates with the project session agent that was picked", async () => {
  const request = render();
  type("Name", "Main");
  await click(picker("Project sessions"));
  await choose("Codex");
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude", project_session_agent: "codex" }]);
});

it("opens a console stored without a project session agent on Same as and sends nothing for it", async () => {
  const request = render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  expect(picker("Project sessions").textContent).toContain("Same as console sessions");
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", default_agent: "codex" }]);
});

it("sends the project session agent only when it changed, and an explicit null when it goes back to Same as", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_agent: "grok" }));
  expect(picker("Project sessions").textContent).toContain("Grok Build");
  await clickButton("Save");
  expect(sent(request)[0]).toEqual({ type: "update_console", console: "c-1", name: "Main", default_agent: "claude" });
  await click(picker("Project sessions"));
  await choose("Same as console sessions, Claude Code");
  await clickButton("Save");
  expect(sent(request)[1]).toEqual({ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", project_session_agent: null });
});

it("lets the console session agent change without moving the project session agent", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_agent: "grok" }));
  await click(picker("Console sessions"));
  await choose("Codex");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "codex" }]);
});

it("follows the console session agent in the Advanced section, not the project session agent", async () => {
  const request = render(consoleOf("c-1", "Main", { default_agent: "codex", project_session_agent: "grok" }));
  await openAdvanced();
  expect(asked(request)).toEqual([{ type: "list_agent_models", agent: "codex" }]);
});

it("shows one row per agent in the accounts table, each with its own account", async () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork]);
  expect(picker("Claude Code account").textContent).toContain("Default");
  expect(picker("Codex account").textContent).toContain("Codex work");
  // Grok has only its default account, as the one option of its picker; the one way to Settings is for the whole
  // section, beside its title.
  expect(picker("Grok Build account").textContent).toContain("Default account");
  // An icon-only button, named by its label.
  expect(buttons().filter((b) => b.getAttribute("aria-label") === "Manage accounts" && b.textContent === "")).toHaveLength(1);
  expect(document.body.textContent).toContain("Agent default accounts");
  expect(document.body.textContent).not.toContain("Default account:");
});

it("names the default account by the name the user gave it where it is the only account", () => {
  render(consoleOf("c-1", "Main"), [work, codexWork]);
  act(() => lastStore.setState((state) => ({ settings: { ...state.settings, default_account_names: { grok: "Personal" } } })));
  expect(picker("Grok Build account").textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("Personal");
  expect(document.body.textContent).not.toContain("Default account:");
});

it("sends an account chosen for an agent that is not the console session agent", async () => {
  const request = render(consoleOf("c-1", "Main"), [work, codexWork]);
  await click(picker("Codex account"));
  await choose("Codex work");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", codex_account_id: "a-codex" }]);
});

it("opens Settings at the agent accounts from the section's Manage accounts button", async () => {
  // The Settings dialog sizes itself by a breakpoint the stylesheet defines.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: true, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  render();
  await clickButton((b) => b.getAttribute("aria-label") === "Manage accounts");
  const settings = document.body.querySelector('[aria-label="Settings"]');
  expect(settings).not.toBeNull();
  expect(settings?.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toBe("Agent accounts");
});

it("keeps the console dialog open and unsaved while an account is added in the Settings over it", async () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: true, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  const request = render(consoleOf("c-1", "Main"));
  await clickButton((b) => b.getAttribute("aria-label") === "Manage accounts");
  await clickButton("Add account");
  const fill = (label: string, value: string) => {
    const input = Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).filter((el) => el.labels?.[0]?.textContent === label).at(-1)!;
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };
  fill("Name", "Work");
  fill("Config directory", "/home/dev/.claude-work");
  await clickButton("Add");
  expect(sent(request)).toEqual([{ type: "create_account", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" }]);
  expect(document.body.textContent).toContain("Console settings");
});

it("restores Manage accounts after virtual activation followed by a pointer backdrop dismissal", async () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: true, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  render();
  await trackMountRefocus()();
  const opener = buttons().find((button) => button.getAttribute("aria-label") === "Manage accounts")!;
  const name = Array.from(document.querySelectorAll<HTMLInputElement>("input")).find((input) => input.labels?.[0]?.textContent === "Name")!;
  act(() => name.focus());
  expect(document.activeElement).toBe(name);
  // An accessibility click activates the button without a pointer-down focus transfer.
  await click(opener);
  const settings = document.querySelector<HTMLElement>('[role="dialog"][aria-label="Settings"]')!;
  expect(settings).not.toBeNull();
  await trackMountRefocus()();
  expect(settings.contains(document.activeElement)).toBe(true);
  const backdrop = settings.closest<HTMLElement>('[data-slot="modal-backdrop"]')!;
  act(() => {
    backdrop.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerType: "mouse" }));
    backdrop.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    // jsdom does not perform WebKit's native blur when pressing a nonfocusable backdrop.
    (document.activeElement as HTMLElement).blur();
    backdrop.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" }));
    backdrop.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, button: 0 }));
    backdrop.click();
  });
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await new Promise((resolve) => requestAnimationFrame(resolve));
  });
  expect(document.querySelector('[role="dialog"][aria-label="Settings"]')).toBeNull();
  expect(document.activeElement).toBe(opener);
});

it("lists only the agents that may be picked, and the accounts of those", () => {
  render(undefined, [work, codexWork], undefined, { codex: "unavailable" });
  expect(picker("Console sessions")).toBeDefined();
  expect(picker("Claude Code account")).toBeDefined();
  expect(buttons().some((b) => b.getAttribute("aria-label") === "Codex account")).toBe(false);
  expect(document.body.textContent).not.toContain("Codex, Not installed");
});

it("leaves the missing agents out of both default agent pickers", async () => {
  render(undefined, [], undefined, { codex: "unavailable" });
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Grok Build"]);
  await choose("Claude Code");
  await click(picker("Project sessions"));
  expect(optionTexts()).toEqual(["Same as console sessions, Claude Code", "Claude Code", "Grok Build"]);
});

it("keeps the stored agent in its picker as not installed while it is the value, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }), [], undefined, { codex: "unavailable" });
  expect(picker("Console sessions").textContent).toContain("Codex, Not installed");
  await click(picker("Console sessions"));
  const codex = Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === "Codex, Not installed");
  expect(codex?.getAttribute("aria-selected")).toBe("true");
  await choose("Claude Code");
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Grok Build"]);
});

it("keeps a missing agent's row while the console pins a named account for it, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork], undefined, { codex: "unavailable" });
  expect(picker("Codex account").textContent).toContain("Codex work");
  expect(document.body.textContent).toContain("Codex, Not installed");
  await click(picker("Codex account"));
  await choose("Default");
  expect(document.body.textContent).not.toContain("Codex, Not installed");
});

it("lists only the added agents, in the order they were added, in the agent pickers, the account rows and the Advanced tabs", async () => {
  render(undefined, [work, codexWork], undefined, {}, ["grok", "claude"]);
  expect(buttons().some((b) => b.getAttribute("aria-label") === "Codex account")).toBe(false);
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Grok Build", "Claude Code"]);
  await choose("Grok Build");
  await click(picker("Project sessions"));
  expect(optionTexts()).toEqual(["Same as console sessions, Grok Build", "Grok Build", "Claude Code"]);
  await choose("Claude Code");
  await openAdvanced();
  expect(tabNames()).toEqual(["Grok Build", "Claude Code"]);
  expect(buttons().filter((b) => /^(Grok Build|Claude Code) account$/.test(b.getAttribute("aria-label") ?? "")).map((b) => b.getAttribute("aria-label"))).toEqual(["Grok Build account", "Claude Code account"]);
});

it("opens a new console on the first added agent that may be picked when Claude Code is not added", () => {
  render(undefined, [], undefined, {}, ["codex"]);
  expect(picker("Console sessions").textContent).toContain("Codex");
});

it("keeps the stored agent in its picker as not added while it is the value, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }), [], undefined, {}, ["claude"]);
  expect(picker("Console sessions").textContent).toContain("Codex, Not added");
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Codex, Not added"]);
  await choose("Claude Code");
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code"]);
});

it("keeps the row of an agent that is not added while the console pins a named account for it, marked, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork], undefined, {}, ["claude"]);
  expect(picker("Codex account").textContent).toContain("Codex work");
  expect(document.body.textContent).toContain("Codex, Not added");
  await click(picker("Codex account"));
  await choose("Default");
  expect(document.body.textContent).not.toContain("Codex, Not added");
});

it("shows the badge after the name of a row that may not be picked, with the row dimmed, and none on the others", () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork], undefined, {}, ["claude"]);
  const names = Array.from(document.body.querySelectorAll<HTMLElement>('[aria-hidden="true"].text-sm')).filter((el) => /^(Claude Code|Codex)/.test(el.textContent ?? ""));
  const codex = names.find((el) => el.textContent?.startsWith("Codex"))!;
  const claude = names.find((el) => el.textContent?.startsWith("Claude Code"))!;
  expect(codex.querySelector("[data-variant=bordered]")?.textContent).toBe("Not added");
  expect(codex.querySelector("span.text-app-muted")?.textContent).toBe("Codex");
  expect(claude.querySelector("[data-variant=bordered]")).toBeNull();
  expect(claude.querySelector("span.text-foreground")?.textContent).toBe("Claude Code");
});

it("keeps the Advanced tab of an agent that is not added while the console stores a value for it, marked, and drops it once cleared", async () => {
  render(consoleOf("c-1", "Main", { codex_project_session_model: "kept" }), [], undefined, {}, ["claude"]);
  await openAdvanced();
  expect(tabNames()).toEqual(["Claude Code", "Codex, Not added"]);
  await clickTab("Codex, Not added");
  await click(groupPicker("Project sessions", "Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  expect(tabNames()).toEqual(["Claude Code"]);
});

it("lists every agent before availability has been determined", async () => {
  render();
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Codex", "Grok Build"]);
});
