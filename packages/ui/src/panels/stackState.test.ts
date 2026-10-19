import { expect, it } from "vitest";

import {
  closePanel,
  openPanel,
  parseStacks,
  resetBoundary,
  resizeBoundary,
  serializeStacks,
  shownKinds,
  sharesOf,
  toggleKind,
  toggleMaximized,
  visibleKinds,
  type PanelStack,
} from "./stackState";

// A kind the app does not have yet stands in for a second real one.
const ORDER = ["files", "fake", "other"] as const;
const empty: PanelStack = { open: [], shares: {} };

const shares = (stack: PanelStack) => sharesOf(stack, shownKinds(stack, ORDER));
const sum = (values: number[]) => values.reduce((total, value) => total + value, 0);

it("stacks kinds in the stack order whichever opens first, each at most once, sharing the height", () => {
  const stack = openPanel(openPanel(openPanel(empty, "other", ORDER), "files", ORDER), "files", ORDER);
  expect(stack.open).toEqual(["files", "other"]);
  expect(shares(stack)).toEqual([0.5, 0.5]);
  expect(sum(shares(openPanel(stack, "fake", ORDER)))).toBeCloseTo(1);
});

it("gives a new panel an even share and takes it from the others in proportion", () => {
  const two = resizeBoundary(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), 0, 300, 1000, 100, ORDER);
  expect(shares(two)).toEqual([0.3, 0.7]);
  const three = openPanel(two, "other", ORDER);
  expect(shares(three).map((share) => Math.round(share * 1000))).toEqual([200, 467, 333]);
});

it("resizes a boundary within the pair, keeping their combined height and each one's minimum", () => {
  const stack = openPanel(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), "other", ORDER);
  const before = shares(stack);
  const moved = resizeBoundary(stack, 0, 450, 900, 120, ORDER);
  expect(shares(moved)[0]).toBeCloseTo(0.5);
  expect(shares(moved)[2]).toBeCloseTo(before[2]);
  expect(shares(resizeBoundary(stack, 0, 0, 900, 120, ORDER))[0]).toBeCloseTo(120 / 900);
  const top = shares(resizeBoundary(stack, 0, 9999, 900, 120, ORDER));
  expect(top[1]).toBeCloseTo(120 / 900);
  expect(top[0] + top[1]).toBeCloseTo(before[0] + before[1]);
  expect(resizeBoundary(stack, 0, 450, 200, 120, ORDER)).toBe(stack);
  expect(resizeBoundary(stack, 2, 450, 900, 120, ORDER)).toBe(stack);
});

it("resets a boundary's pair to an even split", () => {
  const stack = resizeBoundary(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), 0, 100, 1000, 50, ORDER);
  expect(shares(resetBoundary(stack, 0, ORDER))).toEqual([0.5, 0.5]);
});

it("hands a closed panel's share to the others, and closing the last leaves the stack empty", () => {
  const stack = resizeBoundary(openPanel(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), "other", ORDER), 0, 100, 600, 50, ORDER);
  const closed = closePanel(stack, "fake", ORDER);
  expect(closed.open).toEqual(["files", "other"]);
  expect(sum(shares(closed))).toBeCloseTo(1);
  expect(closed.shares.fake).toBeUndefined();
  expect(closePanel(closePanel(closed, "files", ORDER), "other", ORDER).open).toEqual([]);
});

it("shows only the maximized panel until it is restored, closed, or another panel is opened", () => {
  const stack = openPanel(openPanel(empty, "files", ORDER), "fake", ORDER);
  const maximized = toggleMaximized(stack, "fake");
  expect(visibleKinds(maximized, ORDER)).toEqual(["fake"]);
  expect(shares(maximized)).toEqual(shares(stack));
  expect(visibleKinds(toggleMaximized(maximized, "fake"), ORDER)).toEqual(["files", "fake"]);
  expect(visibleKinds(closePanel(maximized, "fake", ORDER), ORDER)).toEqual(["files"]);
  expect(visibleKinds(openPanel(maximized, "other", ORDER), ORDER)).toEqual(["files", "fake", "other"]);
});

it("makes a kind's toggle open the panel when it is not open, and close it when it is; the last one hides the pane", () => {
  const files = openPanel(empty, "files", ORDER);
  expect(toggleKind(files, "fake", ORDER).open).toEqual(["files", "fake"]);
  expect(shownKinds(toggleKind(files, "files", ORDER), ORDER)).toEqual([]);
  const both = openPanel(files, "fake", ORDER);
  expect(toggleKind(both, "files", ORDER).open).toEqual(["fake"]);
  // The pane was hidden by closing the last panel: showing it with a kind opens that one alone.
  expect(toggleKind(empty, "fake", ORDER).open).toEqual(["fake"]);
});

it("makes the toggle of a panel another's maximizing hides show it instead of closing it", () => {
  const maximized = toggleMaximized(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), "fake");
  const pressed = toggleKind(maximized, "files", ORDER);
  expect(visibleKinds(pressed, ORDER)).toEqual(["files", "fake"]);
  expect(pressed.open).toEqual(["files", "fake"]);
});

it("keeps the open kinds and shares of each owner across a reload but not the maximized panel", () => {
  const project = toggleMaximized(resizeBoundary(openPanel(openPanel(empty, "files", ORDER), "fake", ORDER), 0, 300, 1000, 100, ORDER), "fake");
  const stacks = { project, report: { open: [], shares: {} } };
  expect(parseStacks(serializeStacks(stacks))).toEqual({ project: { open: ["files"], shares: { files: 0.3 } }, report: { open: [], shares: {} } });
});

it("falls back to every kind open for an owner whose text is missing or bad, and carries a hidden old pane over as none open", () => {
  for (const raw of [null, "{", "3", "{}"]) expect(parseStacks(raw)).toEqual({ project: { open: ["files"], shares: {} }, report: { open: ["report"], shares: {} } });
  expect(parseStacks(null, true)).toEqual({ project: { open: [], shares: {} }, report: { open: [], shares: {} } });
  expect(parseStacks(JSON.stringify({ project: { open: ["report", "files"] }, report: { open: ["files"] } }))).toEqual({
    project: { open: ["files"], shares: {} },
    report: { open: [], shares: {} },
  });
});

it("evens out shares that are missing or not positive", () => {
  expect(sharesOf({ open: ["files", "fake"], shares: { files: 3 } }, ["files", "fake"])).toEqual([0.5, 0.5]);
  expect(sharesOf({ open: ["files", "fake"], shares: { files: 1, fake: 3 } }, ["files", "fake"])).toEqual([0.25, 0.75]);
});
