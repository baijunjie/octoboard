// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { GroupRow, GroupRows } from "./GroupRows";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
it("hides the visual heading while leaving the help control and independently named input accessible", () => {
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<GroupRows stacked><GroupRow label="Compatibility" help={<button aria-label="About compatibility">?</button>}><input aria-label="Use compatibility" /></GroupRow></GroupRows>));
  const heading = [...container.querySelectorAll("span")].find((element) => element.textContent === "Compatibility");
  expect(heading).toBeDefined(); expect(heading!.getAttribute("aria-hidden")).toBe("true");
  const help = container.querySelector('button[aria-label="About compatibility"]');
  expect(help).not.toBeNull(); expect(help!.closest('[aria-hidden="true"]')).toBeNull();
  const input = container.querySelector('input[aria-label="Use compatibility"]');
  expect(input).not.toBeNull(); expect(input!.closest('[aria-hidden="true"]')).toBeNull();
});
