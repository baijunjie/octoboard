// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { request, settings } from "./agentAccountsSectionTestState";
import { mountAgentSettings } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** The compatibility rows each agent has: the label of the setting's source and the agent's wire id. */
const CASES = [
  { name: "Codex", agent: "codex", setting: "claude_compatibility", label: "Claude compatibility" },
  { name: "Grok Build", agent: "grok", setting: "claude_compatibility", label: "Claude compatibility" },
  { name: "Grok Build", agent: "grok", setting: "codex_compatibility", label: "Codex compatibility" },
  { name: "Claude Code", agent: "claude", setting: "codex_compatibility", label: "Codex compatibility" },
] as const;

const compatSwitch = (container: HTMLElement, label: string) => container.querySelector<HTMLInputElement>(`input[role="switch"][aria-label^="${label}"]`);

describe.each(CASES)("the $label row of $name ($setting)", ({ name, agent, setting, label }) => {
  it("reads off when the daemon reports no value, with its help, and is not in the tab", async () => {
    const { container, section } = await mountAgentSettings(name);
    const input = compatSwitch(container, label)!;
    expect(input.checked).toBe(false);
    expect(input.getAttribute("aria-label")).toBe(`${label} for \u2068${name}\u2069`);
    expect(container.textContent).toContain(`Applies to every \u2068${name}\u2069 account`);
    expect(container.querySelectorAll(`button[aria-label="About ${label}"]`)).toHaveLength(1);
    expect(section.querySelector('[role="tabpanel"]')!.textContent).not.toContain(label);
  });

  it("shows the stored choice of the agent in view", async () => {
    settings.provider_settings = { [agent]: { [setting]: true } };
    expect(compatSwitch((await mountAgentSettings(name)).container, label)!.checked).toBe(true);
  });

  it("saves the agent's own value when toggled", async () => {
    const { container } = await mountAgentSettings(name);
    await act(async () => compatSwitch(container, label)!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent, setting, value: true });
  });

  it("keeps the switch unavailable while saving, then reports a failed save", async () => {
    let fail!: (error: Error) => void;
    const { container } = await mountAgentSettings(name);
    request.mockImplementationOnce(() => new Promise((_, reject) => { fail = reject; }));
    await act(async () => compatSwitch(container, label)!.click());
    expect(compatSwitch(container, label)!.getAttribute("aria-disabled")).toBe("true");
    await act(async () => compatSwitch(container, label)!.click());
    expect(request).toHaveBeenCalledTimes(1);
    await act(async () => fail(new Error("save refused")));
    expect(compatSwitch(container, label)!.hasAttribute("aria-disabled")).toBe(false);
    expect(compatSwitch(container, label)!.checked).toBe(false);
    expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual([`Could not save ${label}`]);
  });
});

it("shows Grok Build both rows, Codex only Claude compatibility and Claude Code only Codex compatibility", async () => {
  const shown = async (name: string) => {
    const { container } = await mountAgentSettings(name);
    return (["Claude compatibility", "Codex compatibility"] as const).filter((label) => compatSwitch(container, label) !== null);
  };
  expect(await shown("Grok Build")).toEqual(["Claude compatibility", "Codex compatibility"]);
  expect(await shown("Codex")).toEqual(["Claude compatibility"]);
  expect(await shown("Claude Code")).toEqual(["Codex compatibility"]);
});

it("puts the Codex Claude compatibility row after the Permission approval control", async () => {
  const { container } = await mountAgentSettings("Codex");
  const approval = container.querySelector('[aria-label^="Permission approval for"]')!;
  expect(approval).not.toBeNull();
  expect(approval.compareDocumentPosition(compatSwitch(container, "Claude compatibility")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
