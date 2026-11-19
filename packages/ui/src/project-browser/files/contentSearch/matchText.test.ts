import { expect, it } from "vitest";

import type { ContentMatch } from "../../../protocol";
import { MATCH_LEAD, matchSegments } from "./matchText";

const line = (fields: Partial<ContentMatch>): ContentMatch => ({ path: "a.ts", line: 1, text: "", text_start: 0, ranges: [], clipped_end: false, ...fields });

it("marks the ranges the daemon sent, leaving out a whole line's indentation", () => {
  expect(matchSegments(line({ text: "    let foo = foo;", ranges: [{ start: 8, end: 11 }, { start: 14, end: 17 }] }))).toEqual({
    segments: [
      { text: "let ", marked: false },
      { text: "foo", marked: true },
      { text: " = ", marked: false },
      { text: "foo", marked: true },
      { text: ";", marked: false },
    ],
    clippedStart: false,
    clippedEnd: false,
  });
});

it("moves ranges by where clipped text starts and holds them to the text", () => {
  // The line is clipped on both sides: the text begins at 100 in the whole line, and one range runs past the text's end.
  expect(matchSegments(line({ text: "  ab", text_start: 100, clipped_end: true, ranges: [{ start: 90, end: 101 }, { start: 103, end: 120 }] }))).toEqual({
    segments: [
      { text: " ", marked: true },
      { text: " a", marked: false },
      { text: "b", marked: true },
    ],
    clippedStart: true,
    clippedEnd: true,
  });
});

it("starts a row a little before its first match when the match would end past the room the row has", () => {
  const text = "const value = someLongFunctionName(argument) + reindex(all);";
  const start = text.indexOf("reindex");
  const match = line({ text, ranges: [{ start, end: start + 7 }] });
  // Room enough: shown from the start.
  expect(matchSegments(match, 80).clippedStart).toBe(false);
  // Too narrow: cut to start MATCH_LEAD characters before the match, with the match in view.
  const cut = matchSegments(match, 50);
  expect(cut.clippedStart).toBe(true);
  expect(cut.segments.map((segment) => segment.text).join("")).toBe(text.slice(start - MATCH_LEAD));
  expect(cut.segments.find((segment) => segment.marked)?.text).toBe("reindex");
  // A row narrower still leads with a quarter of its room.
  expect(matchSegments(match, 16).segments[0].text).toBe(text.slice(start - 4, start));
});

it("counts a tab to its tab stop and a wide character as two cells, so the match stays in the row", () => {
  // Two tabs fill 16 cells: at 20 cells the match would end past the row unless the text is cut before it.
  const tabbed = line({ text: "\t\tx = reindex(all);", text_start: 1, ranges: [{ start: 7, end: 14 }] });
  const cut = matchSegments(tabbed, 20);
  expect(cut.clippedStart).toBe(true);
  expect(cut.segments.find((segment) => segment.marked)?.text).toBe("reindex");
  // Ten CJK characters take 20 cells; the lead before the match (a quarter of 20, 5 cells) holds two of them.
  const text = "一二三四五六七八九十foo";
  const wide = matchSegments(line({ text, ranges: [{ start: 10, end: 13 }] }), 20);
  expect(wide.clippedStart).toBe(true);
  expect(wide.segments.map((segment) => segment.text).join("")).toBe("九十foo");
  // An emoji is not split: a lead of 3 cells holds one emoji of 2 cells, not half of the one before it.
  const emoji = matchSegments(line({ text: "😀😀😀😀😀😀😀😀foo", ranges: [{ start: 16, end: 19 }] }), 12);
  expect(emoji.segments.map((segment) => segment.text).join("")).toBe("😀foo");
  // Room enough counted in cells: shown in full.
  expect(matchSegments(line({ text: "一二foo", ranges: [{ start: 2, end: 5 }] }), 10).clippedStart).toBe(false);
});
