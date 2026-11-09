import { act } from "react";
import { expect, onTestFinished, vi } from "vitest";

import { MARQUEE_INITIAL_DELAY, MARQUEE_PAUSE_FAR, MARQUEE_PAUSE_REST, MARQUEE_RETURN_DURATION, MARQUEE_SPEED } from "../text/FadeOverflow";
import { FAKE_FRAME_MS, fakeTimers, runOutTimers } from "../../test/clock";

/** Frames needed to move over 5 px at `MARQUEE_SPEED`, plus a measuring frame, the frame that starts the pause, and the first movement frame. */
export const FRAMES_INTO_RUN = Math.ceil(5 * 1000 / (MARQUEE_SPEED * FAKE_FRAME_MS)) + 3;

/** From the pointer entering a label until it has moved visibly: the pause it rests in, then `FRAMES_INTO_RUN` frames. */
export const MOVING_AFTER_MS = MARQUEE_INITIAL_DELAY + FRAMES_INTO_RUN * FAKE_FRAME_MS;

/** How long one run through `overflow` px of text takes at `MARQUEE_SPEED`. */
export function marqueeRunMs(overflow: number): number {
  return overflow * 1000 / MARQUEE_SPEED;
}

/** One whole round of a marquee over `overflow` px of text: the pause at rest, the run, the pause at the far edge and
 * the return, up to the pause before the next run. Waiting this long outlasts anything it does in one round. */
export function marqueeRoundMs(overflow: number): number {
  return MARQUEE_INITIAL_DELAY + marqueeRunMs(overflow) + MARQUEE_PAUSE_FAR + MARQUEE_RETURN_DURATION + MARQUEE_PAUSE_REST;
}

/** Gives every label overflow under jsdom, which lays nothing out, enables motion, and fakes the clock. */
export function stubMarquee(): void {
  const matchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
  onTestFinished(() => void (window.matchMedia = matchMedia));
  for (const [name, value] of [["scrollWidth", 300], ["clientWidth", 100]] as const) {
    const original = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, original));
  }
  // After the stubs above, so that they are still in place when the teardown runs the pending timers out.
  fakeTimers();
}

/** Native pointerenter/leave do not bubble: entering a child from outside enters each ancestor
 * too. Including those events catches a marquee accidentally wired to a row or header. */
function pointerOn(element: Element, entering: boolean): void {
  const ancestors: Element[] = [];
  for (let parent: Element | null = element; parent; parent = parent.parentElement) ancestors.push(parent);
  const path = entering ? ancestors.reverse() : ancestors;
  act(() => {
    for (const target of path) target.dispatchEvent(new MouseEvent(entering ? "pointerenter" : "pointerleave"));
  });
}

/** Each label runs by itself; surrounding chrome leaves all labels still. Every run returns to
 * rest before the next pointer target, and the test leaves no animation or delayed work pending.
 * Where a label rests is read before any pointer moves: one clipped at its start rests scrolled to
 * its end, not at 0. */
export function expectIndependentMarquees(labels: HTMLElement[], chrome: Element[]): void {
  const rest = new Map(labels.map((label) => [label, label.scrollLeft]));
  for (const target of [...chrome, ...labels]) {
    pointerOn(target, true);
    act(() => void vi.advanceTimersByTime(MOVING_AFTER_MS));
    try {
      for (const label of labels) {
        if (label === target) expect(Math.abs(label.scrollLeft - rest.get(label)!)).toBeGreaterThan(5);
        else expect(label.scrollLeft).toBe(rest.get(label));
      }
    } finally {
      pointerOn(target, false);
      // A label that ran is measured clipped at both edges while it returns, then clipped at one
      // again at rest. Each change re-renders it, and every render schedules one more measure to
      // confirm the last; each frame is drained in an `act` of its own, and a measure that kept
      // changing would leave one pending.
      runOutTimers();
    }
    for (const label of labels) expect(label.scrollLeft).toBe(rest.get(label));
  }
  expect(vi.getTimerCount()).toBe(0);
}
