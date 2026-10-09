import { describe, expect, it } from "vitest";

import { GROUPS, SCENARIOS } from ".";

describe("gallery scenarios", () => {
  it("have unique ids, which the URL names them by", () => {
    const ids = SCENARIOS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("leave no group empty, which would list a bare heading", () => {
    const used = new Set(SCENARIOS.map((s) => s.group));
    expect(GROUPS.map((g) => g.title).filter((title) => !used.has(title))).toEqual([]);
  });
});
