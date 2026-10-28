// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { request, settings } from "./agentAccountsSectionTestState";
import { mount } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const compatSwitch = (container: HTMLElement) => container.querySelector<HTMLInputElement>('[role="tabpanel"] input[role="switch"][aria-label^="Claude compatibility"]');

it.each(["Codex", "Grok Build"] as const)("shows a Claude compatibility row above the %s table, reading on when the daemon reports no value, with its help", (name) => {
  const { container } = mount(name);
  const panel = container.querySelector('[role="tabpanel"]')!;
  const input = compatSwitch(container)!;
  expect(input.checked).toBe(true);
  expect(input.getAttribute("aria-label")).toBe(`Claude compatibility for ${name}`);
  expect(panel.textContent).toContain(`Applies to every ${name} account`);
  expect(panel.querySelectorAll('button[aria-label="About Claude compatibility"]')).toHaveLength(1);
  expect(input.compareDocumentPosition(panel.querySelector("table")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("puts the Codex Claude compatibility row after the Permission approval control", () => {
  const { container } = mount("Codex");
  const approval = container.querySelector('[role="tabpanel"] [aria-label="Permission approval for Codex"]')!;
  expect(approval).not.toBeNull();
  expect(approval.compareDocumentPosition(compatSwitch(container)!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("shows no Claude compatibility row for Claude Code", () => {
  const { container } = mount("Claude Code");
  expect(container.textContent).not.toContain("Claude compatibility");
});

it("shows the stored choice of the agent in view", () => {
  settings.claude_compatibility = { codex: false };
  expect(compatSwitch(mount("Codex").container)!.checked).toBe(false);
  expect(compatSwitch(mount("Grok Build").container)!.checked).toBe(true);
});

it("saves the agent's own value when toggled", async () => {
  const { container } = mount("Grok Build");
  await act(async () => compatSwitch(container)!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", claude_compatibility: { grok: false } });
});

it("keeps the switch unavailable while saving, then reports a failed save", async () => {
  let fail!: (error: Error) => void;
  const { container } = mount("Codex");
  request.mockImplementationOnce(() => new Promise((_, reject) => { fail = reject; }));
  await act(async () => compatSwitch(container)!.click());
  expect(compatSwitch(container)!.getAttribute("aria-disabled")).toBe("true");
  await act(async () => compatSwitch(container)!.click());
  expect(request).toHaveBeenCalledTimes(1);
  await act(async () => fail(new Error("save refused")));
  expect(compatSwitch(container)!.hasAttribute("aria-disabled")).toBe(false);
  expect(compatSwitch(container)!.checked).toBe(true);
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not save Claude compatibility"]);
});
