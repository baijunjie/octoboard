// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { ConfirmSection } from "./ConfirmSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactElement) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container;
}

const group = (container: HTMLElement) => container.querySelector<HTMLElement>('[role="group"]');

it("names its group by its heading", () => {
  const container = mount(<ConfirmSection heading="Branches" shown divided={false}><p>Body</p></ConfirmSection>);
  const heading = container.querySelector("h3")!;
  expect(heading.textContent).toBe("Branches");
  expect(group(container)!.getAttribute("aria-labelledby")).toBe(heading.id);
  expect(group(container)!.textContent).toContain("Body");
});

it("draws no rule when it is the first section shown", () => {
  const container = mount(<ConfirmSection heading="Branches" shown divided={false}>x</ConfirmSection>);
  expect(group(container)!.className).not.toMatch(/hairline|groove/);
});

it("is divided from the section above by a hairline, not a groove", () => {
  const container = mount(<ConfirmSection heading="Branches" shown divided>x</ConfirmSection>);
  expect(group(container)!.classList.contains("hairline-t")).toBe(true);
  expect(group(container)!.className).not.toContain("groove");
});

it("renders no group while it is not shown", () => {
  const container = mount(<ConfirmSection heading="Branches" shown={false} divided>x</ConfirmSection>);
  expect(group(container)).toBeNull();
  expect(container.querySelector("h3")).toBeNull();
});
