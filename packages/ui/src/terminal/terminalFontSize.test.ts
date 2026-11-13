// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";

import {
  clampTerminalFontSize,
  DEFAULT_TERMINAL_FONT_SIZE,
  MAX_TERMINAL_FONT_SIZE,
  MIN_TERMINAL_FONT_SIZE,
  parseTerminalFontSize,
  resetTerminalFontSize,
  stepTerminalFontSize,
  terminalFontSize,
} from "./terminalFontSize";

afterEach(() => resetTerminalFontSize());

it("starts at xterm's own default size, 15", () => {
  expect(DEFAULT_TERMINAL_FONT_SIZE).toBe(15);
  expect(terminalFontSize.get()).toBe(15);
});

it.each([
  [4, MIN_TERMINAL_FONT_SIZE],
  [99, MAX_TERMINAL_FONT_SIZE],
  [16.4, 16],
  [16.5, 17],
  [Number.NaN, DEFAULT_TERMINAL_FONT_SIZE],
  [Number.POSITIVE_INFINITY, DEFAULT_TERMINAL_FONT_SIZE],
])("holds %s within the range as %s", (size, expected) => {
  expect(clampTerminalFontSize(size)).toBe(expected);
});

it.each([
  ["18", 18],
  ["3", MIN_TERMINAL_FONT_SIZE],
  [null, DEFAULT_TERMINAL_FONT_SIZE],
  ["", DEFAULT_TERMINAL_FONT_SIZE],
  ["large", DEFAULT_TERMINAL_FONT_SIZE],
])("reads the stored %j as %s", (raw, expected) => {
  expect(parseTerminalFontSize(raw)).toBe(expected);
});

it("steps by whole pixels, stops at either end of the range and goes back to the default", () => {
  stepTerminalFontSize(1);
  expect(terminalFontSize.get()).toBe(16);
  stepTerminalFontSize(-2);
  expect(terminalFontSize.get()).toBe(14);
  for (let i = 0; i < 30; i++) stepTerminalFontSize(1);
  expect(terminalFontSize.get()).toBe(MAX_TERMINAL_FONT_SIZE);
  for (let i = 0; i < 30; i++) stepTerminalFontSize(-1);
  expect(terminalFontSize.get()).toBe(MIN_TERMINAL_FONT_SIZE);
  resetTerminalFontSize();
  expect(terminalFontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE);
});

it("stores a size other than the default and nothing for the default", () => {
  stepTerminalFontSize(3);
  expect(localStorage.getItem("octoboard.terminalFontSize")).toBe("18");
  resetTerminalFontSize();
  expect(localStorage.getItem("octoboard.terminalFontSize")).toBeNull();
});
