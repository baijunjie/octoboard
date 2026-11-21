// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { LeadRows } from "./SettingsLead";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactElement) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return { container, root };
}

const wrapper = (container: HTMLElement) => container.firstElementChild as HTMLElement;

it("holds its rows in one element with a hairline above them and nothing below", () => {
  const { container } = mount(<LeadRows><p>Row</p></LeadRows>);
  expect(wrapper(container).textContent).toBe("Row");
  expect(wrapper(container).classList.contains("hairline-t")).toBe(true);
  expect(wrapper(container).classList.contains("hairline-b")).toBe(false);
  expect(container.querySelectorAll("[class*='groove']")).toHaveLength(0);
});

it("also draws a hairline below when more of the section follows", () => {
  const { container } = mount(<LeadRows ruledBelow><p>Row</p></LeadRows>);
  expect(wrapper(container).classList.contains("hairline-t")).toBe(true);
  expect(wrapper(container).classList.contains("hairline-b")).toBe(true);
  expect(container.querySelectorAll("[class*='groove']")).toHaveLength(0);
});

it("is hidden, rules and all, while hidden", () => {
  const { container } = mount(<LeadRows hidden ruledBelow><p>Row</p></LeadRows>);
  expect(wrapper(container).hidden).toBe(true);
  expect(mount(<LeadRows><p>Row</p></LeadRows>).container.firstElementChild!.hasAttribute("hidden")).toBe(false);
});

it("keeps its rows mounted when the rule below comes and goes", () => {
  const { container, root } = mount(<LeadRows><input defaultValue="kept" /></LeadRows>);
  const input = container.querySelector("input")!;
  act(() => root.render(<LeadRows ruledBelow><input defaultValue="kept" /></LeadRows>));
  expect(container.querySelector("input")).toBe(input);
  expect(wrapper(container).classList.contains("hairline-b")).toBe(true);
  act(() => root.render(<LeadRows><input defaultValue="kept" /></LeadRows>));
  expect(container.querySelector("input")).toBe(input);
  expect(wrapper(container).classList.contains("hairline-b")).toBe(false);
});
