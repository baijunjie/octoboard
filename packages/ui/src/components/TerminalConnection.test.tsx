// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { TerminalProblem } from "../terminal/TerminalPane";
import { TerminalConnection } from "./TerminalConnection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
});

function mount(problem?: TerminalProblem) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() => root.render(<TerminalConnection problem={problem} />));
  return container;
}

it("is empty but for its status region while the terminal is healthy", () => {
  const container = mount();
  expect(container.querySelector("[role=status]")?.textContent).toBe("");
  expect(container.querySelector("button")).toBeNull();
});

it("shows a turning arrow, nothing to press, while reconnecting", () => {
  const container = mount({ state: "reconnecting", reconnect: vi.fn() });
  expect(container.querySelector("[role=status]")?.textContent).toBe("Reconnecting the terminal…");
  expect(container.querySelector("[role=img]")?.getAttribute("aria-label")).toBe("Reconnecting the terminal…");
  expect(container.querySelector("button")).toBeNull();
});

it("reconnects when the disconnected indicator is pressed", () => {
  const reconnect = vi.fn();
  const container = mount({ state: "disconnected", reconnect });
  expect(container.querySelector("[role=status]")?.textContent).toBe("Terminal disconnected");
  const button = container.querySelector("button");
  expect(button?.getAttribute("aria-label")).toBe("Terminal disconnected. Reconnect it.");
  act(() => button?.click());
  expect(reconnect).toHaveBeenCalledTimes(1);
});
