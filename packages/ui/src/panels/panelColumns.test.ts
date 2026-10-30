import { expect, it } from "vitest";

import { panelColumns } from "./panelColumns";

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
