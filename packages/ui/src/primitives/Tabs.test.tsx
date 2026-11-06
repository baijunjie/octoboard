// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { Tab, TabList, TabListContainer, TabPanel, Tabs } from "./Tabs";
import { mount } from "./testSupport";

it("gives every tab the compositing layer and the kept hover opacity, and keeps a caller's classes", () => {
  const { container } = mount(
    <Tabs defaultSelectedKey="one">
      <TabListContainer>
        <TabList label="Sections">
          <Tab id="one" className="gap-2">One</Tab>
          <Tab id="two" size="compact" indicator={false}>Two</Tab>
        </TabList>
      </TabListContainer>
      <TabPanel id="one">First</TabPanel>
      <TabPanel id="two">Second</TabPanel>
    </Tabs>,
  );
  const tabs = Array.from(container.querySelectorAll<HTMLElement>("[role=tab]"));
  for (const tab of tabs) {
    expect(tab.className).toContain("transform-[translateZ(0)]");
    expect(tab.className).toContain("[&:not([data-selected=true]):not([data-disabled=true]):is(:hover,[data-hovered=true])]:opacity-100");
  }
  expect(tabs[0].className).toContain("gap-2");
  expect(tabs[1].className).toContain("h-6 px-1.5");
  expect(container.querySelector("[role=tablist]")!.getAttribute("aria-label")).toBe("Sections");
});

it("reports the selection as the tab's id, labels a tab and keeps a panel mounted on request", () => {
  const onSelectionChange = vi.fn();
  const { container } = mount(
    <Tabs variant="underline" selectedKey="one" onSelectionChange={onSelectionChange}>
      <TabListContainer fit="content" data-probe="">
        <TabList label="Agents" fit="content">
          <Tab id="one">One</Tab>
          <Tab id="two" label="Two, not installed" data-list-motion="two">Two</Tab>
        </TabList>
      </TabListContainer>
      <TabPanel id="one">First</TabPanel>
      <TabPanel id="two" keepMounted>Second</TabPanel>
    </Tabs>,
  );
  const second = container.querySelectorAll<HTMLElement>("[role=tab]")[1];
  expect(second.getAttribute("aria-label")).toBe("Two, not installed");
  expect(second.dataset.listMotion).toBe("two");
  expect(container.querySelector("[data-probe]")!.className).toContain("w-max max-w-full self-start");
  expect(container.textContent).toContain("Second");
  act(() => {
    second.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, pointerType: "mouse" }));
    second.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" }));
    second.click();
  });
  expect(onSelectionChange).toHaveBeenCalledWith("two");
});
