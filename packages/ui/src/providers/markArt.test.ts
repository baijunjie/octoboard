import { describe, expect, it } from "vitest";

import { antigravity } from "./antigravity";
import { claude } from "./claude";
import { markArt } from "./markArt";

describe("markArt", () => {
  it("gives a mark drawn as its path alone no art", () => {
    expect(markArt(claude.mark, "a")).toBeUndefined();
  });

  it("makes every id the art defines and refers to unique to the prefix, leaving no placeholder", () => {
    const first = markArt(antigravity.mark, "first")!;
    const second = markArt(antigravity.mark, "second")!;
    expect(first).not.toContain("{id}");
    const ids = (art: string) => [...art.matchAll(/id="([^"]+)"/g)].map((match) => match[1]);
    expect(ids(first).length).toBeGreaterThan(0);
    expect(ids(first).every((id) => id.startsWith("first-"))).toBe(true);
    for (const reference of first.matchAll(/url\(#([^)]+)\)/g)) expect(ids(first)).toContain(reference[1]);
    expect(ids(second).some((id) => ids(first).includes(id))).toBe(false);
  });
});
