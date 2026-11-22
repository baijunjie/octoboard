import { describe, expect, it } from "vitest";

import { quotedText } from "./quotedText";

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
