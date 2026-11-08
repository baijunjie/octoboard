import { describe, expect, it } from "vitest";

import { DEFAULT_SESSION_ORDER } from "../sessions/order";
import { parseSessionOrder, serializeSessionOrder } from "./sessionOrder";

describe("parseSessionOrder", () => {
  it("takes the default for nothing stored", () => {
    expect(parseSessionOrder(null)).toEqual(DEFAULT_SESSION_ORDER);
  });

  it("reads a stored choice", () => {
    expect(parseSessionOrder('{"sessions":"started","activeProjects":"used","inactiveProjects":"used"}')).toEqual({ sessions: "started", activeProjects: "used", inactiveProjects: "used" });
  });

  it("falls back per part for a choice it does not know, and wholly for text that is no choice", () => {
    expect(parseSessionOrder('{"sessions":"started","activeProjects":"newest","inactiveProjects":"status"}')).toEqual({ ...DEFAULT_SESSION_ORDER, sessions: "started" });
    expect(parseSessionOrder('{"activeProjects":"name"}')).toEqual({ ...DEFAULT_SESSION_ORDER, activeProjects: "name" });
    expect(parseSessionOrder("not json")).toEqual(DEFAULT_SESSION_ORDER);
    expect(parseSessionOrder("7")).toEqual(DEFAULT_SESSION_ORDER);
  });
});

describe("serializeSessionOrder", () => {
  it("stores nothing for the default and round-trips the rest", () => {
    expect(serializeSessionOrder(DEFAULT_SESSION_ORDER)).toBeNull();
    const chosen = { sessions: "started", activeProjects: "name", inactiveProjects: "used" } as const;
    expect(parseSessionOrder(serializeSessionOrder(chosen))).toEqual(chosen);
    expect(parseSessionOrder(serializeSessionOrder({ ...DEFAULT_SESSION_ORDER, inactiveProjects: "used" }))).toEqual({ ...DEFAULT_SESSION_ORDER, inactiveProjects: "used" });
  });

  it("stores only the parts that differ from the default", () => {
    expect(serializeSessionOrder({ ...DEFAULT_SESSION_ORDER, inactiveProjects: "used" })).toBe('{"inactiveProjects":"used"}');
  });
});
