// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { Activity } from "../sidebar/order";
import { ActivityMarker } from "./ActivityMarker";
import { enter, leave, mountForTooltip, openTooltip, trigger, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ACTIVITY_TEXT: Array<[NonNullable<Activity>, string]> = [
  ["waiting", "A session is waiting for you"],
  ["working", "A session is working"],
  ["running", "A session is running"],
];

it.each(ACTIVITY_TEXT)("tells the %s activity marker apart from the other two, which look alike", (activity, text) => {
  const settle = withFakeTimers();
  const container = mountForTooltip(<ActivityMarker activity={activity} />);

  enter(trigger(container));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(text);

  leave(trigger(container));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("adds nothing to the accessible name of the row or chip the marker sits in", () => {
  const container = mountForTooltip(<ActivityMarker activity={"waiting"} />);
  expect(trigger(container).hasAttribute("aria-label")).toBe(false);
  expect(container.querySelector("[role=img]")).toBeNull();
  expect(container.querySelector("[aria-hidden=true]")).not.toBeNull();
});

it("draws no marker, and so no tooltip, where nothing is going on", () => {
  const container = mountForTooltip(<ActivityMarker activity={undefined} />);
  expect(container.firstElementChild).toBeNull();
});
