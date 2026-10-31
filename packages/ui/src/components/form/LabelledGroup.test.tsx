// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { LabelledGroup } from "./LabelledGroup";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const help = <button type="button" aria-label="About the field">?</button>;

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

it("seats the help beside a group's label, which alone names the group", () => {
  const container = mount(
    <LabelledGroup label="Default agent" help={help}>
      <span>controls</span>
    </LabelledGroup>,
  );
  const group = container.querySelector<HTMLElement>('[role="group"]')!;
  const name = document.getElementById(group.getAttribute("aria-labelledby")!)!;
  expect(name.textContent).toBe("Default agent");
  expect(name.nextElementSibling).toBe(container.querySelector("button"));
  expect(group.hasAttribute("aria-describedby")).toBe(false);
});

it("renders a plain label when there is no help", () => {
  const container = mount(<LabelledGroup label="Default agent"><span>controls</span></LabelledGroup>);
  const name = container.querySelector('[role="group"]')!.firstElementChild!;
  expect(name.tagName).toBe("LABEL");
  expect(container.querySelector("button")).toBeNull();
});
