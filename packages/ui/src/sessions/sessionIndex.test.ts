import { expect, it } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { indexSessions, sessionsOf } from "./sessionIndex";

const hub = sessionOf("hub", "c1", undefined, "Hub", "idle");
const a1 = sessionOf("a1", "c1", "pa", "A1", "working", { bound_to: "hub" });
const a2 = sessionOf("a2", "c1", "pa", "A2", "idle");
const b1 = sessionOf("b1", "c2", "pb", "B1", "idle");
const mapOf = (...sessions: ReturnType<typeof sessionOf>[]) => new Map(sessions.map((s) => [s.id, s]));

it("groups the sessions by project and by console, and names the owners something is bound to", () => {
  const index = indexSessions(mapOf(hub, a1, a2, b1));
  expect(sessionsOf(index.byProject, "pa")).toEqual([a1, a2]);
  expect(sessionsOf(index.byProject, "missing")).toEqual([]);
  expect(sessionsOf(index.byConsole, "c1")).toEqual([hub, a1, a2]);
  expect(Array.from(index.boundOwnerIds)).toEqual(["hub"]);
});

it("keeps the group of every project an update did not touch, and the owner set while it is the same", () => {
  const before = indexSessions(mapOf(hub, a1, a2, b1));
  const after = indexSessions(mapOf(hub, { ...a1, title: "Renamed" }, a2, b1));
  expect(sessionsOf(after.byProject, "pa")).not.toBe(sessionsOf(before.byProject, "pa"));
  expect(sessionsOf(after.byProject, "pb")).toBe(sessionsOf(before.byProject, "pb"));
  expect(sessionsOf(after.byConsole, "c2")).toBe(sessionsOf(before.byConsole, "c2"));
  expect(after.boundOwnerIds).toBe(before.boundOwnerIds);
});

it("is built once for a map", () => {
  const sessions = mapOf(hub, a1);
  expect(indexSessions(sessions)).toBe(indexSessions(sessions));
});
