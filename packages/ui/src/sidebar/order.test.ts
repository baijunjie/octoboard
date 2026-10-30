import { describe, expect, it } from "vitest";

import { DEFAULT_SETTINGS } from "../defaultSettings";
import { consoleOf } from "../gallery/fixtures/builders";
import type { Project, Session, SessionStatus } from "../protocol";
import { archiveNeedsConfirmation, countsAsWorking, isUnusedSession, consoleActivity, consoleStatus, INACTIVE_PROJECT_RANK, archivePreviewSessions, projectRank, liveConsoleSessions, archivedSessionRows, boundArchivedSessions, boundElsewhere, boundSessionsArchivedWith, focusGroups, liveSessionRows, liveSessions, notUnderConsoleSession, ordersByStatus, sessionsInTreeOrder, sortProjects, switchStrip } from "./order";
import { FOLLOWING_GIT_WORKFLOW } from "../gitWorkflow";

const session = (id: string, status: SessionStatus, started_at: number, pinned = false, bound_to: string | null = null): Session => ({
  id,
  agent: "claude",
  console_id: "c",
  project_id: "p",
  host_id: "h",
  role: "project",
  orchestration_role: bound_to ? "worker" : "coordinator",
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

describe("ordersByStatus", () => {
  it.each([true, false])("follows the setting (%s)", (value) => {
    expect(ordersByStatus({ ...DEFAULT_SETTINGS, sidebar_order_by_status: value })).toBe(value);
  });
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
    ]);
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
    ], false);
    expect(ordered.map((s) => s.id)).toEqual(["pinned-new", "pinned-old", "interrupted", "working", "old-waiting"]);
  });

  it("counts a session that is still starting as a running one, not as no session at all", () => {
    const starting = [session("starting", "starting", 1)];
    expect(liveSessions(starting).map((s) => s.id)).toEqual(["starting"]);
    expect(projectRank(starting)).not.toBe(INACTIVE_PROJECT_RANK);
    expect(consoleActivity(starting)).toBe("running");
  });

  it("does not offer a console session that is still starting as an owner", () => {
    const owners = ["starting", "idle", "archived"].map((status) => ({ ...session(status, status as SessionStatus, 1), role: "console" as const }));
    expect(liveConsoleSessions(owners, "c").map((s) => s.id)).toEqual(["idle"]);
  });
});

describe("consoleStatus", () => {
  it("uses the same priority as rows, regardless of pins, input order or recency", () => {
    const ordered = [
      session("waiting", "waiting_user", 1),
      session("idle", "idle", 2),
      { ...session("background", "idle", 3), background_tasks: [{ id: "watcher" }] },
      session("working", "working", 4, true),
    ];
    const kinds = ["waiting_user", "idle", "background", "working"];
    for (let index = 0; index < ordered.length; index++) {
      expect(consoleStatus(ordered.slice(index).reverse())).toBe(kinds[index]);
    }
    expect(liveSessions(ordered.map((s) => ({ ...s, pinned: false }))).map((s) => s.id))
      .toEqual(["waiting", "idle", "background", "working"]);
  });

  it("omits starting from the avatar while keeping it next to idle in session order", () => {
    const starting = session("starting", "starting", 3);
    const idle = session("idle", "idle", 2);
    const working = session("working", "working", 1);
    expect(consoleStatus([starting])).toBeUndefined();
    expect(consoleStatus([starting, working])).toBe("working");
    expect(liveSessions([working, starting, idle]).map((s) => s.id)).toEqual(["starting", "idle", "working"]);
  });

  it("omits inactive sessions and preserves the selected session's account-switch presentation", () => {
    expect(consoleStatus([])).toBeUndefined();
    expect(consoleStatus([session("paused", "interrupted", 1), session("archived", "archived", 2)])).toBeUndefined();
    expect(consoleStatus([{ ...session("switching", "idle", 3), switching_account: true }])).toBe("switching");
  });
});

describe("countsAsWorking", () => {
  it("counts a working session and one running in the background as working, and nothing else", () => {
    const tasks = [{ id: "b1" }];
    expect(countsAsWorking(session("w", "working", 1))).toBe(true);
    expect(countsAsWorking({ ...session("bg", "idle", 1), background_tasks: tasks })).toBe(true);
    expect(countsAsWorking(session("i", "idle", 1))).toBe(false);
    expect(countsAsWorking({ ...session("u", "waiting_user", 1), background_tasks: tasks })).toBe(false);
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
    ]);
    expect(ordered.map((s) => s.id)).toEqual(["pinned-work", "waiting", "idle", "new-bg", "bg", "working", "interrupted"]);
  });

  it("ranks a project between idle and foreground work while its console still shows work", () => {
    expect(projectRank([background])).toBeLessThan(projectRank([session("w", "working", 1)]));
    expect(projectRank([background])).toBeGreaterThan(projectRank([session("i", "idle", 2)]));
    expect(projectRank([background, session("i", "idle", 2)])).toBe(projectRank([session("i", "idle", 2)]));
    expect(consoleActivity([background])).toBe("working");
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
  it("includes running and interrupted descendants regardless of origin or an archived lead", () => {
    const sessions = [
      session("lead", "interrupted", 1, false, "s-console"),
      session("under-lead", "interrupted", 2, false, "lead"),
      session("under-lead-live", "idle", 3, false, "lead"),
      session("bound-live", "idle", 4, false, "s-console"),
      session("under-live", "interrupted", 5, false, "bound-live"),
      session("archived-lead", "archived", 6, false, "s-console"),
      { ...session("under-archived", "starting", 7, false, "archived-lead"), origin: "console" as const },
      session("unbound", "working", 8),
      session("other-owner", "working", 9, false, "elsewhere"),
      session("old-child", "archived", 10, false, "lead"),
    ];
    expect(boundSessionsArchivedWith(sessions, "s-console").map((s) => s.id)).toEqual(["bound-live", "lead", "under-archived", "under-lead-live", "under-live", "under-lead"]);
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
      session("lead", "idle", 9),
      session("lead-archived-child", "archived", 10, false, "lead"),
      session("lead-grandchild", "interrupted", 11, false, "lead-archived-child"),
    ];
    const needs = (id: string) => archiveNeedsConfirmation(sessions.values(), sessions.find((s) => s.id === id)!);
    expect(["idle", "interrupted", "waiting", "working", "owner", "owner-archived-child", "lead"].map(needs)).toEqual([false, false, true, true, true, false, true]);
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
    ], (p) => sessions[p.id] ?? []);
    expect(ordered.map((p) => p.id)).toEqual(["pinned", "working", "interrupted", "empty", "archived"]);
    expect(projectRank(sessions.interrupted)).toBeLessThan(INACTIVE_PROJECT_RANK);
    expect(projectRank(sessions.archived)).toBe(INACTIVE_PROJECT_RANK);
    expect(projectRank([])).toBe(INACTIVE_PROJECT_RANK);
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
    const ids = (byStatus: boolean) => sessionsInTreeOrder(consoles, projects, sessions, () => true, byStatus).map((s) => s.id);
    expect(ids(true)).toEqual(["b-waiting", "a-old", "a-new"]);
    expect(ids(false)).toEqual(["a-new", "a-old", "b-waiting"]);
  });
});

describe("notUnderConsoleSession", () => {
  it("keeps the sessions no console session is above, whatever their status, and drops a lead session with its team", () => {
    const owners = new Map([["s-console", { ...session("s-console", "idle", 0), role: "console" as const }]]);
    const sessions = [
      session("unbound", "idle", 1),
      session("archived", "archived", 2),
      session("lead", "idle", 3, false, "s-console"),
      session("under-lead", "idle", 4, false, "lead"),
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
    ]);
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
    ], false);
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
    ]);
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
  const owner = (id: string, started_at: number): Session => ({ ...session(id, "idle", started_at), role: "console", project_id: null });
  const owners = new Map([owner("hub-1", 1), owner("hub-2", 2), { ...owner("hub-old", 3), status: "archived" as const }].map((o) => [o.id, o]));

  it.each([
    { name: "counts the live bound sessions, in total and per owner, naming each owner once in the sidebar's order", sessions: [session("a", "idle", 1, false, "hub-1"), session("b", "working", 2, false, "hub-2"), session("c", "idle", 3, false, "hub-1")], expected: { count: 3, owners: [["hub-2", 1], ["hub-1", 2]] } },
    { name: "leaves out unbound, archived, unknown-owner and archived-owner sessions", sessions: [session("a", "idle", 1), session("b", "archived", 2, false, "hub-1"), session("c", "idle", 3, false, "gone"), session("d", "idle", 4, false, "hub-old")], expected: undefined },
    { name: "counts a lead session's own sessions with the console session above them", sessions: [session("lead", "idle", 1, false, "hub-1"), session("under-lead", "idle", 2, false, "lead"), session("under-unbound", "idle", 3, false, "unbound"), session("unbound", "idle", 4)], expected: { count: 2, owners: [["hub-1", 2]] } },
  ])("$name", ({ sessions, expected }) => {
    const summary = boundElsewhere(sessions, owners);
    expect(summary && { count: summary.count, owners: summary.owners.map((o) => [o.owner.id, o.count]) }).toEqual(expected);
  });

  it("names the owners newest first, whatever their status, when the sidebar does not order by status", () => {
    const busy = new Map([{ ...owner("hub-old", 1), status: "waiting_user" as const }, owner("hub-new", 2)].map((o) => [o.id, o]));
    const sessions = [session("a", "idle", 1, false, "hub-old"), session("b", "idle", 2, false, "hub-new")];
    expect(boundElsewhere(sessions, busy)?.owners.map((o) => o.owner.id)).toEqual(["hub-old", "hub-new"]);
    expect(boundElsewhere(sessions, busy, false)?.owners.map((o) => o.owner.id)).toEqual(["hub-new", "hub-old"]);
  });
});

describe("focusGroups", () => {
  it("lists the projects with a live session bound to the console session, each with only those and a lead session's own sessions under it, in project order", () => {
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
    );
    expect(groups.map((g) => [g.project.id, g.sessions.map((row) => [row.session.id, row.under?.id])])).toEqual([
      ["a", [["a-mine", undefined], ["a-under-mine", "a-mine"]]],
      ["b", [["b-mine", undefined]]],
    ]);
    const byName = focusGroups(
      [project("a", "alpha"), project("b", "beta")],
      [in_("a-mine", "a", "idle", "hub"), in_("b-mine", "b", "working", "hub")],
      "hub",
      false,
    );
    expect(byName.map((g) => g.project.id)).toEqual(["a", "b"]);
  });
});

describe("switchStrip", () => {
  const hub = (id: string, status: SessionStatus, started_at: number, extra: Partial<Session> = {}): Session => ({
    ...session(id, status, started_at),
    project_id: null,
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
