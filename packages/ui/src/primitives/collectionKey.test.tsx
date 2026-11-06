// @vitest-environment jsdom
import { expect, it } from "vitest";

import { itemKeyOf, itemKeySelector } from "./collectionKey";
import { Tab, TabList, Tabs } from "./Tabs";
import { mount } from "./testSupport";

it("reads a tab's key and finds the tab again by it", () => {
  const { container } = mount(
    <Tabs defaultSelectedKey="a:b">
      <TabList label="Sections">
        <Tab id="a:b">One</Tab>
        <Tab id="two">Two</Tab>
      </TabList>
    </Tabs>,
  );
  const found = container.querySelector<HTMLElement>(`[role=tab]${itemKeySelector("a:b")}`)!;
  expect(found.textContent).toContain("One");
  expect(itemKeyOf(found)).toBe("a:b");
  expect(container.querySelectorAll(`[role=tab]${itemKeySelector()}`)).toHaveLength(2);
  expect(itemKeyOf(null)).toBeUndefined();
  expect(itemKeyOf(container)).toBeUndefined();
});
