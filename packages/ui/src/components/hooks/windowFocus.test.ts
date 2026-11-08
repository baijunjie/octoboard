// @vitest-environment jsdom
// Loaded first, as `main.tsx` loads it, so a guard that swallows the window's focus events fails this test.
import "../../input/focusGuard";
import { expect, it, vi } from "vitest";

import { onWindowFocusLoss, onWindowFocusReturn, subscribeWindowFocus } from "./windowFocus";

it("answers the window's own focus and blur", () => {
  const onLoss = vi.fn();
  const onReturn = vi.fn();
  const onChange = vi.fn();
  const undo = [onWindowFocusLoss(onLoss), onWindowFocusReturn(onReturn), subscribeWindowFocus(onChange)];
  window.dispatchEvent(new FocusEvent("focus"));
  expect([onLoss.mock.calls.length, onReturn.mock.calls.length, onChange.mock.calls.length]).toEqual([0, 1, 1]);
  window.dispatchEvent(new FocusEvent("blur"));
  expect([onLoss.mock.calls.length, onReturn.mock.calls.length, onChange.mock.calls.length]).toEqual([1, 1, 2]);
  for (const done of undo) done();
  window.dispatchEvent(new FocusEvent("blur"));
  expect([onLoss.mock.calls.length, onChange.mock.calls.length]).toEqual([1, 2]);
});
