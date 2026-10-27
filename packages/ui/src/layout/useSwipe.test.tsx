// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { SWIPE_DISTANCE_PX, useSwipe } from "./useSwipe";
import type { SwipeToward } from "./paneSwipe";
import { touchEvent } from "./touchTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount({ enabled = true, rtl = false, body = "" }: { enabled?: boolean; rtl?: boolean; body?: string } = {}) {
  down.clear();
  const swipes: SwipeToward[] = [];
  function Probe() {
    useSwipe(enabled, rtl, (toward) => swipes.push(toward));
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  container.innerHTML = body;
  const probe = document.body.appendChild(document.createElement("div"));
  const root = createRoot(probe);
  act(() => root.render(<Probe />));
  onTestFinished(() => {
    act(() => root.unmount());
    probe.remove();
    container.remove();
  });
  return { swipes, container };
}

/** The fingers on screen, as `touches` lists them; `touch` sends one finger's event. */
const down = new Map<number, { x: number; y: number }>();

function touch(target: Element, type: string, x: number, y: number, init: { id?: number } = {}) {
  const id = init.id ?? 1;
  if (type === "touchend" || type === "touchcancel") down.delete(id);
  else down.set(id, { x, y });
  const make = (identifier: number, at: { x: number; y: number }) => ({ identifier, clientX: at.x, clientY: at.y });
  const event = touchEvent(type, Array.from(down, ([key, at]) => make(key, at)), [make(id, { x, y })]);
  act(() => void target.dispatchEvent(event));
}

/** A finger down at (100, 100) that moves by (dx, dy) in the given steps. */
function drag(target: Element, dx: number, dy = 0) {
  touch(target, "touchstart", 100, 100);
  touch(target, "touchmove", 100 + dx / 2, 100 + dy / 2);
  touch(target, "touchmove", 100 + dx, 100 + dy);
  touch(target, "touchend", 100 + dx, 100 + dy);
}

function scroller(element: Element, { scrollLeft, scrollWidth = 600, clientWidth = 200, direction = "ltr" }: { scrollLeft: number; scrollWidth?: number; clientWidth?: number; direction?: string }) {
  Object.defineProperties(element, { scrollLeft: { value: scrollLeft }, scrollWidth: { value: scrollWidth }, clientWidth: { value: clientWidth } });
  (element as HTMLElement).style.overflowX = "auto";
  (element as HTMLElement).style.direction = direction;
}

it("reports a rightward swipe as towards the end and a leftward one as towards the start", () => {
  const test = mount();
  drag(document.body, SWIPE_DISTANCE_PX + 4);
  drag(document.body, -(SWIPE_DISTANCE_PX + 4));
  expect(test.swipes).toEqual(["end", "start"]);
});

it("mirrors the directions under right-to-left", () => {
  const test = mount({ rtl: true });
  drag(document.body, SWIPE_DISTANCE_PX + 4);
  drag(document.body, -(SWIPE_DISTANCE_PX + 4));
  expect(test.swipes).toEqual(["start", "end"]);
});

it("fires once the move passes the distance, without waiting for the release", () => {
  const test = mount();
  touch(document.body, "touchstart", 100, 100);
  touch(document.body, "touchmove", 100 + SWIPE_DISTANCE_PX - 1, 100);
  expect(test.swipes).toEqual([]);
  touch(document.body, "touchmove", 100 + SWIPE_DISTANCE_PX, 100);
  expect(test.swipes).toEqual(["end"]);
  touch(document.body, "touchmove", 100 + 3 * SWIPE_DISTANCE_PX, 100);
  expect(test.swipes).toEqual(["end"]);
});

it("ignores a move shorter than the distance", () => {
  const test = mount();
  drag(document.body, SWIPE_DISTANCE_PX - 8);
  expect(test.swipes).toEqual([]);
});

it("ignores a mostly vertical move and a diagonal one", () => {
  const test = mount();
  drag(document.body, 80, 120);
  drag(document.body, 80, 60);
  expect(test.swipes).toEqual([]);
});

it("drops a touch that has turned vertical even if it then goes far sideways", () => {
  const test = mount();
  touch(document.body, "touchstart", 100, 100);
  touch(document.body, "touchmove", 105, 140);
  touch(document.body, "touchmove", 300, 140);
  expect(test.swipes).toEqual([]);
});

it("does nothing while disabled", () => {
  const test = mount({ enabled: false });
  drag(document.body, 100);
  expect(test.swipes).toEqual([]);
});

it("ignores a second finger", () => {
  const test = mount();
  touch(document.body, "touchstart", 100, 100, { id: 1 });
  touch(document.body, "touchstart", 200, 100, { id: 2 });
  touch(document.body, "touchmove", 220, 100, { id: 1 });
  touch(document.body, "touchmove", 300, 100, { id: 1 });
  expect(test.swipes).toEqual([]);
});

it("ignores a touch that ended without a swipe, and starts afresh with the next", () => {
  const test = mount();
  touch(document.body, "touchstart", 100, 100);
  touch(document.body, "touchcancel", 110, 100);
  touch(document.body, "touchmove", 300, 100);
  expect(test.swipes).toEqual([]);
  drag(document.body, 100);
  expect(test.swipes).toEqual(["end"]);
});

it("leaves a drag that starts on a separator or a slider to it", () => {
  const test = mount({ body: '<div role="separator"></div><input type="range">' });
  drag(test.container.querySelector("[role=separator]")!, 100);
  drag(test.container.querySelector("input")!, 100);
  expect(test.swipes).toEqual([]);
});

it("ignores a touch while a dialog, a menu or an open popup is on screen", () => {
  for (const open of ['<div role="dialog"></div>', '<div role="menu"></div>', '<button aria-haspopup="menu" aria-expanded="true"></button>']) {
    const test = mount({ body: open });
    drag(document.body, 100);
    expect(test.swipes).toEqual([]);
  }
});

it("does not take a swipe that starts in an element that can scroll sideways the way the finger goes", () => {
  const test = mount({ body: "<div><span></span></div>" });
  const inner = test.container.querySelector("span")!;
  scroller(test.container.firstElementChild!, { scrollLeft: 100 });
  // Content is out of view on both sides: either way the finger goes, the element scrolls.
  drag(inner, 100);
  drag(inner, -100);
  expect(test.swipes).toEqual([]);
});

it("takes a swipe that goes the way a scroller has no more content in", () => {
  const test = mount({ body: "<div><span></span></div>" });
  const inner = test.container.querySelector("span")!;
  scroller(test.container.firstElementChild!, { scrollLeft: 0 });
  // At the left edge: moving right has nothing to reveal, moving left does.
  drag(inner, 100);
  drag(inner, -100);
  expect(test.swipes).toEqual(["end"]);
});

it("reads the scroll position of a right-to-left scroller from its end", () => {
  const test = mount({ body: "<div><span></span></div>" });
  const inner = test.container.querySelector("span")!;
  // Scrolled to its start edge (the right): the hidden content is to the left, so only a finger
  // moving left has nothing to scroll.
  scroller(test.container.firstElementChild!, { scrollLeft: 0, direction: "rtl" });
  drag(inner, 100);
  drag(inner, -100);
  expect(test.swipes).toEqual(["start"]);
});

it("ignores an element that overflows but does not scroll", () => {
  const test = mount({ body: "<div><span></span></div>" });
  const inner = test.container.querySelector("span")!;
  scroller(test.container.firstElementChild!, { scrollLeft: 100 });
  (test.container.firstElementChild as HTMLElement).style.overflowX = "hidden";
  drag(inner, 100);
  expect(test.swipes).toEqual(["end"]);
});
