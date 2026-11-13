import { expect, it } from "vitest";

import { containsText } from "./MenuFilter";

it("finds a typed text inside an item's text, as a substring", () => {
  expect(containsText("Search API", "arch")).toBe(true);
  expect(containsText("Search API", "ch a")).toBe(true);
  expect(containsText("Search API", "docs")).toBe(false);
});

it("ignores letter case on both sides", () => {
  expect(containsText("Search API", "sEaRcH")).toBe(true);
  expect(containsText("SEARCH api", "search api")).toBe(true);
});

it("ignores the spaces around the typed text but keeps the ones inside it", () => {
  expect(containsText("Docs", "  docs ")).toBe(true);
  expect(containsText("Search API", " ch a ")).toBe(true);
  expect(containsText("SearchAPI", "ch a")).toBe(false);
});

it("matches every text while nothing, or only spaces, is typed", () => {
  expect(containsText("Docs", "")).toBe(true);
  expect(containsText("Docs", "   ")).toBe(true);
});
