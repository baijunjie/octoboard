// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { request, settings } from "./agentAccountsSectionTestState";
import { buttonNamed, chooseItem, mount, mountAgentSettings, openTabMenu, selectTab } from "./agentAccountsSectionTestSupport";
import { TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../../test/testProviders";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const tabNames = (container: HTMLElement) => [...container.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
const modeControl = (container: HTMLElement) => container.querySelector('[aria-label^="Mode for"]');

it("words the test-only provider's setting from its presentation module", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, section } = await mountAgentSettings("Test Agent");
  expect(tabNames(section)).toEqual(["Claude Code", "Test Agent"]);
  expect(modeControl(container)?.getAttribute("aria-label")).toBe("Mode for \u2068Test Agent\u2069");
  expect(container.textContent).toContain("Test mode");
});

it("shows a served provider without a presentation module under its served label, with none of its settings", async () => {
  settings.added_agents = ["claude", "unpresented_agent"];
  const { container, set } = mount();
  set({ providers: [...TEST_PROVIDERS, UNPRESENTED_PROVIDER] });
  expect(tabNames(container)).toEqual(["Claude Code", "Unpresented Agent"]);
  await openTabMenu(container, "Unpresented Agent");
  await chooseItem("Settings");
  expect(modeControl(document.body.querySelector('[role="dialog"]')!)).toBeNull();
});

/** The served providers with the test-only provider declaring that it has the default account only. */
const WITHOUT_OWNED_ACCOUNTS = TEST_PROVIDERS.map((provider) => provider.id === "test_agent" ? { ...provider, capabilities: { ...provider.capabilities, owned_accounts: false } } : provider);

it("offers Add account on the tab of a provider that has owned accounts, and not on the tab of one that has the default account only", () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, set } = mount();
  set({ providers: WITHOUT_OWNED_ACCOUNTS });
  expect(buttonNamed(container, "Add account")).toBeDefined();
  selectTab(container, "Test Agent");
  expect(buttonNamed(container, "Add account")).toBeUndefined();
  expect(buttonNamed(container, "Add agent")).toBeDefined();
  expect(container.querySelector('[role="tabpanel"]:not([hidden])')?.querySelector("[data-account-row]")).not.toBeNull();
  selectTab(container, "Claude Code");
  expect(buttonNamed(container, "Add account")).toBeDefined();
});

it("pads the tabs' end for both actions on a provider that has owned accounts, and for the one action left on one that has the default account only", () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, set } = mount();
  set({ providers: WITHOUT_OWNED_ACCOUNTS });
  const width = () => container.querySelector<HTMLElement>("[data-trailing-control]")?.style.getPropertyValue("--tab-trailing-width");
  const twoControls = "calc(2 * var(--row-control-size) + 0.25rem)";
  expect(width()).toBe(twoControls);
  selectTab(container, "Test Agent");
  expect(width()).toBe("var(--row-control-size)");
  selectTab(container, "Claude Code");
  expect(width()).toBe(twoControls);
});

it("leaves a provider that has the default account only out of the new account form's agents", () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, set } = mount();
  const agents = () => [...document.body.querySelectorAll<HTMLOptionElement>('select[aria-label="Agent"] option')].map((option) => option.value);
  act(() => buttonNamed(container, "Add account")!.click());
  expect(agents()).toEqual(["claude", "test_agent"]);
  act(() => document.body.querySelector<HTMLButtonElement>('[role="dialog"] button[aria-label="Close"], [role="dialog"] button')!.click());
  set({ providers: WITHOUT_OWNED_ACCOUNTS });
  act(() => buttonNamed(container, "Add account")!.click());
  expect(agents()).toEqual(["claude"]);
});

const modeSelect = (container: HTMLElement) => container.querySelector<HTMLSelectElement>('select[aria-label^="Mode for"]')!;
const dialogButton = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')].find((button) => button.textContent === label)!;

it("saves a choice that is not marked for confirmation at once", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, change } = await mountAgentSettings("Test Agent");
  change(modeSelect(container), "careful");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent: "test_agent", setting: "mode", value: "careful" });
  await act(async () => {});
});

it("asks for confirmation of the choice its descriptor marks, in the words of the provider's module, before saving it", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container, change } = await mountAgentSettings("Test Agent");
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
