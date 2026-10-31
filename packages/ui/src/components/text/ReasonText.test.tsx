// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { ReasonText, reasonText, type Reason } from "./ReasonText";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function render(reason: Reason): { lines: string[]; text: string } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<ReasonText reason={reason} />));
  const result = { lines: Array.from(container.querySelectorAll("span"), (line) => line.textContent ?? ""), text: container.textContent ?? "" };
  act(() => root.unmount());
  container.remove();
  return result;
}

it("sets the state and the detail of a reason on two lines", () => {
  expect(render({ state: "Used up", detail: "resets in 2h 5m" }).lines).toEqual(["Used up", "resets in 2h 5m"]);
});

it("leaves a reason without a detail as one line of text", () => {
  expect(render({ state: "Used up" })).toEqual({ lines: [], text: "Used up" });
});

it("reads a reason as one phrase", () => {
  expect(reasonText({ state: "Used up", detail: "resets in 2h" })).toBe("Used up · resets in 2h");
  expect(reasonText({ state: "Used up" })).toBe("Used up");
});
