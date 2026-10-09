import { describe, expect, it } from "vitest";

import type { Project, Session, SessionStatus } from "../protocol";
import { boundArchivedSessions, boundElsewhere, focusGroups, liveSessions, notBoundToConsoleSession, sortProjects, switchStrip } from "./order";

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
  trust_consent: false,
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

describe("notBoundToConsoleSession", () => {
  it("keeps the sessions no console session owns, whatever their status", () => {
    const owners = new Map([["s-console", { ...session("s-console", "idle", 0), role: "console" as const }]]);
    const sessions = [
      session("unbound", "idle", 1),
      session("archived", "archived", 2),
      session("bound", "idle", 3, false, "s-console"),
      session("bound-to-project", "idle", 4, false, "unbound"),
    ];
    expect(notBoundToConsoleSession(sessions, owners).map((s) => s.id)).toEqual(["unbound", "archived", "bound-to-project"]);
  });
});

describe("boundElsewhere", () => {
  const owner = (id: string, started_at: number): Session => ({ ...session(id, "idle", started_at), role: "console", project_id: undefined });
  const owners = new Map([owner("hub-1", 1), owner("hub-2", 2), { ...owner("hub-old", 3), status: "archived" as const }].map((o) => [o.id, o]));

  it.each([
    { name: "counts the live bound sessions, in total and per owner, naming each owner once in the sidebar's order", sessions: [session("a", "idle", 1, false, "hub-1"), session("b", "working", 2, false, "hub-2"), session("c", "idle", 3, false, "hub-1")], expected: { count: 3, owners: [["hub-2", 1], ["hub-1", 2]] } },
    { name: "leaves out unbound, archived, unknown-owner and archived-owner sessions", sessions: [session("a", "idle", 1), session("b", "archived", 2, false, "hub-1"), session("c", "idle", 3, false, "gone"), session("d", "idle", 4, false, "hub-old")], expected: undefined },
  ])("$name", ({ sessions, expected }) => {
    const summary = boundElsewhere(sessions, owners);
    expect(summary && { count: summary.count, owners: summary.owners.map((o) => [o.owner.id, o.count]) }).toEqual(expected);
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

describe("switchStrip", () => {
  const hub = (id: string, status: SessionStatus, started_at: number, extra: Partial<Session> = {}): Session => ({
    ...session(id, status, started_at),
    project_id: undefined,
    role: "console",
    origin: "console",
    ...extra,
  });

  it("lists the console's console sessions that are not archived: pinned first, then by start, ties by id", () => {
    const strip = switchStrip(
      [
        hub("idle-old", "idle", 1),
        hub("idle-new", "idle", 2),
        hub("waiting", "waiting_user", 3),
        hub("tie-b", "idle", 4),
        hub("tie-a", "working", 4),
        hub("pinned", "interrupted", 9, { pinned: true }),
        hub("archived", "archived", 5),
        hub("elsewhere", "idle", 9, { console_id: "other" }),
        session("project-session", "working", 3),
      ],
      "c",
    );
    expect(strip.map((e) => e.consoleSession.id)).toEqual(["pinned", "idle-old", "idle-new", "waiting", "tie-a", "tie-b"]);
  });

  it("keeps its order when statuses change", () => {
    const before = [hub("a", "idle", 1), hub("b", "idle", 2), hub("c", "idle", 3)];
    const after = [hub("a", "interrupted", 1), hub("b", "waiting_user", 2), hub("c", "working", 3)];
    const ids = (sessions: Session[]) => switchStrip(sessions, "c").map((e) => e.consoleSession.id);
    expect(ids(after)).toEqual(ids(before));
  });

  it("sums a console session's activity with that of the sessions bound to it: a raised hand, then work, then nothing", () => {
    const strip = switchStrip(
      [
        hub("a", "idle", 4),
        hub("b", "idle", 3),
        hub("c", "waiting_user", 2),
        hub("d", "interrupted", 1),
        session("a-1", "working", 1, false, "a"),
        session("a-2", "waiting_user", 1, false, "a"),
        session("b-1", "working", 1, false, "b"),
        session("b-2", "archived", 1, false, "b"),
        session("d-1", "idle", 1, false, "d"),
        session("unbound", "waiting_user", 1),
      ],
      "c",
    );
    expect(Object.fromEntries(strip.map((e) => [e.consoleSession.id, e.activity]))).toEqual({
      a: "waiting",
      b: "working",
      c: "waiting",
      d: undefined,
    });
  });
});
