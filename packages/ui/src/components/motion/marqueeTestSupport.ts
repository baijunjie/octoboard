import { act } from "react";
import { expect, onTestFinished, vi } from "vitest";

/** Gives every label overflow under jsdom, which lays nothing out, and enables motion. */
export function stubMarquee(): void {
  vi.useFakeTimers();
  const matchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
  onTestFinished(() => {
    window.matchMedia = matchMedia;
    vi.useRealTimers();
  });
  for (const [name, value] of [["scrollWidth", 300], ["clientWidth", 100]] as const) {
    const original = Object.getOwnPropertyDescriptor(Element.prototype, name)!;
    Object.defineProperty(Element.prototype, name, { configurable: true, get: () => value });
    onTestFinished(() => void Object.defineProperty(Element.prototype, name, original));
  }
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
    act(() => void vi.advanceTimersByTime(1400));
    try {
      for (const label of labels) {
        if (label === target) expect(Math.abs(label.scrollLeft - rest.get(label)!)).toBeGreaterThan(5);
        else expect(label.scrollLeft).toBe(rest.get(label));
      }
    } finally {
      pointerOn(target, false);
      act(() => void vi.advanceTimersByTime(5000));
      // A label that ran is measured clipped at both edges while it returns, then clipped at one
      // again at rest. Each change re-renders it, and every render schedules one more measure to
      // confirm the last; `act` commits those renders only as it exits, so each frame is drained
      // in an `act` of its own. Two settle it; a measure that kept changing would leave a frame.
      act(() => void vi.runOnlyPendingTimers());
      act(() => void vi.runOnlyPendingTimers());
      expect(vi.getTimerCount()).toBe(0);
    }
    for (const label of labels) expect(label.scrollLeft).toBe(rest.get(label));
  }
  expect(vi.getTimerCount()).toBe(0);
}
