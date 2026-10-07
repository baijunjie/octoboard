import { describe, expect, it } from "vitest";

import { SCENARIOS } from ".";

describe("gallery scenarios", () => {
  it("have unique ids, which the URL names them by", () => {
    const ids = SCENARIOS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
