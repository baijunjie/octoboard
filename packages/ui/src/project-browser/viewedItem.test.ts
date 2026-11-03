import { describe, expect, it } from "vitest";

import { compareFilePaths } from "./tree";
import { neighboursIn, rereadFor, type Evidence, type Shown } from "./viewedItem";

describe("neighboursIn", () => {
  const files = ["a/x.ts", "a/y.ts", "b.ts"];
  const near = (current: string) => neighboursIn(files, current, (a, b) => a === b, compareFilePaths);

  it.each([
    ["the first item", "a/x.ts", { previous: undefined, next: "a/y.ts" }],
    ["the last item", "b.ts", { previous: "a/y.ts", next: undefined }],
    ["an item the list does not have", "a/xa.ts", { previous: "a/x.ts", next: "a/y.ts" }],
    ["an item that would come before the whole list", "a/0.ts", { previous: undefined, next: "a/x.ts" }],
    ["an item that would come after the whole list", "z.ts", { previous: "b.ts", next: undefined }],
  ])("stops at the ends and places %s among the rest", (_, current, expected) => {
    expect(near(current)).toEqual(expected);
  });

  it("finds an item by the identity the caller names, not by reference", () => {
    const items = [{ key: "a" }, { key: "b" }];
    expect(neighboursIn(items, { key: "a" }, (x, y) => x.key === y.key, () => 1)).toEqual({ previous: undefined, next: items[1] });
  });
});

describe("rereadFor", () => {
  const listed = (version: string | null): Evidence => ({ state: "listed", version });

  it.each<[string, Shown, Evidence, Evidence | undefined, boolean]>([
    ["a body listed at its own version", { state: "read", version: "v1" }, listed("v1"), undefined, false],
    ["a body rewritten at the same size", { state: "read", version: "v1" }, listed("v2"), listed("v1"), true],
    ["a body listed with no version to compare", { state: "read", version: "v1" }, listed(null), listed("v1"), false],
    ["a body whose item is gone", { state: "read", version: "v1" }, { state: "gone" }, listed("v1"), true],
    ["a body whose directory cannot be listed", { state: "read", version: "v1" }, { state: "unreadable" }, listed("v1"), true],
    ["a body the lists have nothing new about", { state: "read", version: "v1" }, { state: "gone" }, { state: "gone" }, false],
    ["a failure whose item is listed again", { state: "error" }, listed("v2"), { state: "gone" }, true],
    ["a failure whose item is gone", { state: "error" }, { state: "gone" }, listed("v2"), false],
    ["a failure the lists have nothing new about", { state: "error" }, listed("v2"), listed("v2"), false],
    ["an item that kept changing, at every listing", { state: "error", changing: true }, listed("v2"), listed("v2"), true],
    ["an item that kept changing and is gone", { state: "error", changing: true }, { state: "gone" }, listed("v2"), false],
    ["a read still out", { state: "loading" }, { state: "gone" }, undefined, false],
    ["lists that say nothing yet", { state: "read", version: "v1" }, { state: "unknown" }, listed("v1"), false],
  ])("reads %s again: %s", (_, shown, evidence, previous, expected) => {
    expect(rereadFor(shown, evidence, previous, true)).toBe(expected);
  });

  it("reads an item that kept changing again only at a fresh listing", () => {
    expect(rereadFor({ state: "error", changing: true }, listed("v2"), listed("v2"), false)).toBe(false);
  });

  describe("with news only", () => {
    const read = (shown: Shown, evidence: Evidence, previous: Evidence | undefined) => rereadFor(shown, evidence, previous, true, true);

    it("reads a body or a failure again on any news against what was seen", () => {
      expect(read({ state: "read" }, listed("v2"), listed("v1"))).toBe(true);
      expect(read({ state: "read" }, { state: "gone" }, listed("v1"))).toBe(true);
      expect(read({ state: "error" }, { state: "gone" }, listed("v1"))).toBe(true);
      expect(read({ state: "read" }, listed("v1"), listed("v1"))).toBe(false);
    });

    it("takes nothing as news before something was seen", () => {
      expect(read({ state: "read" }, listed("v1"), undefined)).toBe(false);
    });
  });
});
