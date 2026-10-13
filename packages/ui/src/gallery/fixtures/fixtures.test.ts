import { describe, expect, it } from "vitest";

import { GROUPS, SCENARIOS } from ".";
import { DEFAULT_WIDTH, MIN_HEIGHT } from "../scenario";

describe("gallery scenarios", () => {
  it("have unique ids, which the URL names them by", () => {
    const ids = SCENARIOS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("write no width the gallery already opens at, which would be a scenario carrying nothing", () => {
    expect(SCENARIOS.filter((s) => s.width === DEFAULT_WIDTH).map((s) => s.id)).toEqual([]);
  });

  it("write no height below the shortest window the page takes, which it would not be shown at", () => {
    expect(SCENARIOS.filter((s) => s.height !== undefined && s.height < MIN_HEIGHT).map((s) => s.id)).toEqual([]);
  });

  it("leave no group empty, which would list a bare heading", () => {
    const used = new Set(SCENARIOS.map((s) => s.group));
    expect(GROUPS.map((g) => g.title).filter((title) => !used.has(title))).toEqual([]);
  });
});
