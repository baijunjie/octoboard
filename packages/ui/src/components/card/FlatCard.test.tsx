// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { FlatCard } from "./FlatCard";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const unmounts: Array<() => void> = [];

afterEach(() => {
  for (const unmount of unmounts.splice(0)) act(unmount);
  document.body.replaceChildren();
});

function mount(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  unmounts.push(() => root.unmount());
  act(() => root.render(element));
  return container;
}

it("labels the card by its heading", () => {
  const container = mount(
    <FlatCard>
      <FlatCard.Header>
        <FlatCard.Heading>Claude</FlatCard.Heading>
      </FlatCard.Header>
    </FlatCard>,
  );
  const section = container.querySelector("section")!;
  const heading = container.querySelector("h3")!;
  expect(heading.id).not.toBe("");
  expect(section.getAttribute("aria-labelledby")).toBe(heading.id);
  expect(section.className).toContain("panel-edge");
  expect(section.className).not.toContain("shadow");
});

it("insets the dividers of its rows and draws none after the last", () => {
  const container = mount(
    <FlatCard>
      <FlatCard.Header>
        <FlatCard.Heading>Title</FlatCard.Heading>
      </FlatCard.Header>
      <FlatCard.Rows>
        <li>one</li>
        <li>two</li>
      </FlatCard.Rows>
    </FlatCard>,
  );
  const list = container.querySelector("ul")!;
  expect(list.className).toContain("[&>li]:mx-4");
  expect(list.className).toContain("[&>li]:hairline-b");
  expect(list.className).toContain("[&>li:last-child]:border-b-0");
  expect(container.querySelector("h3")!.parentElement!.className).not.toContain("hairline");
});

it("insets and divides its blocks without a second inset on a list among them, and leaves a band undivided", () => {
  const container = mount(
    <FlatCard>
      <FlatCard.Header>
        <FlatCard.Heading>Title</FlatCard.Heading>
      </FlatCard.Header>
      <FlatCard.Blocks>
        <div>defaults</div>
        <FlatCard.Rows padded>
          <li>line</li>
        </FlatCard.Rows>
      </FlatCard.Blocks>
      <FlatCard.Band>Accounts</FlatCard.Band>
      <FlatCard.Rows>
        <li>account</li>
      </FlatCard.Rows>
    </FlatCard>,
  );
  const stack = container.querySelector("section > div:not(:first-child)")!;
  expect(stack.className).toContain("[&>*]:mx-4");
  expect(stack.className).toContain("[&>*]:hairline-b");
  expect(stack.className).toContain("[&>*:last-child]:border-b-0");
  const [inner, outer] = Array.from(container.querySelectorAll("ul"));
  expect(inner.className).not.toContain("[&>li]:mx-4");
  expect(outer.className).toContain("[&>li]:mx-4");
  const band = container.querySelector("h4")!;
  expect(band.className).toContain("bg-(--control-fill)");
  expect(band.className).not.toMatch(/border|hairline|groove/);
});
