// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { LoadingState } from "./LoadingState";

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

it("is hidden from assistive technology and shows its label", () => {
  for (const size of ["sm", "md"] as const) {
    const state = mount(<LoadingState size={size} label="Reading" />).firstElementChild!;
    expect(state.getAttribute("aria-hidden")).toBe("true");
    expect(state.textContent).toContain("Reading");
  }
});

it("leaves out the spinner when asked", () => {
  const withSpinner = mount(<LoadingState size="md" label="Reading" />);
  const without = mount(<LoadingState size="md" label="Reading" spinner={false} />);
  expect(withSpinner.querySelector("[role=progressbar], svg, .spinner")).not.toBeNull();
  expect(without.querySelector("[role=progressbar], svg, .spinner")).toBeNull();
});
