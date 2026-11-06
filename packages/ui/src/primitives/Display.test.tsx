// @vitest-environment jsdom
import { TextContext } from "react-aria-components";
import { expect, it, vi } from "vitest";

import { Badge, BadgeAnchor } from "./Badge";
import { Chip } from "./Chip";
import { Code } from "./Code";
import { Kbd, KbdContent } from "./Kbd";
import { Spinner } from "./Spinner";
import { Tag, TagGroup, TagList, TagRemoveButton } from "./Tag";
import { mount, pressWithMouse } from "./testSupport";

it("leaves a spinner without a label out of the accessibility tree, and names one with a label", () => {
  const { container } = mount(
    <>
      <Spinner size="sm" />
      <Spinner size="sm" label="Loading more" />
    </>,
  );
  const [bare, named] = [...container.children];
  expect(bare.getAttribute("aria-hidden")).toBe("true");
  expect(bare.getAttribute("role")).toBe("presentation");
  expect(bare.hasAttribute("aria-label")).toBe(false);
  expect(named.getAttribute("aria-label")).toBe("Loading more");
  expect(named.hasAttribute("aria-hidden")).toBe(false);
});

it("hides decorative key caps from assistive technology and leaves the others readable", () => {
  const { container } = mount(
    <>
      <Kbd decorative><KbdContent>R</KbdContent></Kbd>
      <Kbd><KbdContent>S</KbdContent></Kbd>
    </>,
  );
  const [decorative, readable] = [...container.children];
  expect(decorative.getAttribute("aria-hidden")).toBe("true");
  expect(readable.hasAttribute("aria-hidden")).toBe(false);
});

it("draws a badge as decoration: inert and hidden from assistive technology", () => {
  const { container } = mount(
    <BadgeAnchor>
      <button type="button">Bell</button>
      <Badge color="accent" />
    </BadgeAnchor>,
  );
  const badge = container.querySelector("button")!.nextElementSibling!;
  expect(badge.hasAttribute("inert")).toBe(true);
  expect(badge.getAttribute("aria-hidden")).toBe("true");
});

it("puts a chip's text in directly, without the chip's padded label", () => {
  const { container } = mount(<Chip size="sm" color="default">A</Chip>);
  const chip = container.firstElementChild!;
  expect(chip.childNodes).toHaveLength(1);
  expect(chip.firstChild!.nodeType).toBe(Node.TEXT_NODE);
});

it("renders code inside a component that hands its text parts slots", () => {
  // React Aria's text throws without a slot inside a context that has slots, as an alert's description or an item's
  // label would provide.
  const { container } = mount(
    <TextContext.Provider value={{ slots: { description: {} } }}>
      <Code dir="ltr">npm ci</Code>
    </TextContext.Provider>,
  );
  expect(container.textContent).toBe("npm ci");
});

it("reports a tag group's selection as whether each tag is picked", () => {
  const onSelectionChange = vi.fn<(isSelected: (tag: string) => boolean) => void>();
  const { container } = mount(
    <TagGroup label="Tags" selectionMode="multiple" selectedKeys={["alpha"]} onSelectionChange={onSelectionChange}>
      <TagList>
        <Tag id="alpha">{({ selected }) => `alpha ${selected ? "picked" : "not picked"}`}</Tag>
        <Tag id="beta">beta</Tag>
      </TagList>
    </TagGroup>,
  );
  expect(container.textContent).toContain("alpha picked");
  const beta = [...container.querySelectorAll('[role="row"]')].find((row) => row.textContent === "beta")!;
  pressWithMouse(beta);
  expect(onSelectionChange).toHaveBeenCalledTimes(1);
  const isSelected = onSelectionChange.mock.calls[0][0];
  expect(["alpha", "beta"].filter(isSelected)).toEqual(["alpha", "beta"]);
});

it("reports the tags removed by a remove button as their texts, and keeps the button out of the tab order", () => {
  const onRemove = vi.fn<(tags: string[]) => void>();
  const { container } = mount(
    <TagGroup label="Tags" onRemove={onRemove}>
      <TagList>
        <Tag id="alpha">
          alpha
          <TagRemoveButton label="Remove" />
        </Tag>
      </TagList>
    </TagGroup>,
  );
  const remove = container.querySelector('button[aria-label="Remove"]')!;
  expect(remove.getAttribute("tabindex")).toBe("-1");
  expect(container.querySelectorAll("button")).toHaveLength(1);
  pressWithMouse(remove);
  expect(onRemove).toHaveBeenCalledWith(["alpha"]);
});
