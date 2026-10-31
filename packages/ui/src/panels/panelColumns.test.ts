import { expect, it } from "vitest";

import { columnSplit, panelColumns } from "./panelColumns";

const panels = ["report", "terminal", "files", "processes"];

it("keeps the first of three panels full-height in the leading column", () => {
  expect(panelColumns(panels.slice(0, 3), 420, 10)).toEqual([["report"], ["terminal", "files"]]);
  expect(panelColumns(panels.slice(1), 720, 10)).toEqual([["terminal"], ["files", "processes"]]);
});

it("puts two panels beside each other only when both have at least 260px after the gap", () => {
  expect(panelColumns(["report", "processes"], 529, 10)).toEqual([["report", "processes"]]);
  expect(panelColumns(["report", "processes"], 530, 10)).toEqual([["report"], ["processes"]]);
  expect(panelColumns(["report", "processes"], 530, 12)).toEqual([["report", "processes"]]);
});

it("keeps a single panel full-height and four panels in two pairs", () => {
  expect(panelColumns([], 720, 10)).toEqual([]);
  expect(panelColumns(["report"], 720, 10)).toEqual([["report"]]);
  expect(panelColumns(panels, 420, 10)).toEqual([["report", "terminal"], ["files", "processes"]]);
});

it("clamps column widths to usable bounds and recovers the saved proportion when widened", () => {
  expect(columnSplit(700, 10, 0.75)).toEqual({ available: 690, minimum: 160, leading: 517.5 });
  expect(columnSplit(530, 10, 0.75).leading).toBe(360);
  expect(columnSplit(700, 10, 0.75).leading).toBe(517.5);
  expect(columnSplit(300, 10, 0.75)).toEqual({ available: 290, minimum: 145, leading: 145 });
  expect(columnSplit(700, 10, NaN).leading).toBe(345);
});
