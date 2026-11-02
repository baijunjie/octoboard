import { expect, it } from "vitest";

import { DEFAULT_FIND_OPTIONS, findPattern, matchOffsets, stepMatch, type FindOptions } from "./find";

const options = (change: Partial<FindOptions> = {}): FindOptions => ({ ...DEFAULT_FIND_OPTIONS, ...change });

function offsets(query: string, text: string, change: Partial<FindOptions> = {}, limit = 100): [number, number][] {
  const pattern = findPattern(query, options(change));
  return pattern.kind === "pattern" ? matchOffsets(pattern.pattern, text, limit) : [];
}

it("asks for nothing until there is a query", () => {
  expect(findPattern("", options())).toEqual({ kind: "empty" });
});

it("matches a plain query literally and ignoring case, whatever characters it holds", () => {
  expect(offsets("a.b", "a.b axb A.B")).toEqual([[0, 3], [8, 11]]);
  expect(offsets("(x", "f(x)")).toEqual([[1, 3]]);
  expect(offsets("a-b/c", "a-b/c")).toEqual([[0, 5]]);
});

it("matches case when asked to", () => {
  expect(offsets("Ab", "ab Ab aB", { matchCase: true })).toEqual([[3, 5]]);
});

it("matches whole words only when asked to, letters beyond ASCII included", () => {
  expect(offsets("cat", "cat concat cat_ cats cat", { wholeWord: true })).toEqual([[0, 3], [21, 24]]);
  expect(offsets("é", "é aé é", { wholeWord: true })).toEqual([[0, 1], [5, 6]]);
});

it("reads the query as a regular expression when asked to", () => {
  expect(offsets("a+b", "ab aab b", { regex: true })).toEqual([[0, 2], [3, 6]]);
  expect(offsets("^b", "a\nb", { regex: true })).toEqual([[2, 3]]);
  expect(offsets("fo+", "foo FOO", { regex: true, matchCase: true })).toEqual([[0, 3]]);
  expect(offsets("a|b", "ab", { regex: true, wholeWord: true })).toEqual([]);
});

it("accepts a regular expression only the older syntax allows", () => {
  expect(offsets("a\\-b", "a-b", { regex: true })).toEqual([[0, 3]]);
});

it("marks a regular expression that does not compile invalid", () => {
  expect(findPattern("(", options({ regex: true }))).toEqual({ kind: "invalid" });
  expect(findPattern("(", options())).toMatchObject({ kind: "pattern" });
});

it("finds no empty matches", () => {
  expect(offsets("a*", "baab", { regex: true })).toEqual([[1, 3]]);
});

it("stops at the limit", () => {
  expect(offsets("a", "aaaaa", {}, 3)).toHaveLength(3);
});

it("steps through the matches and wraps around at both ends", () => {
  expect(stepMatch(0, 3, "next")).toBe(1);
  expect(stepMatch(2, 3, "next")).toBe(0);
  expect(stepMatch(0, 3, "previous")).toBe(2);
  expect(stepMatch(1, 3, "previous")).toBe(0);
  expect(stepMatch(0, 0, "next")).toBe(0);
});
