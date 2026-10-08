import { describe, expect, it } from "vitest";

import type { Project, Session, SessionStatus } from "../protocol";
import { boundArchivedSessions, boundElsewhere, focusGroups, liveSessions, sortProjects, unboundSessions } from "./order";

const session = (id: string, status: SessionStatus, started_at: number, pinned = false, bound_to?: string): Session => ({
  id,
  agent: "claude",
  console_id: "c",
  project_id: "p",
  host_id: "h",
  role: "project",
  origin: "user",
  title: id,
  status,
  has_conversation: false,
  bound_to,
  started_at,
  pinned,
});

const project = (id: string, name: string, pinned = false): Project => ({
  id,
  console_id: "c",
  host_id: "h",
  name,
  path: `/${name}`,
  source: "local",
  claude_trust_consent: false,
  pinned,
  tags: [],
});

describe("liveSessions", () => {
  it("puts pinned first, then a raised hand, work, idle and interrupted, newest first within a status", () => {
    const ordered = liveSessions([
      session("old-idle", "idle", 1),
      session("interrupted", "interrupted", 9),
      session("new-idle", "idle", 5),
      session("working", "working", 2),
      session("archived", "archived", 10),
      session("waiting", "waiting_user", 3),
      session("pinned", "interrupted", 0, true),
    ]);
    expect(ordered.map((s) => s.id)).toEqual(["pinned", "waiting", "working", "new-idle", "old-idle", "interrupted"]);
  });
});

describe("boundArchivedSessions", () => {
  it("keeps only the archived sessions bound to the given console session", () => {
    const sessions = [
      session("bound-archived", "archived", 1, false, "s-console"),
      session("bound-live", "idle", 2, false, "s-console"),
      session("unbound-archived", "archived", 3),
      session("other-owner-archived", "archived", 4, false, "s-other"),
    ];
    expect(boundArchivedSessions(sessions, "s-console").map((s) => s.id)).toEqual(["bound-archived"]);
  });
});

describe("sortProjects", () => {
  it("puts pinned first, then a raised hand, work, a running session, then the inactive ones by name", () => {
    const sessions: Record<string, Session[]> = {
      b: [session("b1", "working", 1)],
      c: [session("c1", "waiting_user", 1)],
      d: [session("d1", "idle", 1)],
    };
    const ordered = sortProjects(
      [project("d", "delta"), project("a10", "item 10"), project("b", "beta"), project("a2", "item 2"), project("c", "charlie"), project("z", "zulu", true)],
      (p) => sessions[p.id] ?? [],
    );
    expect(ordered.map((p) => p.id)).toEqual(["z", "c", "b", "d", "a2", "a10"]);
  });
});

describe("unboundSessions", () => {
  it("keeps the sessions with no binding, whatever their status", () => {
    const sessions = [session("unbound", "idle", 1), session("archived", "archived", 2), session("bound", "idle", 3, false, "s-console")];
    expect(unboundSessions(sessions).map((s) => s.id)).toEqual(["unbound", "archived"]);
  });
});

describe("boundElsewhere", () => {
  const owner = (id: string, started_at: number): Session => ({ ...session(id, "idle", started_at), role: "console", project_id: undefined });
  const owners = new Map([owner("hub-1", 1), owner("hub-2", 2), { ...owner("hub-old", 3), status: "archived" as const }].map((o) => [o.id, o]));

  it.each([
    { name: "counts the live bound sessions and names each owner once, in the sidebar's order", sessions: [session("a", "idle", 1, false, "hub-1"), session("b", "working", 2, false, "hub-2"), session("c", "idle", 3, false, "hub-1")], expected: { count: 3, owners: ["hub-2", "hub-1"] } },
    { name: "leaves out unbound, archived, unknown-owner and archived-owner sessions", sessions: [session("a", "idle", 1), session("b", "archived", 2, false, "hub-1"), session("c", "idle", 3, false, "gone"), session("d", "idle", 4, false, "hub-old")], expected: undefined },
  ])("$name", ({ sessions, expected }) => {
    const summary = boundElsewhere(sessions, owners);
    expect(summary && { count: summary.count, owners: summary.owners.map((o) => o.id) }).toEqual(expected);
  });
});

describe("focusGroups", () => {
  it("lists the projects with a live session bound to the console session, each with only those, in project order", () => {
    const in_ = (id: string, project: string, status: SessionStatus, bound_to?: string): Session => ({ ...session(id, status, 1, false, bound_to), project_id: project });
    const groups = focusGroups(
      [project("a", "alpha"), project("b", "beta"), project("c", "charlie"), project("d", "delta")],
      [
        in_("a-mine", "a", "idle", "hub"),
        in_("a-other", "a", "idle", "other"),
        in_("b-mine", "b", "working", "hub"),
        in_("b-unbound", "b", "working"),
        in_("c-archived", "c", "archived", "hub"),
      ],
      "hub",
    );
    expect(groups.map((g) => [g.project.id, g.sessions.map((s) => s.id)])).toEqual([
      ["b", ["b-mine"]],
      ["a", ["a-mine"]],
    ]);
  });
});
