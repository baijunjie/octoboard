// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { request, settings } from "./agentAccountsSectionTestState";
import { mountAgentSettings } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const compatSwitch = (container: HTMLElement) => container.querySelector<HTMLInputElement>('input[role="switch"][aria-label^="Claude compatibility"]');

it.each(["Codex", "Grok Build"] as const)("shows a Claude compatibility row in the %s settings dialog, reading on when the daemon reports no value, with its help, and none in the tab", async (name) => {
  const { container, section } = await mountAgentSettings(name);
  const panel = container;
  const input = compatSwitch(container)!;
  expect(input.checked).toBe(true);
  expect(input.getAttribute("aria-label")).toBe(`Claude compatibility for \u2068${name}\u2069`);
  expect(panel.textContent).toContain(`Applies to every \u2068${name}\u2069 account`);
  expect(panel.querySelectorAll('button[aria-label="About Claude compatibility"]')).toHaveLength(1);
  expect(section.querySelector('[role="tabpanel"]')!.textContent).not.toContain("Claude compatibility");
});

it("puts the Codex Claude compatibility row after the Permission approval control", async () => {
  const { container } = await mountAgentSettings("Codex");
  const approval = container.querySelector('[aria-label^="Permission approval for"]')!;
  expect(approval).not.toBeNull();
  expect(approval.compareDocumentPosition(compatSwitch(container)!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("shows no Claude compatibility row for Claude Code", async () => {
  const { container } = await mountAgentSettings("Claude Code");
  expect(container.textContent).not.toContain("Claude compatibility");
});

it("shows the stored choice of the agent in view", async () => {
  settings.provider_settings = { codex: { claude_compatibility: false } };
  expect(compatSwitch((await mountAgentSettings("Codex")).container)!.checked).toBe(false);
});

it("saves the agent's own value when toggled", async () => {
  const { container } = await mountAgentSettings("Grok Build");
  await act(async () => compatSwitch(container)!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent: "grok", setting: "claude_compatibility", value: false });
});

it("keeps the switch unavailable while saving, then reports a failed save", async () => {
  let fail!: (error: Error) => void;
  const { container } = await mountAgentSettings("Codex");
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
