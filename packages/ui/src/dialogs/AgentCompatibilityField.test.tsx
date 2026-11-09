// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import { settingMessageKey } from "../agents/providers";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { ProviderInfo } from "../protocol";
import { DaemonProvider } from "../store";
import { settingReferences } from "../providers/registry";
import { buttons, click, optionElements, pickerNamed, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { AgentCompatibilityField } from "./AgentCompatibilityField";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const t = translatorFor("en");
const copy = (provider: ProviderInfo, setting: string, part: string) => {
  const key = settingMessageKey(provider.id, setting, part);
  expect(key).toBeDefined();
  return withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(key!, { agent: isolate(provider.label), ...settingReferences(provider.id, setting) }))!;
};
async function mount(provider: ProviderInfo, inherit?: Record<string, boolean>) {
  fakeTimers();
  const daemon = testDaemon({ state: { providers: TEST_PROVIDERS.map((entry) => entry.id === provider.id ? provider : entry) } });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount()); container.remove();
  });
  const changed = vi.fn();
  function Form() {
    const [value, setValue] = useState<Record<string, boolean | null>>({});
    return <AgentCompatibilityField agent={provider.id} value={value} inherit={inherit} onChange={(next) => { changed(next); setValue(next); }} />;
  }
  await act(async () => root.render(<DaemonProvider value={daemon}><Form /></DaemonProvider>));
  return { daemon, changed, container };
}
it.each(TEST_PROVIDERS)("renders $id's declared order, accessible provider copy and reachable help", async (provider) => {
  const { container } = await mount({ ...provider, settings: [...provider.settings].reverse() });
  const settings = [...provider.settings].reverse().filter((setting) => setting.compatibility_with !== null);
  const actual = [...container.querySelectorAll('[data-slot="select-trigger"]')];
  expect(actual).toHaveLength(settings.length);
  for (const setting of settings) {
    const picker = pickerNamed(copy(provider, setting.id, "label"));
    expect(picker).toBeDefined();
    expect(withoutIsolates(container.textContent)).toContain(copy(provider, setting.id, "heading"));
    const help = buttons().find((button) => withoutIsolates(button.getAttribute("aria-label")) === copy(provider, setting.id, "helpLabel"));
    expect(help).toBeDefined();
    expect(help!.closest('[aria-hidden="true"]')).toBeNull();
    expect(help!.querySelector("svg")).not.toBeNull();
    await click(help);
    const popover = document.querySelector('[role="dialog"]');
    expect(popover).not.toBeNull();
    const part = settingMessageKey(provider.id, setting.id, "helpTiming") ? "helpTiming" : "description";
    expect(withoutIsolates(popover!.textContent)).toContain(copy(provider, setting.id, part));
    await act(async () => { popover!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
  }
  expect(actual).toEqual(settings.map((setting) => pickerNamed(copy(provider, setting.id, "label"))));
});
const compatible = TEST_PROVIDERS.filter((provider) => provider.settings.some((setting) => setting.compatibility_with !== null));
it.each(compatible)("offers follow, On, Off for $id and updates the inherited badge live", async (provider) => {
  const setting = provider.settings.find((entry) => entry.compatibility_with !== null)!;
  const { daemon, changed } = await mount(provider);
  const picker = () => pickerNamed(copy(provider, setting.id, "label"));
  const defaultOn = setting.kind.type === "switch" && setting.kind.default !== "off";
  expect(picker().textContent).toContain(`Same as agent setting, ${defaultOn ? "On" : "Off"}`);
  await act(async () => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [provider.id]: { [setting.id]: !defaultOn } } } })));
  expect(picker().textContent).toContain(`Same as agent setting, ${defaultOn ? "Off" : "On"}`);
  await click(picker());
  expect(optionElements().map((option) => option.textContent)).toEqual([`Same as agent setting, ${defaultOn ? "Off" : "On"}`, "On", "Off"]);
  await click(optionElements().find((option) => option.textContent === "Off"));
  expect(changed).toHaveBeenLastCalledWith({ [setting.id]: false });
  await click(picker()); await click(optionElements().find((option) => option.textContent === "On"));
  expect(changed).toHaveBeenLastCalledWith({ [setting.id]: true });
  await click(picker()); await click(optionElements()[0]);
  expect(changed).toHaveBeenLastCalledWith({ [setting.id]: null });
});
it("keeps explicit inherited false ahead of the agent-level setting", async () => {
  const provider = compatible[0];
  const setting = provider.settings.find((entry) => entry.compatibility_with !== null)!;
  const { daemon } = await mount(provider, { [setting.id]: false });
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [provider.id]: { [setting.id]: true } } } })));
  expect(pickerNamed(copy(provider, setting.id, "label")).textContent).toContain("Same as agent setting, Off");
});

it.each(["on", "off"] as const)("uses the descriptor's %s default when the agent setting is absent", async (value) => {
  const provider = compatible[0];
  const setting = provider.settings.find((entry) => entry.compatibility_with !== null)!;
  const descriptor: ProviderInfo = { ...provider, settings: provider.settings.map((entry) => entry.id === setting.id ? { ...entry, kind: { type: "switch", default: value } } : entry) };
  await mount(descriptor);
  expect(pickerNamed(copy(provider, setting.id, "label")).textContent).toContain(`Same as agent setting, ${value === "on" ? "On" : "Off"}`);
});
