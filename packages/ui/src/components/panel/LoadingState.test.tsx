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

it.each(["sm", "md"] as const)("draws the grey spinner at size %s", (size) => {
  const container = mount(<LoadingState size={size} label="Starting session" />);
  const spinner = container.querySelector("[data-slot=spinner]")!;
  expect(spinner).not.toBeNull();
  expect(spinner.classList.contains(`spinner--${size}`)).toBe(true);
  expect(spinner.classList.contains("text-app-muted")).toBe(true);
});

it("leaves out the spinner when asked", () => {
  const withSpinner = mount(<LoadingState size="md" label="Reading" />);
  const without = mount(<LoadingState size="md" label="Reading" indicator="none" />);
  expect(withSpinner.querySelector("[data-slot=spinner]")).not.toBeNull();
  expect(without.querySelector("[data-slot=spinner], svg")).toBeNull();
  expect(without.textContent).toBe("Reading");
});
