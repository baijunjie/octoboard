import { describe, expect, it } from "vitest";

import { quoteLead, quotedText, type QuoteScreen } from "./quotedText";
import { rowsReader } from "./screenTestSupport";

describe("quotedText", () => {
  it("puts `> ` before every line, and a bare `>` for an empty or whitespace-only line inside", () => {
    expect(quotedText("first\nsecond\n\nthird\n   \nfourth")).toBe("> first\n> second\n>\n> third\n>\n> fourth\n");
  });

  it("reads CRLF and lone CR as line breaks", () => {
    expect(quotedText("a\r\nb\rc\nd")).toBe("> a\n> b\n> c\n> d\n");
  });

  it("drops the blank lines around the text, whitespace-only ones too, and ends with exactly one line break", () => {
    expect(quotedText("\r\n \n\nhello\nworld \t\n  \r\n\n")).toBe("> hello\n> world\n");
  });

  it("drops the trailing whitespace of each line but keeps the indentation of the first and inner lines", () => {
    expect(quotedText("    indented  \n\tinner\t\nlast   ")).toBe(">     indented\n> \tinner\n> last\n");
  });

  it("gives nothing for text that is empty or only whitespace and line breaks", () => {
    expect(quotedText("")).toBe("");
    expect(quotedText(" \t\r\n\n  ")).toBe("");
  });

  it("adds to the quote marks the text already has", () => {
    expect(quotedText("> quoted\n- item")).toBe("> > quoted\n> - item\n");
  });
});

/** A screen of `rows`, as xterm's active buffer reads them: the cursor at column `cursorX` of row `cursorY` of the
 * viewport, which starts `baseY` rows into the buffer. */
const screenOf = (rows: string[], cursor: { cursorX: number; cursorY: number; baseY?: number }): QuoteScreen =>
  ({ baseY: 0, ...cursor, getLine: rowsReader(rows) });

describe("quoteLead", () => {
  it.each([
    ["an empty row", ""],
    ["a prompt mark alone", "❯ "],
    ["a different prompt mark alone", "› "],
    ["a boxed prompt mark alone", "│ ❯ "],
  ])("is nothing for %s left of the cursor", (_name, row) => {
    expect(quoteLead(screenOf([row], { cursorX: row.length, cursorY: 0 }))).toBe("");
  });

  it.each([
    ["typed text", "❯ hello"],
    ["a paste placeholder", "❯ [Pasted text #1 +3 lines]"],
    ["a digit", "› 42"],
    ["a letter of another script", "│ ❯ 你好"],
    ["a wrapped row of only punctuation, touching the cursor", "...)"],
  ])("is a line break for %s left of the cursor", (_name, row) => {
    expect(quoteLead(screenOf([row], { cursorX: row.length, cursorY: 0 }))).toBe("\n");
  });

  it("reads only what is left of the cursor", () => {
    const row = "❯ typed after";
    expect(quoteLead(screenOf([row], { cursorX: 2, cursorY: 0 }))).toBe("");
    expect(quoteLead(screenOf([row], { cursorX: 4, cursorY: 0 }))).toBe("\n");
  });

  it("is nothing with the cursor at column 0, whatever the row holds", () => {
    expect(quoteLead(screenOf(["❯ typed"], { cursorX: 0, cursorY: 0 }))).toBe("");
  });

  it("reads the cursor's row, below the scrolled-off rows by `baseY`, not the first row of the buffer", () => {
    const rows = ["old text", "more old text", "❯ ", "tail"];
    expect(quoteLead(screenOf(rows, { cursorX: 2, cursorY: 1, baseY: 1 }))).toBe("");
  });

  it("is nothing where the cursor's row does not exist", () => {
    expect(quoteLead(screenOf(["text"], { cursorX: 4, cursorY: 5 }))).toBe("");
  });
});
