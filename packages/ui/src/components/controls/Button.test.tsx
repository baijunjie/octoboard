// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { Button } from "./Button";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactNode): HTMLButtonElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(element));
  return container.querySelector("button")!;
}

it("shows a decorative spinner before the unchanged label while pending", () => {
  const button = mount(<Button isPending>Save</Button>);
  const spinner = button.querySelector("[data-slot=spinner]")!;
  expect(button.textContent).toBe("Save");
  expect(spinner.getAttribute("aria-hidden")).toBe("true");
  expect(button.firstElementChild).toBe(spinner);
  expect(button.getAttribute("aria-busy")).toBe("true");
});

it("shows neither spinner nor busy state when not pending", () => {
  const button = mount(<Button>Save</Button>);
  expect(button.querySelector("[data-slot=spinner]")).toBeNull();
  expect(button.hasAttribute("aria-busy")).toBe(false);
});

it("lets a pending icon-only button show the spinner in place of its icon", () => {
  const button = mount(<Button isIconOnly isPending aria-label="Pick"><span data-testid="icon" /></Button>);
  expect(button.querySelector("[data-testid=icon]")).toBeNull();
  expect(button.querySelector("[data-slot=spinner]")).not.toBeNull();
});

it("passes render-prop children through with the spinner", () => {
  const button = mount(<Button isPending>{({ isPending }) => (isPending ? "Working" : "Idle")}</Button>);
  expect(button.textContent).toBe("Working");
  expect(button.querySelector("[data-slot=spinner]")).not.toBeNull();
});
