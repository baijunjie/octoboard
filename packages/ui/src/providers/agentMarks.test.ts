import { describe, expect, it } from "vitest";

import { AGENT_MARKS } from "./agentMarks";
import { BUILT_IN } from "./registry";

describe("the list of agent marks", () => {
  it("holds the mark of every built-in provider, by id, and nothing else", () => {
    expect(Object.keys(AGENT_MARKS).sort()).toEqual(BUILT_IN.map(({ id }) => id).sort());
    for (const module of BUILT_IN) expect(AGENT_MARKS[module.id], module.id).toBe(module.mark);
  });
});
