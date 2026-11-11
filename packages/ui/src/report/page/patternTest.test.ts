import { expect, it } from "vitest";

import { testPattern } from "./patternTest";

it("matches a pattern against the whole value", () => {
  expect(testPattern("[a-z]+\\d", "abc1")).toBe("match");
  expect(testPattern("[a-z]+\\d", "abc1x")).toBe("mismatch");
  expect(testPattern("[a-z]+\\d", "1abc1")).toBe("mismatch");
  expect(testPattern("a|b", "ab")).toBe("mismatch");
});

it("takes a pattern that does not compile to match everything", () => {
  expect(testPattern("(", "anything")).toBe("match");
});

it("reads a pattern written for the older flags too", () => {
  expect(testPattern("[\\w-]+", "a-b")).toBe("match");
});
