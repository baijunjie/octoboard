import { describe, expect, it } from "vitest";

import { tagVocabulary } from "./tagVocabulary";

describe("tagVocabulary", () => {
  it.each([
    { tagLists: [], expected: [] },
    { tagLists: [["web", "api"], ["api", "ops"]], expected: ["api", "ops", "web"] },
    { tagLists: [["Web"], ["web"]], expected: ["Web"] },
  ])("lists the distinct tags in order: $expected", ({ tagLists, expected }) => {
    expect(tagVocabulary(tagLists)).toEqual(expected);
  });
});
