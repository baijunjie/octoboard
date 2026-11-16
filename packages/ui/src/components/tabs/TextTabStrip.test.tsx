// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { Tabs, TabPanel } from "../../primitives";
import { mountHost } from "../../test/mountHost";
import { click } from "../../test/dom";
import { TextTabStrip } from "./TextTabStrip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TABS = [{ id: "a", label: "First" }, { id: "b", label: "Second" }];

function mount(underline?: boolean) {
  return mountHost(
    <Tabs variant={underline ? "underline" : undefined} defaultSelectedKey="a">
      <TextTabStrip label="Ways in" tabs={TABS} underline={underline} />
      <TabPanel id="a">panel a</TabPanel>
      <TabPanel id="b">panel b</TabPanel>
    </Tabs>,
  );
}

const tabs = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>('[role="tab"]')];

it("draws one tab per entry, in order, in a tab list named by its label", () => {
  const container = mount();
  expect(tabs(container).map((tab) => tab.textContent?.trim())).toEqual(["First", "Second"]);
  expect(container.querySelector('[role="tablist"]')?.getAttribute("aria-label")).toBe("Ways in");
  expect(tabs(container).map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false"]);
});

it("selects a tab on a press and shows its panel", async () => {
  const container = mount(true);
  await click(tabs(container)[1]);
  expect(tabs(container).map((tab) => tab.getAttribute("aria-selected"))).toEqual(["false", "true"]);
  expect(container.querySelector('[role="tabpanel"]')?.textContent).toBe("panel b");
  await act(async () => {});
});

it("is a strip drawn as Agent accounts' with a divider across the whole row only when underlined", () => {
  const underlined = mount(true);
  const row = underlined.querySelector('[role="tablist"]')!.parentElement!.closest<HTMLElement>("[class*='groove-b']");
  expect(row).not.toBeNull();
  expect(row!.className).toContain("h-10");
  expect(row!.className).toContain("w-full");
  const filled = mount(false);
  expect(filled.querySelector("[class*='groove-b']")).toBeNull();
});
