// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { SettingRow } from "./SettingRow";

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

it("puts the help at the end of the label's line, before the description", () => {
  const container = mount(<SettingRow label="Appearance" help={<button>About appearance</button>} description="Muted line" />);
  const line = container.querySelector("button")!.parentElement!;
  expect(line.textContent).toBe("AppearanceAbout appearance");
  expect(line.lastElementChild).toBe(container.querySelector("button"));
  expect(line.nextElementSibling?.textContent).toBe("Muted line");
});

it("renders no description line when it has none", () => {
  const container = mount(<SettingRow label="Appearance" help={<button>About appearance</button>} />);
  const line = container.querySelector("button")!.parentElement!;
  expect(line.nextElementSibling).toBeNull();
});
