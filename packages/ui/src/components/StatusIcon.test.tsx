// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { StatusKind } from "../sessionLabel";
import { StatusIcon } from "./StatusIcon";
import { enter, leave, mountForTooltip, openTooltip, trigger, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const STATUS_TEXT: Array<[StatusKind, string]> = [
  ["starting", "Starting"],
  ["working", "Working"],
  ["waiting_user", "Waiting for you"],
  ["idle", "Awaiting instructions"],
  ["background", "Working in background"],
  ["interrupted", "Interrupted"],
  ["archived", "Archived"],
];

it.each(STATUS_TEXT)("says what the %s glyph stands for, after the delay the pointer rests on it", (status, text) => {
  const settle = withFakeTimers();
  const container = mountForTooltip(<StatusIcon status={status} decorative withTooltip />);

  enter(trigger(container));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(text);

  leave(trigger(container));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("adds nothing to the accessible name of the row or trail a decorative glyph sits in", () => {
  const container = mountForTooltip(<StatusIcon status={"working"} decorative withTooltip />);
  expect(trigger(container).hasAttribute("aria-label")).toBe(false);
  expect(container.querySelector("[role=img]")).toBeNull();
  expect(container.querySelector("[aria-hidden=true]")).not.toBeNull();
});

it("names the tooltip's own trigger where the glyph is the only thing saying the status", () => {
  const container = mountForTooltip(<StatusIcon status={"interrupted"} withTooltip />);
  // The name sits on the trigger, the element the tooltip is linked to, and the glyph inside it is
  // hidden, so the status is announced once.
  expect(trigger(container).getAttribute("role")).toBe("img");
  expect(trigger(container).getAttribute("aria-label")).toBe("Interrupted");
  expect(trigger(container).querySelector("[role=img]")).toBeNull();
});

it("takes no focus and no press: the glyph is no tab stop and a press on it falls through", () => {
  const container = mountForTooltip(<StatusIcon status={"idle"} decorative withTooltip />);
  expect(trigger(container).tabIndex).toBe(-1);
  const press = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
  act(() => void trigger(container).dispatchEvent(press));
  expect(press.defaultPrevented).toBe(true);
});

it("carries no tooltip where a visible status label sits beside the glyph", () => {
  const settle = withFakeTimers();
  const container = mountForTooltip(<StatusIcon status={"working"} decorative />);
  enter(trigger(container));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});
