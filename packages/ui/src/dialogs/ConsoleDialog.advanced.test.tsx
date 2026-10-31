// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import type { Agent, AgentAvailability } from "../protocol";
import { presence, stubListMotion } from "../components/listMotionTestSupport";
import { listing, lastStore, render, buttons, click, clickButton, choose, sent, type, openAdvanced, clickTab, nameOf, groupPicker, picker, tabNames } from "./consoleDialogTestSupport";

const selectedTab = () => document.body.querySelector('[role="tab"][aria-selected="true"]')?.textContent;

it("opens the Advanced section on the console session agent, with a tab for each installed agent", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  await openAdvanced();
  expect(tabNames()).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(selectedTab()).toBe("Codex");
});

it("holds the console session and project session values apart for the agent shown", async () => {
  render(consoleOf("c-1", "Main", { launch: { claude: { model: "alpha" } }, project_session_launch: { claude: { model: "beta" } } }));
  await openAdvanced();
  expect(groupPicker("Console sessions", "Model").textContent).toContain("Alpha");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("Beta");
});

it("keeps the edits of every agent while switching tabs, and saves only what changed", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_launch: { codex: { model: "kept" } } }));
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model"));
  await choose("Alpha");
  await clickTab("Codex");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("kept");
  await click(groupPicker("Console sessions", "Reasoning effort"));
  await choose("high");
  await clickTab("Claude Code");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("Alpha");
  await clickButton("Save");
  expect(sent(request)).toEqual([
    {
      type: "update_console",
      console: "c-1",
      name: "Main",
      default_agent: "claude",
      project_session_launch: { claude: { model: "alpha" } }, launch: { codex: { effort: "high" } },
    },
  ]);
});

it("creates with the project session pair that was set", async () => {
  const request = render();
  type("Name", "Main");
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model"));
  await choose("Beta");
  await click(groupPicker("Project sessions", "Reasoning effort"));
  await choose("max");
  await clickButton("Create");
  expect(sent(request)).toEqual([
    { type: "create_console", name: "Main", default_agent: "claude", project_session_launch: { claude: { model: "beta", effort: "max" } } },
  ]);
});

it("sends an explicit null when a project session value is cleared", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_launch: { claude: { model: "alpha" } } }));
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model").parentElement!.querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", project_session_launch: { claude: { model: null } } }]);
});

it("leaves a missing agent without a tab unless the console stores a value for it, and drops it once cleared", async () => {
  render(consoleOf("c-1", "Main", { project_session_launch: { codex: { model: "kept" } } }), [], undefined, { codex: "unavailable", grok: "unavailable" });
  await openAdvanced();
  expect(tabNames()).toEqual(["Claude Code", "Codex, Not installed"]);
  await clickTab("Codex, Not installed");
  await click(groupPicker("Project sessions", "Model").parentElement!.querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  expect(tabNames()).toEqual(["Claude Code"]);
  expect(selectedTab()).toBe("Claude Code");
});

it("follows the console session agent in the Advanced section until a tab is picked", async () => {
  render();
  await click(picker("Console sessions"));
  await choose("Codex");
  await openAdvanced();
  expect(selectedTab()).toBe("Codex");
  await click(picker("Console sessions"));
  await choose("Grok Build");
  expect(selectedTab()).toBe("Grok Build");
  await clickTab("Claude Code");
  await click(picker("Console sessions"));
  await choose("Codex");
  expect(selectedTab()).toBe("Claude Code");
});

it("does not take the view back to an agent picked earlier that comes back", async () => {
  render(consoleOf("c-1", "Main", { project_session_launch: { codex: { model: "kept" } } }), [], undefined, { codex: "unavailable" });
  await openAdvanced();
  await clickTab("Codex, Not installed");
  await click(groupPicker("Project sessions", "Model").parentElement!.querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  expect(selectedTab()).toBe("Claude Code");
  await act(async () => lastStore.setState({ agentAvailability: new Map([["claude", { agent: "claude", availability: "available", default_account_dir: null }], ["codex", { agent: "codex", availability: "available", default_account_dir: null }], ["grok", { agent: "grok", availability: "available", default_account_dir: null }]]) }));
  expect(tabNames()).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(selectedTab()).toBe("Claude Code");
});

it("names each Advanced picker by its sessions and its setting, and describes it by their note", async () => {
  render(undefined, [], async (body) => listing(body, [], { code: "models_timed_out", params: { agent: "Claude Code" }, message: "" }));
  await openAdvanced();
  const pickers = [["Console sessions", "Model"], ["Console sessions", "Reasoning effort"], ["Project sessions", "Model"], ["Project sessions", "Reasoning effort"]];
  const names = pickers.map(([group, name]) => nameOf(groupPicker(group, name)).join(" "));
  expect(names).toEqual(["Console sessions Model", "Console sessions Reasoning effort", "Project sessions Model", "Project sessions Reasoning effort"]);
  expect(new Set(names).size).toBe(4);
  // Named only by the headers: an own aria-label beside them would be read as part of the name.
  for (const [group, name] of pickers) expect(groupPicker(group, name).getAttribute("aria-label")).toBeNull();
  const describedBy = (group: string, name: string) =>
    (groupPicker(group, name).getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean).map((id) => document.getElementById(id)?.textContent ?? "");
  expect(describedBy("Console sessions", "Reasoning effort")).toEqual([]);
  expect(describedBy("Project sessions", "Reasoning effort")).toEqual(["For project sessions, settings on the project or session take precedence."]);
  // The model list's state is a mark inside each Model field, not an Alert, so it describes neither picker; the
  // project's Model picker is described by the note on its sessions alone.
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(describedBy("Console sessions", "Model")).toEqual([]);
  expect(describedBy("Project sessions", "Model")).toEqual(["For project sessions, settings on the project or session take precedence."]);
  // The headers that name the pickers are hidden from assistive technology, so they are not read a
  // second time beside the names, and no visible text of the table is outside them.
  for (const [group, name] of pickers) {
    for (const id of groupPicker(group, name).getAttribute("aria-labelledby")!.split(" ")) {
      const el = document.getElementById(id)!;
      if (el.textContent === group || el.textContent === name) expect(el.closest('[aria-hidden="true"]')).not.toBeNull();
    }
  }
  const headers = Array.from(document.body.querySelectorAll("div, span")).filter((el) => ["Console sessions", "Project sessions", "Reasoning effort"].includes(el.textContent ?? "") && el.children.length === 0 && !el.closest("button, label, [role=tab]"));
  expect(headers.length).toBeGreaterThan(0);
  expect(headers.every((el) => el.closest('[aria-hidden="true"]'))).toBe(true);
});

it("keeps the default agent, accounts and session instructions notes visible, and no per-agent models line", async () => {
  render();
  await openAdvanced();
  const text = document.body.textContent ?? "";
  expect(text).toContain("A project with its own default agent uses that instead.");
  expect(text).toContain("Every session of an agent in this console uses its account here");
  expect(text).toContain("take precedence over them where the two conflict.");
  expect(text).not.toContain("Each agent has its own models");
  expect(buttons().some((b) => b.getAttribute("aria-label")?.startsWith("About"))).toBe(false);
});

it("moves the Advanced tabs that come and go, none as the section opens, and hands focus on from a tab that goes", async () => {
  const motion = stubListMotion();
  render(undefined, [], undefined, { claude: "available", codex: "available", grok: "available" });
  await openAdvanced();
  expect(motion.played()).toEqual([]);
  const grok = Array.from(document.body.querySelectorAll<HTMLElement>('[role="tab"]')).find((tab) => tab.textContent === "Grok Build")!;
  await clickTab("Grok Build");
  await act(async () => grok.focus());
  expect(selectedTab()).toBe("Grok Build");
  const availability = (grokState: AgentAvailability["availability"]) => new Map((["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: agent === "grok" ? grokState : "available", default_account_dir: null }]));
  await act(async () => lastStore.setState({ agentAvailability: availability("unavailable") }));
  expect(motion.played().map(({ frames }) => presence(frames))).toEqual(["exit"]);
  const copy = document.body.querySelector("[data-list-motion-exit]")!;
  expect(copy.getAttribute("aria-hidden")).toBe("true");
  expect(copy.hasAttribute("inert")).toBe(true);
  expect(document.activeElement?.textContent).toBe("Claude Code");
  expect(copy.contains(document.activeElement)).toBe(false);
  motion.clear();
  await act(async () => lastStore.setState({ agentAvailability: availability("available") }));
  expect(motion.played().map(({ element, frames }) => [element.textContent, presence(frames)])).toEqual([["Grok Build", "enter"]]);
});
