import { expect, it, vi } from "vitest";

import { FIT } from "./zoom";
import { ZoomGroup, type ZoomMember } from "./zoomGroup";

/** A member with the given bounds, whose scroller holds `content` in a `view` and is scrolled to `scroll`, so it can
 * scroll `content - view` along each axis. Writes to the scroll position are counted, and `range` stands for the browser
 * holding a position written beyond what it can scroll at the edge. */
function member({
  priority = 0,
  fit = 0.5,
  max = 32,
  content = { width: 2000, height: 1000 },
  view = { width: 400, height: 300 },
  scroll = { left: 0, top: 0 },
  range = { left: Infinity, top: Infinity },
}: {
  priority?: number;
  fit?: number;
  max?: number;
  content?: { width: number; height: number };
  view?: { width: number; height: number };
  scroll?: { left: number; top: number };
  range?: { left: number; top: number };
} = {}): ZoomMember & { writes: { left: number; top: number } } {
  const position = { ...scroll };
  const writes = { left: 0, top: 0 };
  const scroller = {
    clientWidth: view.width,
    clientHeight: view.height,
    scrollWidth: content.width,
    scrollHeight: content.height,
    get scrollLeft() {
      return position.left;
    },
    set scrollLeft(value: number) {
      position.left = Math.min(range.left, Math.max(0, value));
      writes.left += 1;
    },
    get scrollTop() {
      return position.top;
    },
    set scrollTop(value: number) {
      position.top = Math.min(range.top, Math.max(0, value));
      writes.top += 1;
    },
  } as unknown as HTMLElement;
  return { priority, bounds: { fit, max }, scroller, writes };
}

const ids = () => ({ before: Symbol("before"), after: Symbol("after") });

it("has no leader, bounds or scale before a member registers", () => {
  const group = new ZoomGroup();
  expect(group.lead()).toBeUndefined();
  expect(group.bounds()).toBeNull();
  expect(group.leadScale()).toBeUndefined();
  expect(group.scaleOf(Symbol("nobody"))).toBeUndefined();
  expect(group.current()).toEqual(FIT);
  // Nothing to hold a zoom within.
  group.setZoom({ mode: "scale", scale: 3 });
  expect(group.current()).toEqual(FIT);
});

it("leads with the registered member of the highest priority, and hands the lead on as it leaves", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0 }));
  expect(group.lead()).toBe(before);
  group.update(after, member({ priority: 1 }));
  expect(group.lead()).toBe(after);
  group.update(after, null);
  expect(group.lead()).toBe(before);
  group.update(before, null);
  expect(group.lead()).toBeUndefined();
});

it("opens fitted, each member at its own fitted scale", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5 }));
  group.update(after, member({ priority: 1, fit: 0.25 }));
  expect(group.current()).toEqual(FIT);
  expect(group.scaleOf(before)).toBe(0.5);
  expect(group.scaleOf(after)).toBe(0.25);
  expect(group.leadScale()).toBe(0.25);
});

it("tells the zoom in the leader's scale and draws each other member in proportion to its own fitted scale", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5 }));
  group.update(after, member({ priority: 1, fit: 0.25 }));
  group.setZoom({ mode: "scale", scale: 1 });
  expect(group.current()).toEqual({ mode: "scale", scale: 1 });
  expect(group.leadScale()).toBe(1);
  expect(group.scaleOf(after)).toBe(1);
  expect(group.scaleOf(before)).toBeCloseTo(2);
  group.setZoom({ mode: "scale", scale: 0.5 });
  expect(group.scaleOf(before)).toBeCloseTo(1);
  expect(group.scaleOf(after)).toBeCloseTo(0.5);
});

it("bounds the zoom from the leader's fitted scale to where the first member reaches its largest scale, in the leader's terms", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5, max: 32 }));
  group.update(after, member({ priority: 1, fit: 0.25, max: 16.384 }));
  // Before's limit is 32 * 0.25 / 0.5 = 16; After's own is 16.384.
  expect(group.bounds()).toEqual({ fit: 0.25, max: 16 });

  group.update(before, member({ priority: 0, fit: 0.5, max: 40 }));
  expect(group.bounds()!.max).toBeCloseTo(16.384);

  // Only the leader's fitted scale is the smallest, and the largest is never below it.
  const lone = new ZoomGroup();
  lone.update(before, member({ priority: 0, fit: 0.5, max: 1 }));
  lone.update(after, member({ priority: 1, fit: 2, max: 1 }));
  expect(lone.bounds()).toEqual({ fit: 2, max: 2 });
});

it("holds the zoom within the bounds, and fits again on landing on the fitted scale", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5, max: 32 }));
  group.update(after, member({ priority: 1, fit: 0.25, max: 32 }));
  group.setZoom({ mode: "scale", scale: 500 });
  expect(group.current()).toEqual({ mode: "scale", scale: 16 });
  group.setZoom({ mode: "scale", scale: 0.01 });
  expect(group.current()).toEqual(FIT);
  group.setZoom({ mode: "scale", scale: 2 });
  group.setZoom({ mode: "scale", scale: 0.25 });
  expect(group.current()).toEqual(FIT);
  group.setZoom({ mode: "scale", scale: 2 });
  group.setZoom(FIT);
  expect(group.current()).toEqual(FIT);
  expect(group.scaleOf(before)).toBe(0.5);
});

it("tells its subscribers when it changes, and only then", () => {
  const group = new ZoomGroup();
  const listener = vi.fn();
  const stop = group.subscribe(listener);
  const { before, after } = ids();

  const first = group.getSnapshot();
  group.update(before, member({ priority: 0 }));
  expect(listener).toHaveBeenCalledTimes(1);
  expect(group.getSnapshot()).not.toBe(first);

  // The same bounds and priority again is no change, even for a new set of elements.
  group.update(before, member({ priority: 0 }));
  expect(listener).toHaveBeenCalledTimes(1);

  group.update(before, member({ priority: 0, fit: 0.4 }));
  expect(listener).toHaveBeenCalledTimes(2);
  group.update(after, member({ priority: 1 }));
  expect(listener).toHaveBeenCalledTimes(3);

  group.setZoom({ mode: "scale", scale: 2 });
  expect(listener).toHaveBeenCalledTimes(4);
  const snapshot = group.getSnapshot();
  group.setZoom({ mode: "scale", scale: 2 });
  expect(listener).toHaveBeenCalledTimes(4);
  expect(group.getSnapshot()).toBe(snapshot);
  group.setZoom(FIT);
  expect(listener).toHaveBeenCalledTimes(5);
  group.setZoom(FIT);
  expect(listener).toHaveBeenCalledTimes(5);

  group.update(after, null);
  expect(listener).toHaveBeenCalledTimes(6);
  group.update(after, null);
  expect(listener).toHaveBeenCalledTimes(6);

  stop();
  group.update(after, member({ priority: 1 }));
  expect(listener).toHaveBeenCalledTimes(6);
});

it("scrolls every other member to the same fraction of its own scroll range as the member that scrolled", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  // After scrolls 1600 across and 700 down, and stands halfway along both. Before is larger: 2600 across, 1200 down.
  const from = member({ priority: 1, content: { width: 2000, height: 1000 }, scroll: { left: 800, top: 350 } });
  const other = member({ priority: 0, content: { width: 3000, height: 1500 } });
  group.update(after, from);
  group.update(before, other);
  group.syncScroll(after);
  expect(other.scroller.scrollLeft).toBeCloseTo(0.5 * 2600);
  expect(other.scroller.scrollTop).toBeCloseTo(0.5 * 1200);
  // The member that scrolled is not touched.
  expect(from.writes).toEqual({ left: 0, top: 0 });
  expect(from.scroller.scrollLeft).toBe(800);

  // And the other way round: Before a quarter along its width and three quarters down its height.
  other.scroller.scrollLeft = 0.25 * 2600;
  other.scroller.scrollTop = 0.75 * 1200;
  group.syncScroll(before);
  expect(from.scroller.scrollLeft).toBeCloseTo(0.25 * 1600);
  expect(from.scroller.scrollTop).toBeCloseTo(0.75 * 700);
});

it("brings every side to its edge when one reaches its own, whatever the sizes", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const small = member({ priority: 1, content: { width: 1000, height: 500 }, scroll: { left: 600, top: 200 } });
  const large = member({ priority: 0, content: { width: 5000, height: 4000 } });
  group.update(after, small);
  group.update(before, large);
  // The far edges: 1000 - 400 = 600 across, 500 - 300 = 200 down.
  group.syncScroll(after);
  expect(large.scroller.scrollLeft).toBeCloseTo(5000 - 400);
  expect(large.scroller.scrollTop).toBeCloseTo(4000 - 300);
  // The near edges.
  small.scroller.scrollLeft = 0;
  small.scroller.scrollTop = 0;
  group.syncScroll(after);
  expect(large.scroller.scrollLeft).toBe(0);
  expect(large.scroller.scrollTop).toBe(0);
});

it("lines up two images of the same size pixel for pixel", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const from = member({ priority: 1, scroll: { left: 123, top: 45 } });
  const other = member({ priority: 0 });
  group.update(after, from);
  group.update(before, other);
  group.syncScroll(after);
  expect(other.scroller.scrollLeft).toBeCloseTo(123);
  expect(other.scroller.scrollTop).toBeCloseTo(45);
});

it("brings every other member to the middle of its range along an axis the member that scrolled cannot scroll, its image being centred", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  // From fits across (under a pixel of range) and scrolls down.
  const from = member({ priority: 1, content: { width: 400.5, height: 1000 }, scroll: { left: 0, top: 350 } });
  const other = member({ priority: 0, content: { width: 3000, height: 1500 }, scroll: { left: 700, top: 0 } });
  group.update(after, from);
  group.update(before, other);
  group.syncScroll(after);
  expect(other.scroller.scrollLeft).toBeCloseTo(0.5 * 2600);
  expect(other.scroller.scrollTop).toBeCloseTo(0.5 * 1200);

  // Both axes without a range: the other goes to the middle of both.
  const flat = member({ priority: 1, content: { width: 300, height: 200 } });
  const third = member({ priority: 0, content: { width: 3000, height: 1500 } });
  const second = new ZoomGroup();
  second.update(after, flat);
  second.update(before, third);
  second.syncScroll(after);
  expect(third.scroller.scrollLeft).toBeCloseTo(0.5 * 2600);
  expect(third.scroller.scrollTop).toBeCloseTo(0.5 * 1200);
});

it("leaves a member with no range along an axis where it is", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const reader = member({ priority: 1, scroll: { left: 800, top: 0 } });
  const fixed = member({ priority: 0, content: { width: 400, height: 1500 } });
  group.update(after, reader);
  group.update(before, fixed);
  group.syncScroll(after);
  expect(fixed.scroller.scrollLeft).toBe(0);
  expect(fixed.writes.left).toBe(0);
});

it("leaves a member that is already within a pixel of the place alone, so scroll events settle", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const from = member({ priority: 1, scroll: { left: 300, top: 100 } });
  // The same shape and size, a hair off the same place.
  const other = member({ priority: 0, scroll: { left: 300.6, top: 99.5 } });
  group.update(after, from);
  group.update(before, other);
  group.syncScroll(after);
  expect(other.writes).toEqual({ left: 0, top: 0 });
  expect(other.scroller.scrollLeft).toBe(300.6);
  // Syncing back does not move the source either: there is no feedback.
  group.syncScroll(before);
  expect(from.writes).toEqual({ left: 0, top: 0 });

  // A whole pixel off is moved.
  other.scroller.scrollLeft = 302;
  group.syncScroll(after);
  expect(other.scroller.scrollLeft).toBeCloseTo(300);
});

it("does nothing to scroll for a group of one, or for a member it does not know", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const only = member({ priority: 0, scroll: { left: 50, top: 50 } });
  group.update(before, only);
  group.syncScroll(before);
  expect(only.writes).toEqual({ left: 0, top: 0 });
  group.update(after, member({ priority: 1 }));
  expect(() => group.syncScroll(Symbol("nobody"))).not.toThrow();
  expect(only.writes).toEqual({ left: 0, top: 0 });
});

/** The reader scrolls `scroller` to `to`, and its scroll event reaches the group. */
function readerScrolls(group: ZoomGroup, id: symbol, scroller: HTMLElement, to: { left?: number; top?: number }): void {
  if (to.left !== undefined) scroller.scrollLeft = to.left;
  if (to.top !== undefined) scroller.scrollTop = to.top;
  group.scrolled(id);
}

it("syncs only the axes it is asked to", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const from = member({ priority: 1, scroll: { left: 300, top: 100 } });
  const other = member({ priority: 0, scroll: { left: 7, top: 9 } });
  group.update(after, from);
  group.update(before, other);
  group.syncScroll(after, { x: true, y: false });
  expect(other.scroller.scrollLeft).toBeCloseTo(300);
  expect(other.scroller.scrollTop).toBe(9);
  other.scroller.scrollLeft = 7;
  group.syncScroll(after, { x: false, y: true });
  expect(other.scroller.scrollLeft).toBe(7);
  expect(other.scroller.scrollTop).toBeCloseTo(100);
});

it("does not pass a follower's own scroll event back: it is an echo of where the group put it", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  // The reader scrolls Before; After is held by the browser at 100 across, short of where the group puts it.
  const reader = member({ priority: 0, range: { left: 1000, top: 500 } });
  const follower = member({ priority: 1, range: { left: 100, top: 500 } });
  group.update(before, reader);
  group.update(after, follower);

  readerScrolls(group, before, reader.scroller, { left: 600 });
  // The follower goes as far as it can, and stays at its own edge.
  expect(follower.scroller.scrollLeft).toBe(100);
  // Its scroll event (at the position it was left at) does not pull the reader back to match.
  const readerWrites = { ...reader.writes };
  group.scrolled(after);
  expect(reader.scroller.scrollLeft).toBe(600);
  expect(reader.writes).toEqual(readerWrites);
  expect(follower.scroller.scrollLeft).toBe(100);

  // The image with the most room sets how far the group scrolls: the reader keeps going.
  readerScrolls(group, before, reader.scroller, { left: 800 });
  expect(reader.scroller.scrollLeft).toBe(800);
  expect(follower.scroller.scrollLeft).toBe(100);
});

it("treats a scroll event within a pixel of where the group left a member as that echo, and any other as the reader's", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const first = member({ priority: 0, content: { width: 2000, height: 1000 } });
  const second = member({ priority: 1, content: { width: 2000, height: 1000 } });
  group.update(before, first);
  group.update(after, second);
  readerScrolls(group, before, first.scroller, { left: 400, top: 200 });
  expect(second.scroller.scrollLeft).toBeCloseTo(400);

  // A hair off the echo position is still the echo.
  second.scroller.scrollLeft = 400.5;
  group.scrolled(after);
  expect(first.scroller.scrollLeft).toBe(400);

  // The reader takes hold of the second member and moves it: now the first follows.
  readerScrolls(group, after, second.scroller, { left: 900 });
  expect(first.scroller.scrollLeft).toBeCloseTo(900);
});

it("syncs only the axes that moved since the member's previous scroll event", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const reader = member({ priority: 0, content: { width: 2000, height: 1000 } });
  const follower = member({ priority: 1, content: { width: 2000, height: 1000 } });
  group.update(before, reader);
  group.update(after, follower);

  // The first event has no earlier position to compare with: both axes follow.
  readerScrolls(group, before, reader.scroller, { left: 300, top: 150 });
  expect(follower.scroller.scrollLeft).toBeCloseTo(300);
  expect(follower.scroller.scrollTop).toBeCloseTo(150);

  // The follower is put somewhere else vertically, by a zoom, say; scrolling across leaves it there.
  follower.scroller.scrollTop = 40;
  readerScrolls(group, before, reader.scroller, { left: 500 });
  expect(follower.scroller.scrollLeft).toBeCloseTo(500);
  expect(follower.scroller.scrollTop).toBe(40);

  // And scrolling down leaves where it stands across.
  follower.scroller.scrollLeft = 55;
  readerScrolls(group, before, reader.scroller, { top: 250 });
  expect(follower.scroller.scrollTop).toBeCloseTo(250);
  expect(follower.scroller.scrollLeft).toBe(55);
});

it("does nothing for a scroll event of a member it does not know", () => {
  const group = new ZoomGroup();
  expect(() => group.scrolled(Symbol("nobody"))).not.toThrow();
});

it("holds a chosen zoom within the new bounds as the leader's view grows past it", () => {
  const group = new ZoomGroup();
  const { after } = ids();
  group.update(after, member({ priority: 1, fit: 0.25 }));
  group.setZoom({ mode: "scale", scale: 0.33 });
  expect(group.current()).toEqual({ mode: "scale", scale: 0.33 });
  // The view grows: fitting is now 50%, above the 33% chosen.
  group.update(after, member({ priority: 1, fit: 0.5 }));
  expect(group.current()).toEqual(FIT);
  expect(group.leadScale()).toBe(0.5);
});

it("keeps a chosen zoom that the new bounds still hold, without converting it, when the same leader updates", () => {
  const group = new ZoomGroup();
  const { after } = ids();
  group.update(after, member({ priority: 1, fit: 0.25 }));
  group.setZoom({ mode: "scale", scale: 1 });
  group.update(after, member({ priority: 1, fit: 0.5 }));
  expect(group.current()).toEqual({ mode: "scale", scale: 1 });
  expect(group.leadScale()).toBe(1);
  // The upper bound is held too.
  group.setZoom({ mode: "scale", scale: 16 });
  group.update(after, member({ priority: 1, fit: 0.5, max: 4 }));
  expect(group.current()).toEqual({ mode: "scale", scale: 4 });
});

it("keeps the old leader's drawn size when a member joins and leads, converting the chosen scale to the new leader's terms", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5 }));
  group.setZoom({ mode: "scale", scale: 1 });
  expect(group.scaleOf(before)).toBe(1);

  group.update(after, member({ priority: 1, fit: 0.25 }));
  expect(group.lead()).toBe(after);
  // 1 * 0.25 / 0.5: Before is still drawn at 1, as the follower now.
  expect(group.leadScale()).toBeCloseTo(0.5);
  expect(group.scaleOf(before)).toBeCloseTo(1);
  expect(group.scaleOf(after)).toBeCloseTo(0.5);

  // And back, when the leader leaves.
  group.update(after, null);
  expect(group.lead()).toBe(before);
  expect(group.leadScale()).toBeCloseTo(1);
});

it("holds a converted zoom within the new bounds, and leaves fitting alone", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  group.update(before, member({ priority: 0, fit: 0.5, max: 32 }));
  group.setZoom({ mode: "scale", scale: 32 });
  // 32 converts to 16, above what the new leader can reach.
  group.update(after, member({ priority: 1, fit: 0.25, max: 8 }));
  expect(group.current()).toEqual({ mode: "scale", scale: 8 });

  const fitted = new ZoomGroup();
  fitted.update(before, member({ priority: 0, fit: 0.5 }));
  fitted.update(after, member({ priority: 1, fit: 0.25 }));
  expect(fitted.current()).toEqual(FIT);
  expect(fitted.scaleOf(before)).toBe(0.5);
  expect(fitted.scaleOf(after)).toBe(0.25);
});

it("forgets a removed member's echo and position, so a scroll event at the old echo is the reader's", () => {
  const group = new ZoomGroup();
  const { before, after } = ids();
  const first = member({ priority: 0 });
  const second = member({ priority: 1 });
  group.update(before, first);
  group.update(after, second);
  readerScrolls(group, before, first.scroller, { left: 400 });
  expect(second.scroller.scrollLeft).toBeCloseTo(400);

  group.update(after, null);
  group.update(after, second);
  first.scroller.scrollLeft = 0;
  // Second is where the group left it, but that was before it left: this scroll event is the reader's now.
  group.scrolled(after);
  expect(first.scroller.scrollLeft).toBeCloseTo(400);
});
