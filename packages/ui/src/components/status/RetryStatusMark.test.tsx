// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { DaemonRequestError } from "../../daemon-client";
import { mountForTooltip } from "../hint/tooltipTestSupport";
import { RetryStatusMark } from "./RetryStatusMark";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function press(element: Element) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const dialog = () => document.querySelector('[role="dialog"]');
const tryAgain = () => [...(dialog()?.querySelectorAll("button") ?? [])].find((button) => button.textContent === "Try again");

it("says the caller's sentence, then the failure, and asks again from Try again, closing the popover", () => {
  const onRetry = vi.fn();
  const container = mountForTooltip(
    <RetryStatusMark label="Could not list" error="connection reset" summary="The folder could not be listed." onRetry={onRetry}>
      <p>Pages already here stay usable.</p>
    </RetryStatusMark>,
  );
  press(container.querySelector("button")!);
  const paragraphs = [...dialog()!.querySelectorAll("p")].map((p) => p.textContent);
  expect(paragraphs.slice(0, 2)).toEqual(["Pages already here stay usable.", "The folder could not be listed."]);
  press(tryAgain()!);
  expect(onRetry).toHaveBeenCalledOnce();
  expect(dialog()).toBeNull();
});

it("offers no Try again for a failure asking again cannot mend", () => {
  const container = mountForTooltip(<RetryStatusMark label="Could not list" error={new DaemonRequestError("bad path", "invalid_path", {})} onRetry={() => {}} />);
  press(container.querySelector("button")!);
  expect(dialog()).not.toBeNull();
  expect(tryAgain()).toBeUndefined();
});
