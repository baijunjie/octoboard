// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { request, settings } from "./agentAccountsSectionTestState";
import { mount } from "./agentAccountsSectionTestSupport";
import { TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../../test/testProviders";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const tabNames = (container: HTMLElement) => [...container.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
const modeControl = (container: HTMLElement) => container.querySelector('[role="tabpanel"] [aria-label^="Mode for"]');

it("words the test-only provider's setting from its presentation module", () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container } = mount("Test Agent");
  expect(tabNames(container)).toEqual(["Claude Code", "Test Agent"]);
  expect(modeControl(container)?.getAttribute("aria-label")).toBe("Mode for \u2068Test Agent\u2069");
  expect(container.querySelector('[role="tabpanel"]')!.textContent).toContain("Test mode");
});

it("shows a served provider without a presentation module under its served label, with none of its settings", () => {
  settings.added_agents = ["claude", "unpresented_agent"];
  const { container, set } = mount();
  set({ providers: [...TEST_PROVIDERS, UNPRESENTED_PROVIDER] });
  expect(tabNames(container)).toEqual(["Claude Code", "Unpresented Agent"]);
  const tab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === "Unpresented Agent")!;
  expect(tab).toBeDefined();
  expect(modeControl(container)).toBeNull();
});

const modeSelect = (container: HTMLElement) => container.querySelector<HTMLSelectElement>('[role="tabpanel"] select[aria-label^="Mode for"]')!;
const dialogButton = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')].find((button) => button.textContent === label)!;

it("saves a choice that is not marked for confirmation at once", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, change } = mount("Test Agent");
  change(modeSelect(container), "careful");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent: "test_agent", setting: "mode", value: "careful" });
  await act(async () => {});
});

it("asks for confirmation of the choice its descriptor marks, in the words of the provider's module, before saving it", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, change } = mount("Test Agent");
  change(modeSelect(container), "reckless");
  expect(request).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"] h2')?.textContent).toBe("Use reckless mode?");
  expect(document.querySelector('[role="alertdialog"]')!.textContent).toContain("The test agent will skip every safeguard.");
  act(() => dialogButton("Cancel").click());
  expect(request).not.toHaveBeenCalled();
  expect(modeSelect(container).value).toBe("inherit");
  change(modeSelect(container), "reckless");
  act(() => dialogButton("Reckless").click());
  await act(async () => {});
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent: "test_agent", setting: "mode", value: "reckless" });
});
