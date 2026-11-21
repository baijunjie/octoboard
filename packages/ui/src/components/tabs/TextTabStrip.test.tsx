// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { Tabs, TabPanel } from "../../primitives";
import { mountHost } from "../../test/mountHost";
import { click } from "../../test/dom";
import { TextTabStrip } from "./TextTabStrip";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TABS = [{ id: "a", label: "First" }, { id: "b", label: "Second" }];

function mount(underline?: "hairline" | "groove") {
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
  const container = mount("hairline");
  await click(tabs(container)[1]);
  expect(tabs(container).map((tab) => tab.getAttribute("aria-selected"))).toEqual(["false", "true"]);
  expect(container.querySelector('[role="tabpanel"]')?.textContent).toBe("panel b");
  await act(async () => {});
});

const rowOf = (container: HTMLElement) => container.querySelector('[role="tablist"]')!.parentElement!.closest<HTMLElement>("[class*='h-10']");

it.each([["hairline", "groove"], ["groove", "hairline"]] as const)("draws the %s kind of divider across the whole row when underlined, and not the %s", (kind, other) => {
  const row = rowOf(mount(kind));
  expect(row).not.toBeNull();
  expect(row!.classList.contains(`${kind}-b`)).toBe(true);
  expect(row!.className).not.toContain(`${other}-b`);
  expect(row!.className).toContain("h-10");
  expect(row!.className).toContain("w-full");
});

it("draws no divider and no row height without an underline", () => {
  const filled = mount();
  expect(filled.querySelector("[class*='groove-b']")).toBeNull();
  expect(filled.querySelector("[class*='hairline-b']")).toBeNull();
  expect(rowOf(filled)).toBeNull();
});
