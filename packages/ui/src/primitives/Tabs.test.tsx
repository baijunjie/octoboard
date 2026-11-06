// @vitest-environment jsdom
import React, { act } from "react";
import { expect, it, vi } from "vitest";

import { Tab, TabList, TabListContainer, TabPanel, Tabs, useTabPanelLink } from "./Tabs";
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

it("makes the caller's own element the panel of a tab, under the ids react-aria gave the tab", () => {
  function Strip({ tab }: { tab: string | undefined }) {
    const panel = React.useRef<HTMLDivElement>(null);
    const [tabs, setTabs] = React.useState<ReadonlyMap<string, HTMLElement>>(new Map());
    useTabPanelLink(panel, tab === undefined ? undefined : tabs.get(tab));
    return (
      <>
        <Tabs selectedKey={tab ?? ""}>
          <TabListContainer>
            <TabList label="Sections">
              {["one", "two"].map((id) => (
                <Tab key={id} id={id} ref={(element) => { if (element) setTabs((held) => (held.get(id) === element ? held : new Map(held).set(id, element))); }}>{id}</Tab>
              ))}
            </TabList>
          </TabListContainer>
        </Tabs>
        <div ref={panel} data-testid="own-panel" />
      </>
    );
  }
  const { container, render } = mount(<Strip tab="one" />);
  const panel = container.querySelector<HTMLElement>("[data-testid=own-panel]")!;
  const tabs = Array.from(container.querySelectorAll<HTMLElement>("[role=tab]"));
  expect(tabs[0].getAttribute("aria-controls")).toBeTruthy();
  expect(panel.id).toBe(tabs[0].getAttribute("aria-controls"));
  expect(panel.getAttribute("aria-labelledby")).toBe(tabs[0].id);
  render(<Strip tab="two" />);
  expect(panel.id).toBe(tabs[1].getAttribute("aria-controls"));
  expect(panel.getAttribute("aria-labelledby")).toBe(tabs[1].id);
  render(<Strip tab={undefined} />);
  expect(panel.hasAttribute("id")).toBe(false);
  expect(panel.hasAttribute("aria-labelledby")).toBe(false);
});
