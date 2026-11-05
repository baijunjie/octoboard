import { describe, expect, it } from "vitest";

import { highlightCacheKey } from "./cacheKey";

describe("highlightCacheKey", () => {
  it("is the same for the same code", () => {
    expect(highlightCacheKey("file", "a.ts", "typescript", "const a = 1;")).toBe(highlightCacheKey("file", "a.ts", "typescript", "const a = 1;"));
  });

  it("changes with the text, even at the same length", () => {
    expect(highlightCacheKey("file", "a.ts", "typescript", "const a = 1;")).not.toBe(highlightCacheKey("file", "a.ts", "typescript", "const a = 2;"));
  });

  it("changes with the kind, the name and the language", () => {
    const key = highlightCacheKey("file", "a.ts", "typescript", "x");
    expect(highlightCacheKey("diff", "a.ts", "typescript", "x")).not.toBe(key);
    expect(highlightCacheKey("file", "b.ts", "typescript", "x")).not.toBe(key);
    expect(highlightCacheKey("file", "a.ts", "javascript", "x")).not.toBe(key);
  });
});
