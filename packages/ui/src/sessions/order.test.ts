import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import type { Project, Session, SessionStatus } from "../protocol";
import { archiveNeedsConfirmation, archivedDeletionScope, isUnusedSession, topStatus, INACTIVE_PROJECT_RANK, STATUS_RANK, archivePreviewSessions, projectRank, liveConsoleSessions, archivedSessionRows, boundArchivedSessions, boundElsewhere, boundSessionsArchivedWith, focusGroups, liveSessionRows, liveSessions, notUnderConsoleSession, sessionsInTreeOrder, sortProjects, switchStrip } from "./order";
import { FOLLOWING_GIT_WORKFLOW } from "../git/gitWorkflow";
import { delegatingSessionIds, type ShownStatusKind } from "./sessionStatus";

/** No session delegates: the sessions of a test that does not look at delegated work. */
const NONE: ReadonlySet<string> = new Set();

const session = (id: string, status: SessionStatus, started_at: number, pinned = false, bound_to: string | null = null): Session => ({
  id,
  agent: "claude",
  console_id: "c",
  project_id: "p",
  host_id: "h",
  kind: "project",
  role: bound_to ? "worker" : "coordinator",
  origin: "user",
  title: id,
  status,
  has_conversation: false,
  bound_to,
  started_at,
  pinned,
  locked: false, agent_session_id: null, colour: null, ordinal: null, account_id: null, config_dir: null, ended_at: null,
});

const project = (id: string, name: string, pinned = false): Project => ({
  id,
  console_id: "c",
  host_id: "h",
  name,
  path: `/${name}`,
  default_agent: null,
  source: "local",
  remote_url: null,
  trust_consent: false,
  pinned,
  tags: [],
  description: null,
  accounts: { claude: { kind: "inherit" }, codex: { kind: "inherit" }, grok: { kind: "inherit" } },
  launch: { claude: { model: null, effort: null }, codex: { model: null, effort: null }, grok: { model: null, effort: null } },
  git_workflow: FOLLOWING_GIT_WORKFLOW,
});

describe("liveSessions", () => {
  it("puts pinned first, then a raised hand, idle, work and interrupted, newest first within a status", () => {
    const ordered = liveSessions([
      session("old-idle", "idle", 1),
      session("interrupted", "interrupted", 9),
      session("new-idle", "idle", 5),
      session("working", "working", 2),
      session("archived", "archived", 10),
      session("waiting", "waiting_user", 3),
      session("pinned", "interrupted", 0, true),
    ], NONE, NONE);
    expect(ordered.map((s) => s.id)).toEqual(["pinned", "waiting", "new-idle", "old-idle", "working", "interrupted"]);
  });

  it("puts pinned first, then newest first, whatever the status, when the sidebar does not order by status", () => {
    const ordered = liveSessions([
      session("old-waiting", "waiting_user", 1),
      session("interrupted", "interrupted", 9),
      session("working", "working", 5),
      session("archived", "archived", 10),
      session("pinned-old", "working", 0, true),
      session("pinned-new", "interrupted", 2, true),
    ], NONE, NONE, false);
    expect(ordered.map((s) => s.id)).toEqual(["pinned-new", "pinned-old", "interrupted", "working", "old-waiting"]);
  });

  it("counts a session that is still starting as a running one, not as no session at all", () => {
    const starting = [session("starting", "starting", 1)];
    expect(liveSessions(starting, NONE, NONE).map((s) => s.id)).toEqual(["starting"]);
    expect(projectRank(starting, NONE, NONE)).not.toBe(INACTIVE_PROJECT_RANK);
    expect(topStatus(starting, NONE, NONE)).toBe("starting");
  });

  it("does not offer a console session that is still starting as an owner", () => {
    const owners = ["starting", "idle", "archived"].map((status) => ({ ...session(status, status as SessionStatus, 1), kind: "console" as const }));
    expect(liveConsoleSessions(owners, "c").map((s) => s.id)).toEqual(["idle"]);
  });
});

describe("topStatus", () => {
  it("uses the same priority as rows, regardless of pins, input order or recency", () => {
    const ordered = [
      session("waiting", "waiting_user", 1),
      session("idle", "idle", 2),
      { ...session("background", "idle", 3), background_tasks: [{ id: "watcher" }] },
      session("working", "working", 4, true),
    ];
    const kinds = ["waiting_user", "idle", "background", "working"];
    for (let index = 0; index < ordered.length; index++) {
      expect(topStatus(ordered.slice(index).reverse(), NONE, NONE)).toBe(kinds[index]);
    }
    expect(liveSessions(ordered.map((s) => ({ ...s, pinned: false })), NONE, NONE).map((s) => s.id))
      .toEqual(["waiting", "idle", "background", "working"]);
  });

  it("names every status there is but archived, each as the top one of a session in it", () => {
    const sessions: Record<Exclude<ShownStatusKind, "archived">, Session> = {
      waiting_user: session("waiting_user", "waiting_user", 1),
      dormant: session("dormant", "dormant", 1),
      paused: { ...session("paused", "idle", 1), paused: true },
      done: { ...session("done", "idle", 1), has_conversation: true },
      idle: session("idle", "idle", 1),
      delegated: session("delegated", "idle", 1),
      background: { ...session("background", "idle", 1), background_tasks: [{ id: "watcher" }] },
      working: session("working", "working", 1),
      starting: session("starting", "starting", 1),
      switching: { ...session("switching", "idle", 1), switching_account: true },
      interrupted: session("interrupted", "interrupted", 1),
    };
    const delegating = delegatingSessionIds([...Object.values(sessions), session("child", "working", 2, false, "delegated")]);
    for (const [kind, one] of Object.entries(sessions)) expect(topStatus([one], delegating, NONE)).toBe(kind);
  });

  it("ranks a session starting up or switching account where it will land, and draws it as it is", () => {
    const starting = session("starting", "starting", 3);
    const idle = session("idle", "idle", 2);
    const working = session("working", "working", 1);
    expect(topStatus([starting], NONE, NONE)).toBe("starting");
    expect(topStatus([starting, working], NONE, NONE)).toBe("starting");
    expect(topStatus([working, { ...starting, prompt_on_launch: true }], NONE, NONE)).toBe("starting");
    expect(topStatus([working, { ...starting, has_conversation: true }, session("waiting", "waiting_user", 0)], NONE, NONE)).toBe("waiting_user");
    expect(topStatus([session("interrupted", "interrupted", 9), starting], NONE, NONE)).toBe("starting");
    expect(liveSessions([working, starting, idle], NONE, NONE).map((s) => s.id)).toEqual(["starting", "idle", "working"]);
  });

  it("puts interrupted last, and leaves out archived sessions", () => {
    expect(topStatus([], NONE, NONE)).toBeUndefined();
    expect(topStatus([session("interrupted", "interrupted", 1), session("archived", "archived", 2)], NONE, NONE)).toBe("interrupted");
    expect(topStatus([session("interrupted", "interrupted", 1), session("working", "working", 0)], NONE, NONE)).toBe("working");
    expect(topStatus([session("archived", "archived", 2)], NONE, NONE)).toBeUndefined();
    expect(topStatus([{ ...session("switching", "idle", 3), switching_account: true }], NONE, NONE)).toBe("switching");
  });

  it("takes the newest of equally ranked sessions", () => {
    expect(topStatus([session("old", "working", 1), { ...session("new", "starting", 2), prompt_on_launch: true }], NONE, NONE)).toBe("starting");
    expect(topStatus([session("new", "working", 2), { ...session("old", "starting", 1), prompt_on_launch: true }], NONE, NONE)).toBe("working");
  });
});

describe("a session running in the background", () => {
  const background = { ...session("bg", "idle", 1), background_tasks: [{ id: "b1" }] };

  it("puts background before newer working sessions, keeping pins, prompts and newest-first ties", () => {
    const ordered = liveSessions([
      session("working", "working", 20),
      background,
      { ...background, id: "new-bg", started_at: 5 },
      session("idle", "idle", 3),
      session("interrupted", "interrupted", 40),
      session("waiting", "waiting_user", 4),
      session("pinned-work", "working", 0, true),
    ], NONE, NONE);
    expect(ordered.map((s) => s.id)).toEqual(["pinned-work", "waiting", "idle", "new-bg", "bg", "working", "interrupted"]);
  });

  it("ranks a project between idle and foreground work while its console still shows background work", () => {
    expect(projectRank([background], NONE, NONE)).toBeLessThan(projectRank([session("w", "working", 1)], NONE, NONE));
    expect(projectRank([background], NONE, NONE)).toBeGreaterThan(projectRank([session("i", "idle", 2)], NONE, NONE));
    expect(projectRank([background, session("i", "idle", 2)], NONE, NONE)).toBe(projectRank([session("i", "idle", 2)], NONE, NONE));
    expect(topStatus([background], NONE, NONE)).toBe("background");
    expect(topStatus([background, session("w", "working", 3)], NONE, NONE)).toBe("background");
    expect(topStatus([background, session("i", "idle", 2)], NONE, NONE)).toBe("idle");
  });
});

describe("a dormant or paused session", () => {
  const paused = { ...session("paused", "idle", 2), paused: true };
  const dormant = session("dormant", "dormant", 1);

  it("sorts after a raised hand and before done and idle, dormant ahead of paused", () => {
    const done = { ...session("done", "idle", 4), has_conversation: true };
    const ordered = liveSessions([session("idle", "idle", 5), done, paused, dormant, session("waiting", "waiting_user", 0)], NONE, NONE);
    expect(ordered.map((s) => s.id)).toEqual(["waiting", "dormant", "paused", "done", "idle"]);
    expect(topStatus([session("idle", "idle", 5), paused], NONE, NONE)).toBe("paused");
    expect(topStatus([paused, dormant], NONE, NONE)).toBe("dormant");
  });

  it("is the top status of a project over the work beneath it", () => {
    expect(topStatus([dormant, paused, session("w", "working", 3)], NONE, NONE)).toBe("dormant");
    expect(topStatus([paused, session("w", "working", 3)], NONE, NONE)).toBe("paused");
  });
});

describe("archivePreviewSessions", () => {
  it("keeps the ten newest rows by default", () => {
    const parents = Array.from({ length: 11 }, (_, index) => ({
      ...session(`parent-${index}`, "archived", index),
      ended_at: index,
    }));
    expect(archivePreviewSessions(parents).map((s) => s.id))
      .toEqual(Array.from({ length: 10 }, (_, index) => `parent-${10 - index}`));
  });

  it("lists the rows the archive lists on their own: a child whose owner is not archived, but not one under its archived owner", () => {
    const parents = Array.from({ length: 3 }, (_, index) => ({
      ...session(`parent-${index}`, "archived", 100 - index, index === 0, index === 1 ? "hub" : undefined),
      ended_at: index,
    }));
    const children = [
      { ...session("archived-owner-child", "archived", 200, false, "parent-2"), ended_at: 20 },
      { ...session("live-owner-child", "archived", 300, false, "live-owner"), ended_at: 10 },
    ];
    expect(archivePreviewSessions([...children, ...parents], 3).map((s) => s.id))
      .toEqual(["live-owner-child", "parent-2", "parent-1"]);
  });
});

describe("boundArchivedSessions", () => {
  it("keeps only the archived sessions bound to the given owner, or to an archived one bound to it", () => {
    const sessions = [
      session("bound-archived", "archived", 1, false, "s-console"),
      session("nested-archived", "archived", 5, false, "bound-archived"),
      session("nested-live-owner", "idle", 6, false, "s-console"),
      session("under-live-owner", "archived", 7, false, "nested-live-owner"),
      session("bound-live", "idle", 2, false, "s-console"),
      session("unbound-archived", "archived", 3),
      session("other-owner-archived", "archived", 4, false, "s-other"),
    ];
    expect(boundArchivedSessions(sessions, "s-console").map((s) => s.id)).toEqual(["nested-archived", "bound-archived"]);
  });
});

describe("boundSessionsArchivedWith", () => {
  it("includes running and interrupted descendants regardless of origin or an archived coordinator", () => {
    const sessions = [
      session("coordinator", "interrupted", 1, false, "s-console"),
      session("under-coordinator", "interrupted", 2, false, "coordinator"),
      session("under-coordinator-live", "idle", 3, false, "coordinator"),
      session("bound-live", "idle", 4, false, "s-console"),
      session("under-live", "interrupted", 5, false, "bound-live"),
      session("archived-coordinator", "archived", 6, false, "s-console"),
      { ...session("under-archived", "starting", 7, false, "archived-coordinator"), origin: "console" as const },
      session("unbound", "working", 8),
      session("other-owner", "working", 9, false, "elsewhere"),
      session("old-child", "archived", 10, false, "coordinator"),
    ];
    expect(boundSessionsArchivedWith(sessions, "s-console", NONE, NONE).map((s) => s.id)).toEqual(["bound-live", "coordinator", "under-archived", "under-coordinator-live", "under-live", "under-coordinator"]);
  });
});

describe("isUnusedSession", () => {
  const unused = (extra: Partial<Session>) => ({ ...session("s", "idle", 1), ...extra });

  it.each([
    ["an unbound session with no conversation", {}, false, true],
    ["a session that has had a turn", { has_conversation: true }, false, false],
    ["a session still starting", { status: "starting" }, false, false],
    ["an owner with a session bound to it", {}, true, false],
    ["a bound session the user opened", { bound_to: "h", origin: "user" }, false, true],
    ["a bound session an agent started", { bound_to: "h", origin: "console" }, false, false],
  ] as const)("%s: %s", (_, extra, hasBound, expected) => {
    expect(isUnusedSession(unused(extra), hasBound)).toBe(expected);
  });
});

describe("archiveNeedsConfirmation", () => {
  it("skips the confirmation only for a stopped session with no unarchived session under it", () => {
    const sessions = [
      session("idle", "idle", 1),
      session("interrupted", "interrupted", 2),
      session("waiting", "waiting_user", 3),
      session("working", "working", 4),
      session("owner", "idle", 5),
      session("child", "interrupted", 6, false, "owner"),
      session("owner-archived-child", "interrupted", 7),
      session("archived-child", "archived", 8, false, "owner-archived-child"),
      session("coordinator", "idle", 9),
      session("coordinator-archived-child", "archived", 10, false, "coordinator"),
      session("coordinator-grandchild", "interrupted", 11, false, "coordinator-archived-child"),
    ];
    const needs = (id: string) => archiveNeedsConfirmation(sessions.values(), sessions.find((s) => s.id === id)!);
    expect(["idle", "interrupted", "waiting", "working", "owner", "owner-archived-child", "coordinator"].map(needs)).toEqual([false, false, true, true, true, false, true]);
  });

  it("asks for an idle session with work left running in the background, which archiving ends", () => {
    const busy = { ...session("busy", "idle", 1), background_tasks: [{ id: "b1" }] };
    expect(archiveNeedsConfirmation([busy].values(), busy)).toBe(true);
  });
});

describe("sortProjects", () => {
  it("puts unarchived projects first, then pinned, activity and name", () => {
    const sessions: Record<string, Session[]> = {
      b: [session("b1", "working", 1)],
      c: [session("c1", "waiting_user", 1)],
      d: [session("d1", "idle", 1)],
    };
    const ordered = sortProjects(
      [project("d", "delta"), project("a10", "item 10"), project("b", "beta"), project("a2", "item 2"), project("c", "charlie"), project("z", "zulu", true)],
      (p) => sessions[p.id] ?? [],
      NONE, NONE,
    );
    expect(ordered.map((p) => p.id)).toEqual(["c", "d", "b", "z", "a2", "a10"]);
  });

  it("keeps interrupted sessions ahead of empty and archived-only projects, without dimming", () => {
    const sessions: Record<string, Session[]> = {
      interrupted: [session("stopped", "interrupted", 1)],
      archived: [session("old", "archived", 2)],
      pinned: [session("pinned", "interrupted", 3)],
      working: [session("busy", "working", 4)],
    };
    const ordered = sortProjects([
      project("empty", "alpha", true), project("archived", "beta"),
      project("interrupted", "zulu"), project("pinned", "pinned", true), project("working", "working"),
    ], (p) => sessions[p.id] ?? [], NONE, NONE);
    expect(ordered.map((p) => p.id)).toEqual(["pinned", "working", "interrupted", "empty", "archived"]);
    expect(projectRank(sessions.interrupted, NONE, NONE)).toBeLessThan(INACTIVE_PROJECT_RANK);
    expect(projectRank(sessions.archived, NONE, NONE)).toBe(INACTIVE_PROJECT_RANK);
    expect(projectRank([], NONE, NONE)).toBe(INACTIVE_PROJECT_RANK);
  });

  it("keeps active projects first and pinned first within each group, but ranks none by status, when the sidebar does not order by status", () => {
    const sessions: Record<string, Session[]> = {
      b: [session("b1", "working", 1)],
      c: [session("c1", "waiting_user", 1)],
      d: [session("d1", "interrupted", 1)],
      p: [session("p1", "interrupted", 1)],
      x: [session("x1", "archived", 1)],
    };
    const ordered = sortProjects(
      [project("d", "delta"), project("x", "xray"), project("b", "beta"), project("z", "zulu", true), project("c", "charlie"), project("p", "papa", true), project("a", "alpha")],
      (p) => sessions[p.id] ?? [],
      NONE, NONE,
      false,
    );
    expect(ordered.map((p) => p.id)).toEqual(["p", "b", "c", "d", "z", "a", "x"]);
  });
});

describe("sessionsInTreeOrder", () => {
  it("walks the projects and sessions in the order the sidebar shows, with or without ranking by status", () => {
    const consoles = [consoleOf("c", "Main")];
    const projects = [project("a", "alpha"), project("b", "beta")];
    const sessions = [
      { ...session("a-old", "idle", 1), project_id: "a" },
      { ...session("a-new", "working", 2), project_id: "a" },
      { ...session("b-waiting", "waiting_user", 3), project_id: "b" },
    ];
    const ids = (byStatus: boolean) => sessionsInTreeOrder(consoles, projects, sessions, () => true, NONE, NONE, byStatus).map((s) => s.id);
    expect(ids(true)).toEqual(["b-waiting", "a-old", "a-new"]);
    expect(ids(false)).toEqual(["a-new", "a-old", "b-waiting"]);
  });
});

describe("notUnderConsoleSession", () => {
  it("keeps the sessions no console session is above, whatever their status, and drops a bound coordinator with its team", () => {
    const owners = new Map([["s-console", { ...session("s-console", "idle", 0), kind: "console" as const }]]);
    const sessions = [
      session("unbound", "idle", 1),
      session("archived", "archived", 2),
      session("coordinator", "idle", 3, false, "s-console"),
      session("under-coordinator", "idle", 4, false, "coordinator"),
      session("bound-to-project", "idle", 5, false, "unbound"),
    ];
    expect(notUnderConsoleSession(sessions, owners).map((s) => s.id)).toEqual(["unbound", "archived", "bound-to-project"]);
  });
});

describe("liveSessionRows", () => {
  it("puts the sessions bound to a project session under it, each level in the sidebar's order", () => {
    const rows = liveSessionRows([
      session("unbound", "idle", 1),
      session("newer-unbound", "idle", 9),
      session("under-unbound", "idle", 2, false, "unbound"),
      session("pinned-under-unbound", "idle", 3, true, "unbound"),
      session("archived-under-unbound", "archived", 4, false, "unbound"),
      session("under-an-owner-elsewhere", "idle", 5, false, "s-console"),
    ], NONE, NONE);
    expect(rows.map((row) => [row.session.id, row.under?.id])).toEqual([
      ["newer-unbound", undefined],
      ["under-an-owner-elsewhere", undefined],
      ["unbound", undefined],
      ["pinned-under-unbound", "unbound"],
      ["under-unbound", "unbound"],
    ]);
  });

  it("ranks each level by pin and start alone when the sidebar does not order by status", () => {
    const rows = liveSessionRows([
      session("owner", "waiting_user", 1),
      session("newer", "interrupted", 9),
      session("under-old", "waiting_user", 2, false, "owner"),
      session("under-new", "interrupted", 3, false, "owner"),
    ], NONE, NONE, false);
    expect(rows.map((row) => [row.session.id, row.under?.id])).toEqual([
      ["newer", undefined],
      ["owner", undefined],
      ["under-new", "owner"],
      ["under-old", "owner"],
    ]);
  });

  it("still lists a session it cannot place under its owner, rather than dropping it", () => {
    const rows = liveSessionRows([
      session("unbound", "idle", 1),
      session("under-unbound", "idle", 2, false, "unbound"),
      session("a-third-level", "idle", 3, false, "under-unbound"),
    ], NONE, NONE);
    expect(rows.map((row) => [row.session.id, row.under?.id])).toEqual([
      ["unbound", undefined],
      ["under-unbound", "unbound"],
      ["a-third-level", undefined],
    ]);
  });
});

describe("archivedSessionRows", () => {
  it("puts an archived session under its archived owner, and lists one whose owner is not archived on its own", () => {
    const rows = archivedSessionRows([
      { ...session("owner", "archived", 1, false), ended_at: 10 },
      { ...session("under-owner", "archived", 2, false, "owner"), ended_at: 20 },
      session("live-owner", "idle", 3),
      { ...session("under-live-owner", "archived", 4, false, "live-owner"), ended_at: 30 },
    ]);
    expect(rows.map((row) => [row.session.id, row.under?.id])).toEqual([
      ["under-live-owner", undefined],
      ["owner", undefined],
      ["under-owner", "owner"],
    ]);
  });
});

describe("boundElsewhere", () => {
  const owner = (id: string, started_at: number): Session => ({ ...session(id, "idle", started_at), kind: "console", project_id: null });
  const owners = new Map([owner("hub-1", 1), owner("hub-2", 2), { ...owner("hub-old", 3), status: "archived" as const }].map((o) => [o.id, o]));

  it.each([
    { name: "counts the live bound sessions, in total and per owner, naming each owner once in the sidebar's order", sessions: [session("a", "idle", 1, false, "hub-1"), session("b", "working", 2, false, "hub-2"), session("c", "idle", 3, false, "hub-1")], expected: { count: 3, owners: [["hub-2", 1], ["hub-1", 2]] } },
    { name: "leaves out unbound, archived, unknown-owner and archived-owner sessions", sessions: [session("a", "idle", 1), session("b", "archived", 2, false, "hub-1"), session("c", "idle", 3, false, "gone"), session("d", "idle", 4, false, "hub-old")], expected: undefined },
    { name: "counts a bound coordinator's own sessions with the console session above them", sessions: [session("coordinator", "idle", 1, false, "hub-1"), session("under-coordinator", "idle", 2, false, "coordinator"), session("under-unbound", "idle", 3, false, "unbound"), session("unbound", "idle", 4)], expected: { count: 2, owners: [["hub-1", 2]] } },
  ])("$name", ({ sessions, expected }) => {
    const summary = boundElsewhere(sessions, owners, NONE, NONE);
    expect(summary && { count: summary.count, owners: summary.owners.map((o) => [o.owner.id, o.count]) }).toEqual(expected);
  });

  it("names the owners newest first, whatever their status, when the sidebar does not order by status", () => {
    const busy = new Map([{ ...owner("hub-old", 1), status: "waiting_user" as const }, owner("hub-new", 2)].map((o) => [o.id, o]));
    const sessions = [session("a", "idle", 1, false, "hub-old"), session("b", "idle", 2, false, "hub-new")];
    expect(boundElsewhere(sessions, busy, NONE, NONE)?.owners.map((o) => o.owner.id)).toEqual(["hub-old", "hub-new"]);
    expect(boundElsewhere(sessions, busy, NONE, NONE, false)?.owners.map((o) => o.owner.id)).toEqual(["hub-new", "hub-old"]);
  });
});

describe("focusGroups", () => {
  it("lists the projects with a live session bound to the console session, each with only those and a bound coordinator's own sessions under it, in project order", () => {
    const in_ = (id: string, project: string, status: SessionStatus, bound_to?: string): Session => ({ ...session(id, status, 1, false, bound_to), project_id: project });
    const groups = focusGroups(
      [project("a", "alpha"), project("b", "beta"), project("c", "charlie"), project("d", "delta")],
      [
        in_("a-mine", "a", "idle", "hub"),
        in_("a-other", "a", "idle", "other"),
        in_("b-mine", "b", "working", "hub"),
        in_("b-unbound", "b", "working"),
        in_("c-archived", "c", "archived", "hub"),
        in_("a-under-mine", "a", "idle", "a-mine"),
        in_("a-archived-under-mine", "a", "archived", "a-mine"),
      ],
      "hub",
      NONE, NONE,
    );
    expect(groups.map((g) => [g.project.id, g.sessions.map((row) => [row.session.id, row.under?.id])])).toEqual([
      ["a", [["a-mine", undefined], ["a-under-mine", "a-mine"]]],
      ["b", [["b-mine", undefined]]],
    ]);
    const byName = focusGroups(
      [project("a", "alpha"), project("b", "beta")],
      [in_("a-mine", "a", "idle", "hub"), in_("b-mine", "b", "working", "hub")],
      "hub",
      NONE, NONE,
      false,
    );
    expect(byName.map((g) => g.project.id)).toEqual(["a", "b"]);
  });
});

describe("switchStrip", () => {
  const hub = (id: string, status: SessionStatus, started_at: number, extra: Partial<Session> = {}): Session => ({
    ...session(id, status, started_at),
    project_id: null,
    kind: "console",
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
      NONE, NONE,
    );
    expect(strip.map((e) => e.consoleSession.id)).toEqual(["pinned", "idle-old", "idle-new", "waiting", "tie-a", "tie-b"]);
  });

  it("keeps its order when statuses change", () => {
    const before = [hub("a", "idle", 1), hub("b", "idle", 2), hub("c", "idle", 3)];
    const after = [hub("a", "interrupted", 1), hub("b", "waiting_user", 2), hub("c", "working", 3)];
    const ids = (sessions: Session[]) => switchStrip(sessions, "c", NONE, NONE).map((e) => e.consoleSession.id);
    expect(ids(after)).toEqual(ids(before));
  });

  it("shows each console session the top status among itself and the sessions bound to it, every status included", () => {
    const sessions = [
      hub("a", "idle", 4),
      hub("b", "idle", 3),
      hub("c", "waiting_user", 2),
      hub("d", "interrupted", 1),
      hub("e", "idle", 0),
      hub("f", "idle", 5),
      hub("g", "starting", 6),
      session("a-1", "working", 1, false, "a"),
      session("a-2", "dormant", 1, false, "a"),
      session("b-1", "working", 1, false, "b"),
      session("b-2", "archived", 1, false, "b"),
      session("d-1", "interrupted", 1, false, "d"),
      session("unbound", "waiting_user", 1),
      { ...session("e-1", "idle", 1, false, "e"), background_tasks: [{ id: "bg" }] },
      { ...session("f-1", "idle", 1, false, "f"), paused: true },
    ];
    const strip = switchStrip(sessions, "c", delegatingSessionIds(sessions), NONE);
    expect(Object.fromEntries(strip.map((e) => [e.consoleSession.id, e.status]))).toEqual({
      a: "dormant",
      b: "delegated",
      c: "waiting_user",
      d: "interrupted",
      // Its session's background work is work it delegated.
      e: "delegated",
      f: "paused",
      g: "starting",
    });
  });

  it("shows the chip of a console session whose sessions are all archived as its own status alone", () => {
    const strip = switchStrip([hub("a", "idle", 1), session("a-1", "archived", 1, false, "a")], "c", NONE, NONE);
    expect(strip.map((e) => e.status)).toEqual(["idle"]);
  });
});

describe("STATUS_RANK", () => {
  it("lists what wants a person first, then work elsewhere, then work, then no process, archived at the very end", () => {
    const byRank = (Object.keys(STATUS_RANK) as (keyof typeof STATUS_RANK)[]).sort((a, b) => STATUS_RANK[a] - STATUS_RANK[b]);
    expect(byRank).toEqual(["waiting_user", "dormant", "paused", "done", "idle", "delegated", "background", "working", "interrupted", "archived"]);
    expect(INACTIVE_PROJECT_RANK).toBe(STATUS_RANK.archived);
  });

  it("orders sessions by the kind they show, a starting or switching one by the kind it will become", () => {
    const sessions = [
      session("working", "working", 1),
      { ...session("background", "idle", 2), background_tasks: [{ id: "b" }] },
      session("delegating", "idle", 3),
      session("child", "working", 4, false, "delegating"),
      { ...session("switching", "idle", 5), switching_account: true },
      { ...session("switching-done", "interrupted", 5), switching_account: true, has_conversation: true },
      session("starting", "starting", 6),
      session("interrupted", "interrupted", 7),
      session("fresh", "idle", 8),
      { ...session("done", "idle", 9), has_conversation: true },
      session("waiting", "waiting_user", 10),
    ];
    const delegating = delegatingSessionIds(sessions);
    expect(liveSessions(sessions, delegating, NONE).map((s) => s.id)).toEqual([
      "waiting", "done", "switching-done", "fresh", "starting", "switching", "delegating", "background", "child", "working", "interrupted",
    ]);
  });

  it.each(["starting", "switching"] as const)("ranks a %s session as working with a prompt on launch, else done with a conversation, else idle", (kind) => {
    const of = (extra: Partial<Session>): Session =>
      kind === "starting" ? { ...session("s", "starting", 1), ...extra } : { ...session("s", "interrupted", 1), switching_account: true, ...extra };
    const rankIn = (extra: Partial<Session>) => projectRank([of(extra)], NONE, NONE);
    expect(rankIn({ prompt_on_launch: true, has_conversation: true })).toBe(STATUS_RANK.working);
    expect(rankIn({ has_conversation: true })).toBe(STATUS_RANK.done);
    expect(rankIn({ has_conversation: false })).toBe(STATUS_RANK.idle);
  });

  it("ranks and shows a session this window has queued for an account switch as switching, from the queue alone", () => {
    const queued = { ...session("queued", "interrupted", 1), has_conversation: true };
    const held = new Set([queued.id]);
    expect(projectRank([queued], NONE, held)).toBe(STATUS_RANK.done);
    expect(topStatus([queued], NONE, held)).toBe("switching");
    expect(projectRank([queued], NONE, NONE)).toBe(STATUS_RANK.interrupted);
    expect(topStatus([queued], NONE, NONE)).toBe("interrupted");
  });

  it("takes the most urgent kind for a project and for the console's avatar", () => {
    const sessions = [session("parent", "idle", 1), session("child", "working", 2, false, "parent")];
    const delegating = delegatingSessionIds(sessions);
    expect(projectRank([sessions[0]], delegating, NONE)).toBe(STATUS_RANK.delegated);
    expect(topStatus([sessions[0]], delegating, NONE)).toBe("delegated");
    expect(topStatus([{ ...sessions[0], has_conversation: true }], NONE, NONE)).toBe("done");
  });

  it("does not count delegated work, or a done session, as working", () => {
    const parent = session("parent", "idle", 1);
    expect(topStatus([parent], NONE, NONE)).toBe("idle");
    expect(topStatus([{ ...parent, has_conversation: true }], NONE, NONE)).toBe("done");
  });
});

describe("archivedDeletionScope", () => {
  const console_ = consoleOf("c-1", "Main");
  const project = projectOf("p-1", console_.id, "Search API");
  const hub = sessionOf("s-hub", console_.id, undefined, "Hub", "archived", { kind: "console" });
  const coordinator = sessionOf("s-coordinator", console_.id, project.id, "Coordinator", "archived", { bound_to: hub.id });
  const worker = sessionOf("s-worker", console_.id, project.id, "Worker", "archived", { bound_to: coordinator.id });
  const live = sessionOf("s-live", console_.id, project.id, "Live", "idle", { bound_to: hub.id });
  const all = [hub, coordinator, worker, live];

  it("takes a console's archived console sessions with the archived sessions bound under them", () => {
    const scope = archivedDeletionScope(all, { console: console_ });
    expect(scope.count).toBe(1);
    expect(scope.withThem).toBe(2);
    expect(scope.targetIds.sort()).toEqual(["s-coordinator", "s-hub", "s-worker"]);
  });

  it("takes a project's archived sessions, each with those bound under it, once", () => {
    const scope = archivedDeletionScope(all, { console: console_, project });
    expect(scope.count).toBe(2);
    expect(scope.withThem).toBe(0);
    expect(scope.targetIds.sort()).toEqual(["s-coordinator", "s-worker"]);
  });

  it("takes the archived sessions bound to a console session, and those bound to them", () => {
    const scope = archivedDeletionScope(all, { console: console_, consoleSession: hub });
    expect(scope.count).toBe(2);
    expect(scope.targetIds.sort()).toEqual(["s-coordinator", "s-worker"]);
  });
});
