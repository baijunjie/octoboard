// @vitest-environment jsdom
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { createElement } from "react";
import { describe, expect, it, vi } from "vitest";

import { currentLanguage, t } from "../i18n/language";
import type { State, NotificationRequest } from "../store";
import { broadcastNotification } from "../store/notifications";

import type { Agent, ClientRequestBody } from "../protocol";
import { exportedDocument } from "./fixtureConfiguration";
import { createFixtureDaemon } from "./fixtureDaemon";
import type { Fixture } from "./fixture";
import { terminalFixtureUrl } from "./fakeTerminal";
import { RESTORE_FILES, RESTORE_GIT, RESTORED_PROJECT } from "./restoreSample";
import type { FixtureFile } from "./sampleFiles";
import { SAMPLE_GIT } from "./sampleGit";
import { quotaFailure, quotaOf, quotaWindow, resetCreditOf, resetCreditsOf } from "./quotas";
import { accountOf, accountSharingOf, branchItemOf, broadcastErrorNotification, consoleOf, gitStatusOf, keptOf, leftoverOf, pageOf, processTreeOf, projectOf, sessionNoticeNotification, sessionOf, shellTerminalOf, snapshotState, worktreeItemOf } from "./builders";
import { DEFAULT_GIT_WORKFLOW, FOLLOWING_GIT_WORKFLOW } from "../git/gitWorkflow";

describe("the fixture daemon's session archiving", () => {
  it("archives running and interrupted descendants and supports reopening", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "idle");
    const lead = sessionOf("lead", "c", "p", "Lead", "interrupted", { bound_to: owner.id });
    const child = sessionOf("child", "c", "p", "Child", "idle", { bound_to: lead.id });
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [owner, lead, child] }) });
    await daemon.request({ type: "archive_session", session: owner.id });
    for (const id of [owner.id, lead.id, child.id]) expect(daemon.store.getState().sessions.get(id)).toMatchObject({ status: "archived", ended_at: expect.any(Number) });
    await daemon.request({ type: "resume_session", session: lead.id });
    expect(daemon.store.getState().sessions.get(lead.id)?.status).toBe("idle");
    expect(daemon.store.getState().sessions.get(owner.id)?.status).toBe("idle");
    expect(daemon.store.getState().sessions.get(child.id)?.status).toBe("archived");
    await expect(daemon.request({ type: "archive_session", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("leaves the whole team intact when runtime archiving fails", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "idle");
    const child = sessionOf("child", "c", "p", "Child", "working", { bound_to: owner.id });
    const error = { code: "store_write_failed", params: { cause: "disk_full", detail: "database or disk is full: Error code 13: Insertion failed because database is full" }, message: "Octoboard could not read or write its own data: database or disk is full: Error code 13: Insertion failed because database is full" };
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [owner, child] }), sessionArchiveError: error });
    await expect(daemon.request({ type: "archive_session", session: owner.id })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect([...daemon.store.getState().sessions.values()]).toEqual([owner, child]);
  });
});

describe("the fixture daemon's conversation lookup", () => {
  const ID = "3f2b8c1e-7a4d-4e9b-9c55-0d6a1b2e4f70";
  const ask = (conversationLocation?: NonNullable<Parameters<typeof createFixtureDaemon>[0]["conversationLocation"]>) =>
    createFixtureDaemon({ conversationLocation }).request({ type: "locate_conversation_to_resume", project: "p", agent_session_id: ID });

  it("answers with the location the scenario gives, and not found by default", async () => {
    await expect(ask({ kind: "record", agent: "codex", account: null })).resolves.toEqual({ type: "conversation_located", id: null, location: { kind: "record", agent: "codex", account: null } });
    await expect(ask()).resolves.toMatchObject({ type: "conversation_located", location: { kind: "not_found" } });
  });

  it("refuses with the scenario's error, and never answers a pending one", async () => {
    await expect(ask({ code: "unknown_project", params: { project: "p" }, message: "unknown project p" })).rejects.toMatchObject({ code: "unknown_project" });
    const settled = vi.fn();
    void ask("pending").then(settled, settled);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(settled).not.toHaveBeenCalled();
  });
});

describe("the fixture daemon's listings that the scenario refuses", () => {
  const error = { code: "permission_denied", params: { path: "/Users/dev/Documents", detail: "Operation not permitted" }, message: "Operation not permitted" };
  const unreadable = { ...error, code: "directory_unreadable" };

  it("refuses a directory listing with the error given for its path and lists the others", async () => {
    const daemon = createFixtureDaemon({ directories: { "/Users/dev/Documents": unreadable, "/Users/dev": [] } });
    await expect(daemon.request({ type: "list_dir", path: "/Users/dev/Documents" })).rejects.toMatchObject({ code: "directory_unreadable" });
    await expect(daemon.request({ type: "list_dir", path: "~" })).resolves.toMatchObject({ type: "dir_listing", path: "/Users/dev", entries: [] });
  });

  it("refuses the page list of the console session named, and no other", async () => {
    const hub = sessionOf("hub", "c", undefined, "Hub", "idle");
    const other = sessionOf("other", "c", undefined, "Other", "idle");
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [hub, other] }), pageListErrors: { [hub.id]: error } });
    await expect(daemon.request({ type: "list_pages", session: hub.id })).rejects.toMatchObject({ code: "permission_denied" });
    expect(daemon.store.getState().pages.has(hub.id)).toBe(false);
    await expect(daemon.request({ type: "list_pages", session: other.id })).resolves.toMatchObject({ type: "page_list", pages: [] });
  });
});

describe("the fixture daemon's session reopening", () => {
  it("restores an archived session and its archived owners while preserving its children", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "archived", { ended_at: 10 });
    const lead = sessionOf("lead", "c", "p", "Lead", "archived", { bound_to: owner.id, ended_at: 20 });
    const child = sessionOf("child", "c", "p", "Child", "archived", { bound_to: lead.id, ended_at: 30 });
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [owner, lead, child] }) });
    await expect(daemon.request({ type: "resume_session", session: lead.id })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().sessions.get(lead.id)).toEqual({ ...lead, status: "idle", ended_at: null });
    expect(daemon.store.getState().sessions.get(owner.id)).toEqual({ ...owner, status: "idle", ended_at: null });
    expect(daemon.store.getState().sessions.get(child.id)).toEqual(child);
    await expect(daemon.request({ type: "resume_session", session: lead.id })).rejects.toMatchObject({ code: "session_already_running" });
    await expect(daemon.request({ type: "resume_session", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
  });
});

describe("the fixture daemon's session handoff", () => {
  const hub = sessionOf("hub", "c", undefined, "Hub", "idle", { colour: "pink", ordinal: 1 });
  const lead = sessionOf("lead", "c", "p", "Lead", "idle", { bound_to: hub.id, pinned: true });
  const child = sessionOf("child", "c", "p", "Child", "idle", { bound_to: lead.id });
  const archivedChild = sessionOf("old", "c", "p", "Old", "archived", { bound_to: lead.id });
  const state = snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [hub, lead, child, archivedChild] });
  const scenario = { state };

  it("starts the receiving session in the original's place and archives the original", async () => {
    const daemon = createFixtureDaemon(scenario);
    const reply = await daemon.request({ type: "handoff_session", session: lead.id, agent: "codex" });
    if (reply.type !== "session_opened") throw new Error("expected session_opened");
    const sessions = daemon.store.getState().sessions;
    expect(reply.session).toMatchObject({ agent: "codex", project_id: "p", role: "project", title: "Lead", bound_to: hub.id, pinned: true, status: "working" });
    expect(sessions.get(lead.id)).toMatchObject({ status: "archived", bound_to: hub.id });
    expect(sessions.get(child.id)?.bound_to).toBe(reply.session.id);
    expect(sessions.get(archivedChild.id)?.bound_to).toBe(reply.session.id);
  });

  it("gives a console session's receiving session the next ordinal and a colour no live session wears", async () => {
    const daemon = createFixtureDaemon(scenario);
    const reply = await daemon.request({ type: "handoff_session", session: hub.id, agent: "grok" });
    if (reply.type !== "session_opened") throw new Error("expected session_opened");
    expect(reply.session).toMatchObject({ role: "console", ordinal: 2, colour: "red" });
  });

  it("fails a branch inspection the scenario stages, and answers the others", async () => {
    const error = { code: "internal_error", params: { detail: "git could not be run" }, message: "git could not be run" };
    await expect(createFixtureDaemon({ ...scenario, sessionBranches: error }).request({ type: "inspect_session_branches", sessions: [lead.id] })).rejects.toMatchObject({ code: "internal_error" });
    await expect(createFixtureDaemon(scenario).request({ type: "inspect_session_branches", sessions: [lead.id] })).resolves.toMatchObject({ items: [] });
  });

  it("counts an interrupted console session's colour as in use but not an archived one's", async () => {
    const interrupted = sessionOf("hub-2", "c", undefined, "Hub 2", "interrupted", { colour: "red", ordinal: 2 });
    const archived = sessionOf("hub-3", "c", undefined, "Hub 3", "archived", { colour: "orange", ordinal: 3 });
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ consoles: [consoleOf("c", "Main")], sessions: [hub, interrupted, archived] }) });
    const reply = await daemon.request({ type: "handoff_session", session: hub.id, agent: "codex" });
    if (reply.type !== "session_opened") throw new Error("expected session_opened");
    expect(reply.session.colour).toBe("orange");
  });

  it("refuses what the daemon refuses, and a failure the scenario stages first", async () => {
    const working = sessionOf("working", "c", "p", "Working", "working");
    const archived = sessionOf("gone", "c", "p", "Gone", "archived");
    const locked = sessionOf("locked", "c", "p", "Locked", "idle", { locked: true });
    const staged = { code: "session_working", params: { session: lead.id }, message: "mid-turn" };
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [lead, working, archived, locked] }), sessionHandoffs: { [lead.id]: staged } });
    await expect(daemon.request({ type: "handoff_session", session: lead.id, agent: "codex" })).rejects.toMatchObject({ code: "session_working" });
    await expect(daemon.request({ type: "handoff_session", session: working.id, agent: "codex" })).rejects.toMatchObject({ code: "session_working" });
    await expect(daemon.request({ type: "handoff_session", session: archived.id, agent: "codex" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "handoff_session", session: locked.id, agent: "codex" })).rejects.toMatchObject({ code: "session_locked" });
    await expect(daemon.request({ type: "handoff_session", session: "missing", agent: "codex" })).rejects.toMatchObject({ code: "unknown_session" });
    const plain = createFixtureDaemon(scenario);
    await expect(plain.request({ type: "handoff_session", session: lead.id, agent: "claude" })).rejects.toMatchObject({ code: "handoff_to_same_agent", params: { agent: "Claude Code" } });
    await expect(plain.request({ type: "handoff_session", session: lead.id, agent: "codex", account: "nobody" })).rejects.toMatchObject({ code: "unknown_account" });
  });
});

describe("the fixture daemon's account order", () => {
  const setup = () => createFixtureDaemon({ state: snapshotState({ settings: {
    ...DEFAULT_SETTINGS,
    added_agents: ["claude", "codex"],
    default_clone_dir: "/Users/dev/Projects",
    accounts: [
      accountOf("work", "claude", "Work", "/work"),
      accountOf("personal", "claude", "Personal", "/personal"),
      accountOf("codex", "codex", "Codex", "/codex"),
    ],
    account_order: { claude: ["removed", "work", null], codex: ["codex", null] },
  } }) });

  it("saves a complete order including Default and preserves other agents", async () => {
    const daemon = setup();
    await expect(daemon.request({ type: "reorder_accounts", agent: "claude", accounts: ["personal", null, "work"] })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().settings.account_order).toEqual({ claude: ["personal", null, "work"], codex: ["codex", null] });
  });

  it("replaces stale saved references and broadcasts only when the saved order changes", async () => {
    const daemon = setup();
    const before = daemon.store.getState().settings;
    const request = { type: "reorder_accounts", agent: "claude", accounts: ["work", null, "personal"] } as const;
    await daemon.request({ ...request, accounts: [...request.accounts] });
    const settings = daemon.store.getState().settings;
    expect(settings).not.toBe(before);
    expect(settings.account_order?.claude).toEqual(request.accounts);
    await daemon.request({ ...request, accounts: [...request.accounts] });
    expect(daemon.store.getState().settings).toBe(settings);
    await daemon.request({ type: "reorder_accounts", agent: "grok", accounts: [null] });
    expect(daemon.store.getState().settings.account_order?.grok).toEqual([null]);
  });

  it("refuses missing, duplicate, foreign and stale IDs without changing settings", async () => {
    const daemon = setup();
    const settings = daemon.store.getState().settings;
    for (const accounts of [["work", "personal"], [null, "work"], [null, "work", "work"], [null, "work", "codex"], [null, "work", "removed"], [null, null, "work", "personal"]]) {
      await expect(daemon.request({ type: "reorder_accounts", agent: "claude", accounts })).rejects.toMatchObject({ code: "account_order_invalid" });
      expect(daemon.store.getState().settings).toBe(settings);
    }
  });
});

describe("the fixture daemon's added agents", () => {
  const lead = sessionOf("lead", "c", "p", "Lead", "idle");
  const state = snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [lead] });
  const withAdded = (added: Agent[] | null) => createFixtureDaemon({ state: { ...state, settings: { ...state.settings!, added_agents: added } } });
  const open = (agent: Agent) => ({ type: "open_session", console_id: "c", project_id: "p", agent }) as const;

  it("refuses to open a session or hand one off to an agent that is not added", async () => {
    const daemon = withAdded(["claude"]);
    await expect(daemon.request(open("codex"))).rejects.toMatchObject({ code: "agent_not_added", params: { agent: "Codex" } });
    await expect(daemon.request({ type: "handoff_session", session: lead.id, agent: "grok" })).rejects.toMatchObject({ code: "agent_not_added", params: { agent: "Grok Build" } });
    await expect(daemon.request(open("claude"))).resolves.toMatchObject({ type: "session_opened" });
  });

  it("refuses an agent nothing was added for, and nothing while the list is undecided", async () => {
    await expect(withAdded([]).request(open("claude"))).rejects.toMatchObject({ code: "agent_not_added" });
    await expect(withAdded(null).request(open("codex"))).resolves.toMatchObject({ type: "session_opened" });
  });
});

describe("the fixture daemon's system directory action", () => {
  const project = projectOf("p", "c", "Website");
  const state = snapshotState({ projects: [project] });
  const scenario = { state };

  it("acknowledges a known project without opening a host application and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "open_project_directory", project: project.id })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "open_project_directory", project: "missing" })).rejects.toMatchObject({ code: "unknown_project", params: { project: "missing" } });
    expect(daemon.store.getState().projects.get(project.id)).toEqual(project);
  });

  it("returns the configured directory failure, after checking project ownership", async () => {
    const error = { code: "directory_unavailable", params: { detail: "The directory no longer exists" }, message: "Directory unavailable" };
    const daemon = createFixtureDaemon({ ...scenario, projectDirectoryOpenError: error });
    await expect(daemon.request({ type: "open_project_directory", project: project.id })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "open_project_directory", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
  });
});

describe("the fixture daemon's system terminal and push actions", () => {
  const project = projectOf("p", "c", "Website");
  const scenario = { state: snapshotState({ projects: [project] }) };
  const actions = [
    { type: "open_project_terminal" as const, field: "projectTerminalOpenError" as const, code: "terminal_unavailable" },
    { type: "push_project_branch" as const, field: "projectPushError" as const, code: "git_push_failed" },
  ];

  it.each(actions)("acknowledges $type for a known project and refuses an unknown one", async ({ type }) => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type, project: project.id })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type, project: "missing" })).rejects.toMatchObject({ code: "unknown_project", params: { project: "missing" } });
  });

  it.each(actions)("returns the configured $type failure, after checking project ownership", async ({ type, field, code }) => {
    const error = { code, params: { detail: "refused" }, message: "refused" };
    const daemon = createFixtureDaemon({ ...scenario, [field]: error });
    await expect(daemon.request({ type, project: project.id })).rejects.toMatchObject({ code, params: error.params });
    await expect(daemon.request({ type, project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
  });
});

describe("the fixture daemon's project removal", () => {
  const project = projectOf("p", "c", "Website");
  const error = { code: "project_directory_shared", params: { project: "Billing" }, message: "shared" };
  const scenario = { state: snapshotState({ projects: [project] }), projectDeleteError: error };

  it("refuses only a removal that asks for the folder to be moved to the Trash", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "delete_project", project: project.id })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "delete_project", project: project.id, trash_directory: false })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "delete_project", project: project.id, trash_directory: true })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "delete_project", project: "missing", trash_directory: true })).rejects.toMatchObject({ code: "unknown_project" });
  });
});

describe("the fixture daemon's account removal", () => {
  const account = accountOf("a", "claude", "Work", "/Users/dev/.claude-work");
  const error = { code: "account_directory_protected", params: { path: account.config_dir }, message: "protected" };
  const base = snapshotState({});
  const scenario = { state: { ...base, settings: { ...base.settings!, accounts: [account] } }, accountDeleteError: error };

  it("refuses only a removal that asks for the directory to be moved to the Trash", async () => {
    await expect(createFixtureDaemon(scenario).request({ type: "delete_account", account: account.id, trash_directory: true })).rejects.toMatchObject({ code: error.code, params: error.params });
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "delete_account", account: account.id, trash_directory: false })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().settings.accounts).toEqual([]);
    await expect(createFixtureDaemon(scenario).request({ type: "delete_account", account: "missing", trash_directory: true })).rejects.toMatchObject({ code: "unknown_account" });
  });
});

describe("the fixture daemon's session colour", () => {
  const owner = consoleOf("c", "Main");
  const hub = sessionOf("hub", owner.id, undefined, "Hub", "idle", { colour: "lime" });
  const scenario = { state: snapshotState({ consoles: [owner], sessions: [hub] }) };

  it("acknowledges a colour for a known session and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "set_session_colour", session: hub.id, colour: "blue" })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "set_session_colour", session: "missing", colour: "blue" })).rejects.toMatchObject({ code: "unknown_session" });
  });
});

describe("the fixture daemon's project file trees", () => {
  it("searches unopened descendants by filename and returns sorted project-relative paths", async () => {
    const daemon = createFixtureDaemon({
      files: {
        "": { dir: true },
        "src": { dir: true },
        "src/server.ts": { text: "" },
        "server-docs": { dir: true },
        "server-docs/notes.md": { text: "" },
        "Server.md": { text: "" },
        ".git": { dir: true },
        ".git/server": { text: "" },
      },
    });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: " SERVER " })).resolves.toEqual({
      type: "project_file_matches", id: null, project: "web", query: " SERVER ", paths: ["Server.md", "src/server.ts"], complete: true,
    });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "missing" })).resolves.toMatchObject({ paths: [], complete: true });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "" })).resolves.toMatchObject({
      paths: ["Server.md", "server-docs/notes.md", "src/server.ts"], complete: true,
    });
  });

  it("answers a search incomplete where a folder is unreadable or its listing is cut short", async () => {
    const files = { "": { dir: true }, "server.ts": { text: "" } } as const;
    const incomplete = async (extra: Record<string, FixtureFile>) => {
      const daemon = createFixtureDaemon({ files: { ...files, ...extra } });
      return daemon.request({ type: "search_project_files", project: "web", query: "server" });
    };
    await expect(incomplete({})).resolves.toMatchObject({ paths: ["server.ts"], complete: true });
    await expect(incomplete({ locked: { dir: true, error: { code: "permission_denied", params: {}, message: "denied" } } })).resolves.toMatchObject({ paths: ["server.ts"], complete: false });
    await expect(incomplete({ many: { dir: true, complete: false } })).resolves.toMatchObject({ paths: ["server.ts"], complete: false });
  });

  it("marks what Git ignores, and everything beneath it, in listings and in a search that includes it", async () => {
    const daemon = createFixtureDaemon({
      files: {
        "": { dir: true },
        "server.ts": { text: "" },
        dist: { dir: true, ignored: true },
        "dist/server.js": { text: "" },
        "dist/lib": { dir: true },
        "dist/lib/server.map": { text: "" },
        ".env": { text: "", ignored: true },
      },
    });
    const listing = await daemon.request({ type: "list_project_dir", project: "web", path: "dist" });
    expect(listing).toMatchObject({ entries: [{ name: "lib", ignored: true }, { name: "server.js", ignored: true }] });
    const root = await daemon.request({ type: "list_project_dir", project: "web", path: "" });
    expect(root.type === "project_dir" && root.entries.map((entry) => [entry.name, entry.ignored === true])).toEqual([[".env", true], ["dist", true], ["server.ts", false]]);
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "server" })).resolves.toEqual({
      type: "project_file_matches", id: null, project: "web", query: "server", paths: ["server.ts"], complete: true,
    });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "server", include_ignored: true })).resolves.toEqual({
      type: "project_file_matches", id: null, project: "web", query: "server", paths: ["dist/lib/server.map", "dist/server.js", "server.ts"],
      ignored: ["dist/lib/server.map", "dist/server.js"], complete: true,
    });
  });

  it("leaves the system files, in any ASCII case, and everything beneath a left-out directory out of a search", async () => {
    const daemon = createFixtureDaemon({
      files: {
        "": { dir: true },
        "server.ts": { text: "" },
        "._server.ts": { text: "" },
        ".DS_Store": { text: "" },
        "server.desktop.ini": { text: "" },
        "desktop.ini": { text: "" },
        "Desktop.ini": { text: "" },
        "src": { dir: true },
        "src/.DS_Store": { text: "" },
        "src/._server-copy.ts": { text: "" },
        "src/Thumbs.db": { text: "" },
        ".Trashes": { dir: true },
        ".Trashes/server.bak": { text: "" },
        "src/.AppleDouble": { dir: true },
        "src/.AppleDouble/server.ts": { text: "" },
        "src/.APPLEDOUBLE": { dir: true },
        "src/.APPLEDOUBLE/server.ts": { text: "" },
        "des\u212Atop.ini": { text: "" },
      },
    });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "" })).resolves.toMatchObject({
      paths: ["des\u212Atop.ini", "server.desktop.ini", "server.ts"], complete: true,
    });
  });

  it("uses each project's tree for listings and reads, retaining the shared fallback", async () => {
    const daemon = createFixtureDaemon({
      files: { "": { dir: true }, "website.txt": { text: "Website" } },
      filesByProject: { api: { "": { dir: true }, "api.txt": { text: "Search API" } } },
    });
    await expect(daemon.request({ type: "list_project_dir", project: "api", path: "" })).resolves.toMatchObject({
      entries: [{ name: "api.txt" }],
    });
    await expect(daemon.request({ type: "list_project_dir", project: "web", path: "" })).resolves.toMatchObject({
      entries: [{ name: "website.txt" }],
    });
    await expect(daemon.request({ type: "read_project_file", project: "api", path: "api.txt" })).resolves.toMatchObject({
      file: { text: "Search API" },
    });
    await expect(daemon.request({ type: "read_project_file", project: "api", path: "website.txt" })).rejects.toMatchObject({ code: "file_not_found" });
  });
});

describe("the fixture daemon's session creation", () => {
  it("uses the console's current name while preserving existing session titles", async () => {
    const owner = consoleOf("c", "Release");
    const sessions = [
      sessionOf("old", owner.id, undefined, "Hub 2", "idle", { ordinal: 2 }),
      sessionOf("custom", owner.id, undefined, "Coordinate release", "idle", { ordinal: 3 }),
    ];
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [owner], sessions }) });
    const renamed = { ...owner, name: "Release planning" };
    daemon.store.setState({ consoles: new Map([[owner.id, renamed]]) });
    await expect(daemon.request({ type: "open_session", console_id: owner.id })).resolves.toMatchObject({ type: "session_opened", session: { title: renamed.name, ordinal: 4 } });
    for (const session of sessions) expect(daemon.store.getState().sessions.get(session.id)).toEqual(session);
  });

  it("honors a console session's explicit agent, account, name and launch overrides", async () => {
    const owner = consoleOf("c", "Release", { default_agent: "claude", accounts: { codex: "work" }, launch: { codex: { model: "inherited-model", effort: "high" } } });
    const project = projectOf("p", owner.id, "Website");
    const existing = sessionOf("hub", owner.id, undefined, "Renamed release coordinator", "archived", { ordinal: 6 });
    const accounts = [accountOf("work", "codex", "Work", "/Users/dev/.codex-work")];
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [owner], projects: [project], sessions: [existing], settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts } }) });
    await daemon.request({ type: "delete_archived_sessions", console: owner.id });
    expect(daemon.store.getState().sessions.size).toBe(0);
    const reply = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", title: "Coordinate release", model: "chosen-model", effort: "xhigh", bound_to: "ignored-console-binding" });
    expect(reply.type).toBe("session_opened");
    if (reply.type !== "session_opened") throw new Error("Missing opened session");
    expect(reply.session).toMatchObject({ agent: "codex", role: "console", origin: "user", status: "idle", title: "Coordinate release", account_id: "work", config_dir: accounts[0].config_dir, model: "chosen-model", effort: "xhigh", ordinal: 7 });
    expect(reply.session.bound_to).toBeNull();
    expect(daemon.store.getState().sessions.get(reply.session.id)).toEqual(reply.session);
    const defaults = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", account: null, model: null, effort: null });
    expect(defaults).toMatchObject({ type: "session_opened", session: { title: owner.name, ordinal: 8, model: null, effort: null } });
    if (defaults.type !== "session_opened") throw new Error("Missing default session");
    expect(defaults.session.account_id).toBeNull();
    const blank = await daemon.request({ type: "open_session", console_id: owner.id, title: "", task: "" });
    expect(blank).toMatchObject({ type: "session_opened", session: { origin: "user", status: "working", title: owner.name, ordinal: 9 } });
    const projectReply = await daemon.request({ type: "open_session", console_id: owner.id, project_id: project.id });
    expect(projectReply).toMatchObject({ type: "session_opened", session: { role: "project", origin: "user", status: "idle", title: project.name } });
    const repeated = await daemon.request({ type: "open_session", console_id: owner.id });
    expect(repeated).toMatchObject({ type: "session_opened", session: { title: owner.name, ordinal: 10 } });
    expect(daemon.store.getState().consoles.get(owner.id)).toEqual(owner);
  });

  it("trims explicit names and falls back to owner names for whitespace-only names", async () => {
    const owner = consoleOf("c", "Release");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [owner], projects: [project] }) });
    for (const projectId of [undefined, project.id]) {
      await expect(daemon.request({ type: "open_session", console_id: owner.id, project_id: projectId, title: " \t " })).resolves.toMatchObject({ type: "session_opened", session: { title: projectId ? project.name : owner.name } });
      await expect(daemon.request({ type: "open_session", console_id: owner.id, project_id: projectId, title: "  Coordinate release  " })).resolves.toMatchObject({ type: "session_opened", session: { title: "Coordinate release" } });
    }
  });
});

describe("the fixture daemon's session pins", () => {
  it("updates only the requested session and refuses an unknown session", async () => {
    const first = sessionOf("first", "c-1", "p-1", "Pinned work", "working", { pinned: true, bound_to: "hub" });
    const second = sessionOf("second", "c-1", "p-1", "Other work", "idle");
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [first, second] }) });
    await expect(daemon.request({ type: "set_session_pinned", session: first.id, pinned: false })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual({ ...first, pinned: false });
    expect(daemon.store.getState().sessions.get(second.id)).toEqual(second);
    await daemon.request({ type: "set_session_pinned", session: first.id, pinned: true });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual(first);
    await expect(daemon.request({ type: "set_session_pinned", session: "missing", pinned: true })).rejects.toMatchObject({ code: "unknown_session", params: { session: "missing" } });
    expect(daemon.store.getState().sessions.size).toBe(2);
  });
});

describe("the fixture daemon's session locks", () => {
  it("toggles only the requested session without changing its position or other properties", async () => {
    const first = sessionOf("first", "c", "p", "First", "idle", { pinned: true });
    const second = sessionOf("second", "c", "p", "Second", "archived", { locked: true });
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [first, second] }) });
    await expect(daemon.request({ type: "set_session_locked", session: first.id, locked: true })).resolves.toEqual({ type: "ack", id: null });
    expect([...daemon.store.getState().sessions]).toEqual([[first.id, { ...first, locked: true }], [second.id, second]]);
    await daemon.request({ type: "set_session_locked", session: first.id, locked: false });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual(first);
    await expect(daemon.request({ type: "set_session_locked", session: "missing", locked: true })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("refuses locking an archived session but still unlocks it", async () => {
    const archived = sessionOf("archived", "c", "p", "Archived", "archived");
    const stale = sessionOf("stale", "c", "p", "Stale", "archived", { locked: true });
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [archived, stale] }) });
    await expect(daemon.request({ type: "set_session_locked", session: archived.id, locked: true })).rejects.toMatchObject({ code: "session_archived" });
    expect(daemon.store.getState().sessions.get(archived.id)).toEqual(archived);
    await expect(daemon.request({ type: "set_session_locked", session: stale.id, locked: false })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().sessions.get(stale.id)).toEqual({ ...stale, locked: false });
  });

  it("refuses archiving a locked grandchild atomically and allows it after unlocking", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "interrupted");
    const lead = sessionOf("lead", "c", "p", "Lead", "interrupted", { bound_to: owner.id });
    const child = sessionOf("child", "c", "p", "Child", "interrupted", { bound_to: lead.id, locked: true });
    const daemon = createFixtureDaemon({ state: snapshotState({ sessions: [owner, lead, child] }) });
    for (const id of [owner.id, lead.id, child.id]) await expect(daemon.request({ type: "archive_session", session: id })).rejects.toMatchObject({ code: "session_locked", params: { session: child.id } });
    expect([...daemon.store.getState().sessions.values()]).toEqual([owner, lead, child]);
    await daemon.request({ type: "set_session_locked", session: child.id, locked: false });
    await daemon.request({ type: "archive_session", session: owner.id });
    expect([...daemon.store.getState().sessions.values()].every((session) => session.status === "archived")).toBe(true);
  });

  it("refuses single and bulk deletion before removing any archived family member", async () => {
    const console_ = consoleOf("c", "Main");
    const project = projectOf("p", console_.id, "Website");
    const owner = sessionOf("owner", console_.id, undefined, "Hub", "archived");
    const lead = sessionOf("lead", console_.id, project.id, "Lead", "archived", { bound_to: owner.id });
    const child = sessionOf("child", console_.id, project.id, "Child", "archived", { bound_to: lead.id, locked: true });
    const other = sessionOf("other", console_.id, project.id, "Other", "archived");
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [console_], projects: [project], sessions: [other, owner, lead, child] }) });
    const requests: ClientRequestBody[] = [
      { type: "delete_session", session: owner.id },
      { type: "delete_session", session: child.id },
      { type: "delete_archived_sessions", console: console_.id },
      { type: "delete_archived_sessions", console: console_.id, project: project.id },
      { type: "delete_archived_sessions", console: console_.id, console_session: owner.id },
      { type: "delete_project", project: project.id },
      { type: "delete_console", console: console_.id },
    ];
    for (const request of requests) {
      await expect(daemon.request(request)).rejects.toMatchObject({ code: "session_locked", params: { session: child.id } });
      expect([...daemon.store.getState().sessions.values()]).toEqual([other, owner, lead, child]);
    }
    await daemon.request({ type: "set_session_locked", session: child.id, locked: false });
    await daemon.request({ type: "delete_archived_sessions", console: console_.id, project: project.id });
    expect([...daemon.store.getState().sessions.keys()]).toEqual([owner.id]);
  });
});

describe("the fixture daemon's deletion of archived sessions", () => {
  const console_ = consoleOf("c-1", "Main");
  const project = projectOf("p-1", console_.id, "Search API");
  const other = projectOf("p-2", console_.id, "Website");
  const sessions = [
    // An archived console session with an archived lead session under it, which has its own
    // archived session, and a live one that keeps its place in the project.
    sessionOf("hub", console_.id, undefined, "Hub", "archived"),
    sessionOf("lead", console_.id, project.id, "Lead", "archived", { bound_to: "hub" }),
    sessionOf("under-lead", console_.id, project.id, "Under the lead", "archived", { bound_to: "lead" }),
    sessionOf("bound-live", console_.id, project.id, "Bound and live", "idle", { bound_to: "hub" }),
    // A live lead session's archived session goes only with its project, not with an owner above.
    sessionOf("live-lead", console_.id, project.id, "Live lead", "idle", { bound_to: "hub" }),
    sessionOf("under-live-lead", console_.id, project.id, "Under the live lead", "archived", { bound_to: "live-lead" }),
    sessionOf("elsewhere", console_.id, other.id, "In another project", "archived"),
    sessionOf("loose", console_.id, project.id, "Loose", "archived"),
  ];
  const remaining = async (body: { console_session?: string; project?: string }) => {
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [console_], projects: [project, other], sessions }),
    });
    await daemon.request({ type: "delete_archived_sessions", console: console_.id, ...body });
    return Array.from(daemon.store.getState().sessions.keys()).sort();
  };

  it("takes an archived owner's archived sessions with it, at both levels", async () => {
    expect(await remaining({})).toEqual(["bound-live", "elsewhere", "live-lead", "loose", "under-live-lead"]);
  });

  it("takes a project's archived sessions and no others", async () => {
    expect(await remaining({ project: project.id })).toEqual(["bound-live", "elsewhere", "hub", "live-lead"]);
  });

  it("takes the archived sessions under a console session, and those under its archived lead sessions", async () => {
    expect(await remaining({ console_session: "hub" })).toEqual(["bound-live", "elsewhere", "hub", "live-lead", "loose", "under-live-lead"]);
  });
});


describe("the fixture daemon's settings and trusted folders", () => {
  it("applies a settings request atomically, preserves accounts, and reaches an empty trust list by removal", async () => {
    const accounts = [accountOf("work", "claude", "Work", "/Users/dev/.claude-work")];
    const daemon = createFixtureDaemon({ state: snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts }, trustedDirectories: ["/Users/dev/code"] }) });
    await expect(daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "relative" })).rejects.toThrow();
    expect(daemon.store.getState().settings.auto_pull_repositories).toBe(false);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "~/work/../repos" });
    expect(daemon.store.getState().settings).toEqual({ ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [], default_clone_dir: "/Users/dev/repos", accounts, claude_compatibility: { codex: false, grok: true } });
    await daemon.request({ type: "update_settings", auto_pull_repositories: false, default_clone_dir: " " });
    expect(daemon.store.getState().settings).toEqual({ ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts, claude_compatibility: { codex: false, grok: true } });
    await daemon.request({ type: "update_settings", auto_delete_unresumable_archives: false });
    expect(daemon.store.getState().settings.auto_delete_unresumable_archives).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_session_requests).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_resume_requests).toBe(false);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: true });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_session_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: false });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(false);
    await daemon.request({ type: "update_settings", auto_approve_resume_requests: true });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(true);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(false);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: true });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_approve_resume_requests: false });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_resume_requests).toBe(false);
    for (const [setting, other] of [["auto_approve_account_switches", "auto_approve_handoffs"], ["auto_approve_handoffs", "auto_approve_account_switches"]] as const) {
      expect(daemon.store.getState().settings[setting]).toBe(false);
      expect(exportedDocument(daemon.store.getState()).settings?.[setting]).toBe(false);
      await daemon.request({ type: "update_settings", [setting]: true });
      expect(daemon.store.getState().settings[setting]).toBe(true);
      // Setting one never moves the other: off before the first turns on, as it was left for the second.
      expect(daemon.store.getState().settings[other]).toBe(setting === "auto_approve_account_switches" ? false : true);
      expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(false);
      expect(exportedDocument(daemon.store.getState()).settings?.[setting]).toBe(true);
      await daemon.request({ type: "update_settings", auto_approve_session_requests: false });
      expect(daemon.store.getState().settings[setting]).toBe(true);
    }
    expect(exportedDocument(daemon.store.getState()).settings?.auto_select_account).toBe(DEFAULT_SETTINGS.auto_select_account);
    await daemon.request({ type: "update_settings", auto_select_account: true });
    expect(daemon.store.getState().settings.auto_select_account).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_select_account).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_select_account).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_switch_on_usage_limit).toBe(DEFAULT_SETTINGS.auto_switch_on_usage_limit);
    await daemon.request({ type: "update_settings", auto_switch_on_usage_limit: true });
    expect(daemon.store.getState().settings.auto_switch_on_usage_limit).toBe(true);
    await daemon.request({ type: "update_settings", auto_select_account: false });
    expect(daemon.store.getState().settings.auto_switch_on_usage_limit).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_switch_on_usage_limit).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.sidebar_order_by_status).toBe(DEFAULT_SETTINGS.sidebar_order_by_status);
    await daemon.request({ type: "update_settings", sidebar_order_by_status: false });
    expect(daemon.store.getState().settings.sidebar_order_by_status).toBe(false);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.sidebar_order_by_status).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings?.sidebar_order_by_status).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_start_quota_windows).toBe(DEFAULT_SETTINGS.auto_start_quota_windows);
    await daemon.request({ type: "update_settings", auto_start_quota_windows: true });
    expect(daemon.store.getState().settings.auto_start_quota_windows).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_start_quota_windows).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_start_quota_windows).toBe(true);
    await daemon.request({ type: "remove_trusted_directory", path: "~/work/../code" });
    expect(daemon.store.getState().trustedDirectories).toEqual([]);
  });

  it("exports only the Claude compatibility the user chose, refuses a name that has none and leaves the request unapplied", async () => {
    const daemon = createFixtureDaemon({ state: snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts: [] } }) });
    const exported = async () => ((await daemon.request({ type: "export_configuration" })) as { document: ReturnType<typeof exportedDocument> }).document.settings?.claude_compatibility;
    // The values in effect follow the added agents; none has been chosen, so the export carries none.
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: true });
    expect(await exported()).toEqual({});
    await daemon.request({ type: "update_settings", claude_compatibility: { grok: false } });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: false });
    expect(await exported()).toEqual({ grok: false });
    await expect(daemon.request({ type: "update_settings", auto_pull_repositories: true, claude_compatibility: { codex: false, claude: true } })).rejects.toMatchObject({ code: "internal_error" });
    expect(daemon.store.getState().settings.auto_pull_repositories).toBe(false);
    expect(await exported()).toEqual({ grok: false });
    // A later refusal leaves the choices unapplied too, whatever the request carries before it.
    await expect(daemon.request({ type: "update_settings", claude_compatibility: { codex: false }, default_clone_dir: "relative/dir" })).rejects.toMatchObject({ code: "path_not_absolute" });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: false });
    expect(daemon.store.getState().settings.default_clone_dir).toBe("/Users/dev/Projects");
    expect(await exported()).toEqual({ grok: false });
  });
});


describe("the fixture daemon's account directories", () => {
  const work = accountOf("work", "claude", "Work", "/Users/dev/.claude-work");
  const state = {
    ...snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex-work", agent: "codex" as const }] } }),
    agentAvailability: new Map([["claude" as const, { agent: "claude" as const, availability: "available" as const, default_account_dir: "/Users/dev/.claude" }]]),
  };
  const daemon = () => createFixtureDaemon({ state });

  it("refuses a directory another account of the agent or the default account uses, compared after normalising", async () => {
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "~/x/../.claude-work/" })).rejects.toMatchObject({ code: "config_dir_taken", params: { agent: "Claude Code", name: "Work" } });
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "~/.claude" })).rejects.toMatchObject({ code: "config_dir_taken", params: { agent: "Claude Code", name: "Default" } });
    await expect(daemon().request({ type: "update_account", account: "work", config_dir: "/Users/dev/.claude" })).rejects.toMatchObject({ code: "config_dir_taken" });
  });

  it("accepts another agent's path, an account's own directory and a new one", async () => {
    await expect(daemon().request({ type: "create_account", agent: "grok", name: "Studio", config_dir: "/Users/dev/.claude-work" })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon().request({ type: "update_account", account: "work", name: "Renamed", config_dir: "/Users/dev/.claude-work" })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "/Users/dev/.claude-studio" })).resolves.toEqual({ type: "ack", id: null });
  });
});


describe("the fixture daemon's session release", () => {
  const hub = sessionOf("hub", "c", undefined, "Hub", "idle", { colour: "pink", ordinal: 1 });
  const lead = sessionOf("lead", "c", "p", "Lead", "working", { bound_to: hub.id, pinned: true });
  const child = sessionOf("child", "c", "p", "Child", "idle", { bound_to: lead.id });
  const archived = sessionOf("old", "c", "p", "Old", "archived", { bound_to: hub.id });
  const loner = sessionOf("loner", "c", "p", "Loner", "idle");
  const state = snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [hub, lead, child, archived, loner] });
  const scenario = { state };

  it("unbinds the session in any state, leaving what it owns bound to it", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "release_session", session: lead.id })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "release_session", session: archived.id })).resolves.toEqual({ type: "ack", id: null });
    const { sessions } = daemon.store.getState();
    expect(sessions.get(lead.id)).toMatchObject({ status: "working", pinned: true, bound_to: null });
    expect(sessions.get(archived.id)).toMatchObject({ status: "archived", bound_to: null });
    expect(sessions.get(child.id)?.bound_to).toBe(lead.id);
  });

  it("refuses an unknown, a console and an unbound session", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "release_session", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
    await expect(daemon.request({ type: "release_session", session: hub.id })).rejects.toMatchObject({ code: "console_session_never_bound", params: { session: hub.id } });
    await expect(daemon.request({ type: "release_session", session: loner.id })).rejects.toMatchObject({ code: "session_not_bound", params: { session: loner.id } });
    expect(daemon.store.getState().sessions.get(hub.id)).toEqual(hub);
  });

  it("leaves the session bound when the scenario stages a refusal", async () => {
    const error = { code: "session_binding_changed", params: { session: lead.id }, message: "the binding changed while it was being released" };
    const daemon = createFixtureDaemon({ ...scenario, sessionReleaseError: error });
    await expect(daemon.request({ type: "release_session", session: lead.id })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(daemon.store.getState().sessions.get(lead.id)).toEqual(lead);
  });

  it("moves the session to another owner as a staged refusal fires", async () => {
    const error = { code: "session_binding_changed", params: { session: lead.id }, message: "the binding changed while it was being released" };
    const daemon = createFixtureDaemon({ ...scenario, sessionReleaseError: error, sessionReleaseMovedTo: archived.id });
    await expect(daemon.request({ type: "release_session", session: lead.id })).rejects.toMatchObject({ code: error.code });
    expect(daemon.store.getState().sessions.get(lead.id)).toEqual({ ...lead, bound_to: archived.id });
  });
});

describe("the fixture daemon's account switches", () => {
  const work = accountOf("work", "claude", "Work", "/Users/dev/.claude-work");
  const session = sessionOf("s", "c", "p", "Release", "working", { model: "opus", effort: "high", bound_to: "hub" });
  const state = snapshotState({ sessions: [session, { ...session, id: "archived", status: "archived" }], settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex", agent: "codex" }] } });
  const scenario = { state };

  it("records the new account while keeping session launch settings, and clears it for the default account", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: work.id, config_dir: work.config_dir });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "session_already_on_account" });
    await daemon.request({ type: "switch_session_account", session: "s", account: null });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: null, config_dir: null });
  });

  it("broadcasts the session as switching while the switch holds it, and as it rests once the switch ends", async () => {
    const pending = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: "pending" } });
    void pending.request({ type: "switch_session_account", session: "s", account: "work" });
    expect(pending.store.getState().sessions.get("s")).toEqual({ ...session, switching_account: true });
    const failing = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "internal_error", params: { detail: "x" }, message: "x" } } });
    await expect(failing.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "internal_error" });
    expect(failing.store.getState().sessions.get("s")).toEqual(session);
    const done = createFixtureDaemon(scenario);
    await done.request({ type: "switch_session_account", session: "s", account: "work" });
    expect(done.store.getState().sessions.get("s")?.switching_account).toBeUndefined();
  });

  it("leaves a session whose relaunch under the new account ended at once interrupted on the account it had", async () => {
    const daemon = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "switch_did_not_come_up", params: { session: "s" }, message: "x" } } });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "switch_did_not_come_up" });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, status: "interrupted" });
  });

  it("refuses invalid targets and configured failures without changing the session", async () => {
    const daemon = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "internal_error", params: { detail: "Could not restart the agent" }, message: "Could not restart the agent" } } });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "codex" })).rejects.toMatchObject({ code: "unknown_account" });
    await expect(daemon.request({ type: "switch_session_account", session: "archived", account: "work" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "internal_error", params: { detail: "Could not restart the agent" } });
    expect(daemon.store.getState().sessions.get("s")).toEqual(session);
  });
});

describe("the fixture daemon's model changes", () => {
  const running = sessionOf("s", "c", "p", "Release", "idle", { model: "sonnet", effort: "medium", observed_model: "claude-sonnet-5-5", observed_effort: "medium" });
  const interrupted = sessionOf("down", "c", "p", "Interrupted", "interrupted", { model: "sonnet" });
  const state = snapshotState({ sessions: [running, interrupted, { ...running, id: "archived", status: "archived" }, { ...running, id: "starting", status: "starting" }] });
  const scenario = { state };
  const { observed_model: _model, observed_effort: _effort, ...forgotten } = running;

  it("records the values on a running session and forgets what its records showed", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "set_session_model", session: "s", model: "opus" })).resolves.toEqual({ type: "session_model_set", id: null, session: "s", outcome: "restarted", continued: true });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...forgotten, model: "opus", effort: "medium" });
    await daemon.request({ type: "set_session_model", session: "s", effort: " " });
    expect(daemon.store.getState().sessions.get("s")).toMatchObject({ model: "opus", effort: null });
  });

  it("only records the values of an interrupted session, and leaves one that already has them alone", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "set_session_model", session: "down", model: "opus", effort: "high" })).resolves.toEqual({ type: "session_model_set", id: null, session: "down", outcome: "recorded" });
    expect(daemon.store.getState().sessions.get("down")).toEqual({ ...interrupted, model: "opus", effort: "high" });
    const before = daemon.store.getState().sessions;
    await expect(daemon.request({ type: "set_session_model", session: "s", model: "sonnet", effort: "medium" })).resolves.toEqual({ type: "session_model_set", id: null, session: "s", outcome: "unchanged" });
    expect(daemon.store.getState().sessions).toBe(before);
  });

  it("shows the session starting while the change is held, and as it was or interrupted when the change fails", async () => {
    const pending = createFixtureDaemon({ ...scenario, sessionModelChanges: { s: "pending" } });
    void pending.request({ type: "set_session_model", session: "s", model: "opus" });
    expect(pending.store.getState().sessions.get("s")).toEqual({ ...forgotten, status: "starting" });
    const didNotStop = { code: "model_change_did_not_stop", params: { session: "s" }, message: "x" };
    const stuck = createFixtureDaemon({ ...scenario, sessionModelChanges: { s: didNotStop } });
    await expect(stuck.request({ type: "set_session_model", session: "s", model: "opus" })).rejects.toMatchObject({ code: didNotStop.code });
    expect(stuck.store.getState().sessions.get("s")).toEqual(running);
    const didNotComeUp = { code: "model_change_did_not_come_up", params: { session: "s" }, message: "x" };
    const ended = createFixtureDaemon({ ...scenario, sessionModelChanges: { s: didNotComeUp } });
    await expect(ended.request({ type: "set_session_model", session: "s", model: "opus" })).rejects.toMatchObject({ code: didNotComeUp.code });
    expect(ended.store.getState().sessions.get("s")).toEqual({ ...forgotten, status: "interrupted" });
  });

  it("refuses what the daemon refuses", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "set_session_model", session: "missing", model: "opus" })).rejects.toMatchObject({ code: "unknown_session" });
    await expect(daemon.request({ type: "set_session_model", session: "s" })).rejects.toMatchObject({ code: "field_required", params: { field: "model" } });
    await expect(daemon.request({ type: "set_session_model", session: "archived", model: "opus" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "set_session_model", session: "starting", model: "opus" })).rejects.toMatchObject({ code: "session_already_starting" });
  });
});

describe("the fixture daemon's project restore", () => {
  const project = projectOf("restore", "c", "Search API");
  const other = projectOf("other", "c", "Other project");
  const scenario = { state: snapshotState({ projects: [project, other] }), git: RESTORE_GIT, files: RESTORE_FILES };

  it("uses the restored snapshot for only the requested project and never mutates shared fixtures", async () => {
    const daemon = createFixtureDaemon({ ...scenario, projectRestores: { [project.id]: RESTORED_PROJECT } });
    await expect(daemon.request({ type: "restore_project_changes", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
    await expect(daemon.request({ type: "restore_project_changes", project: project.id })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "list_project_changes", project: project.id })).resolves.toMatchObject({ changes: [{ group: "untracked" }] });
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "README.md" })).resolves.toMatchObject({ file: { text: "# Search API\n" } });
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "new.txt" })).rejects.toMatchObject({ code: "file_not_found" });
    for (const path of ["notes.txt", "cache/local.txt"]) {
      await expect(daemon.request({ type: "read_project_file", project: project.id, path })).resolves.toMatchObject({ file: { text: (RESTORE_FILES[path] as { text: string }).text } });
    }
    await expect(daemon.request({ type: "read_project_file", project: other.id, path: "README.md" })).resolves.toMatchObject({ file: { text: "# Working edit\n" } });
    expect(RESTORE_GIT.changes["restore-main"]).toHaveLength(4);
  });

  it("returns a configured error without changing the files", async () => {
    const error = { code: "git_restore_failed", params: { detail: "index.lock exists" }, message: "Restore failed" };
    const daemon = createFixtureDaemon({ ...scenario, projectRestores: { [project.id]: error } });
    await expect(daemon.request({ type: "restore_project_changes", project: project.id })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "new.txt" })).resolves.toMatchObject({ file: { text: "A staged new file.\n" } });
  });
});


describe("the fixture daemon's local branch switches", () => {
  const project = projectOf("switch", "c", "Search API");
  const other = projectOf("other", "c", "Other project");
  const status = gitStatusOf(project.id, { branch: "feature/search" });
  const scenario = { state: snapshotState({ projects: [project, other] }), git: RESTORE_GIT, files: RESTORE_FILES };

  it("applies the supplied branch snapshot and badge to only the requested project", async () => {
    const daemon = createFixtureDaemon({ ...scenario, projectBranchSwitches: { [project.id]: { "feature/search": { ...RESTORED_PROJECT, status } } } });
    await expect(daemon.request({ type: "switch_project_branch", project: "missing", branch: "feature/search" })).rejects.toMatchObject({ code: "unknown_project" });
    await expect(daemon.request({ type: "switch_project_branch", project: project.id, branch: "feature/search" })).resolves.toEqual({ type: "ack", id: null });
    expect(daemon.store.getState().gitStatuses.get(project.id)).toEqual(status);
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "README.md" })).resolves.toMatchObject({ file: { text: "# Search API\n" } });
    await expect(daemon.request({ type: "read_project_file", project: other.id, path: "README.md" })).resolves.toMatchObject({ file: { text: "# Working edit\n" } });
  });

  it("keeps files and the badge intact when switching fails", async () => {
    const error = { code: "git_switch_failed", params: { detail: "local changes would be overwritten" }, message: "Switch failed" };
    const daemon = createFixtureDaemon({ ...scenario, projectBranchSwitches: { [project.id]: { "feature/search": error } } });
    await expect(daemon.request({ type: "switch_project_branch", project: project.id, branch: "feature/search" })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(daemon.store.getState().gitStatuses.has(project.id)).toBe(false);
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "README.md" })).resolves.toMatchObject({ file: { text: "# Working edit\n" } });
  });
});

describe("the fixture daemon's console saving", () => {
  const owner = consoleOf("c", "Release", { console_session_prompt: "Be brief.", launch: { codex: { model: "gpt-x" } } });
  const daemon = () => createFixtureDaemon({ state: snapshotState({ consoles: [owner] }) });

  it("keeps what a created console is given, the instructions trimmed and a blank one as none", async () => {
    const created = async (body: Extract<ClientRequestBody, { type: "create_console" }>) => {
      const d = daemon();
      await d.request(body);
      return Array.from(d.store.getState().consoles.values()).find((c) => c.id !== owner.id);
    };
    expect(await created({ type: "create_console", name: " Ops ", default_agent: "codex", accounts: { codex: "work" }, launch: { codex: { effort: " high " } }, console_session_prompt: "  Answer in Japanese.  " })).toMatchObject({
      name: "Ops",
      accounts: { claude: null, codex: "work", grok: null },
      launch: { claude: { model: null, effort: null }, codex: { model: null, effort: "high" }, grok: { model: null, effort: null } },
      console_session_prompt: "Answer in Japanese.",
    });
    expect((await created({ type: "create_console", name: "Plain", default_agent: "claude", console_session_prompt: "   " }))?.console_session_prompt).toBeNull();
  });

  it("leaves the instructions alone when absent, clears them with null or blank and exports what is saved", async () => {
    const d = daemon();
    await d.request({ type: "update_console", console: owner.id, name: "Renamed" });
    expect(d.store.getState().consoles.get(owner.id)).toMatchObject({ name: "Renamed", console_session_prompt: "Be brief.", launch: { codex: { model: "gpt-x" } } });
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: " Use tables. " });
    expect(exportedDocument(d.store.getState()).consoles[0]?.console_session_prompt).toBe("Use tables.");
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: null });
    expect(d.store.getState().consoles.get(owner.id)?.console_session_prompt).toBeNull();
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: "x" });
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: "  ", launch: { codex: { model: null } } });
    expect(d.store.getState().consoles.get(owner.id)).toMatchObject({ console_session_prompt: null, launch: { codex: { model: null, effort: null } } });
  });
});

describe("the fixture daemon's bound branches and worktrees", () => {
  const project = projectOf("p", "c", "Site");
  const owner = sessionOf("owner", "c", project.id, "Owner", "archived");
  const other = sessionOf("other", "c", project.id, "Other", "idle");
  const worktree = worktreeItemOf(project, owner, "feat/a", "/Users/dev/worktrees/a");
  const branch = branchItemOf(project, other, "fix/b");
  const unbound = worktreeItemOf(project, null, null, "/Users/dev/worktrees/c", "removable");
  const scenario = {
    state: snapshotState({ projects: [project], sessions: [owner, other] }),
    sessionBranches: [worktree, branch, unbound],
    projectRemovalBranches: { [project.id]: [worktree, unbound] },
  };

  it("finds what is bound to exactly the sessions asked about that still exist", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "inspect_session_branches", sessions: [owner.id, "missing"] })).resolves.toEqual({ type: "session_branches_inspected", id: null, items: [worktree] });
    // A worktree bound to no session belongs to no inspection of sessions.
    await expect(daemon.request({ type: "inspect_session_branches", sessions: [owner.id, other.id] })).resolves.toMatchObject({ items: [worktree, branch] });
    await expect(createFixtureDaemon({ ...scenario, sessionBranches: undefined }).request({ type: "inspect_session_branches", sessions: [owner.id] })).resolves.toMatchObject({ items: [] });
  });

  it("answers a project removal per project and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "inspect_project_removal", project: project.id })).resolves.toEqual({ type: "project_removal_inspected", id: null, items: [worktree, unbound], processes: [] });
    await expect(createFixtureDaemon({ ...scenario, projectRemovalBranches: undefined }).request({ type: "inspect_project_removal", project: project.id })).resolves.toMatchObject({ items: [] });
    await expect(daemon.request({ type: "inspect_project_removal", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
  });

  it("hands a project inspection's token over with the items it held back", async () => {
    const held = { ...unbound, state: "failed" as const, uncommitted: false, detail: "could not be told", cause: "project_folder_unknown" as const };
    const daemon = createFixtureDaemon({ ...scenario, projectRemovalBranches: { [project.id]: { items: [worktree, held], inspection: "token-2" } } });
    await expect(daemon.request({ type: "inspect_project_removal", project: project.id })).resolves.toEqual({ type: "project_removal_inspected", id: null, items: [worktree, held], processes: [], inspection: "token-2" });
  });

  it("carries the project's processes in a removal's inspection, the oldest first", async () => {
    const newer = leftoverOf("1:1", "vite", 10);
    const older = leftoverOf("2:1", "esbuild", 20);
    const daemon = createFixtureDaemon({ ...scenario, processes: { projectProcesses: { [project.id]: [newer, older] } } });
    await expect(daemon.request({ type: "inspect_project_removal", project: project.id })).resolves.toMatchObject({ processes: [older, newer] });
  });

  it("hands a project inspection's refusal over with all its params, the token the removal sends back among them", async () => {
    const refusal = { code: "project_directory_trash_failed", params: { path: "/work/p", detail: "it did not respond in time", cause: "unanswered", inspection: "token-1" }, message: "refused" };
    const daemon = createFixtureDaemon({ ...scenario, projectRemovalBranches: { [project.id]: refusal } });
    await expect(daemon.request({ type: "inspect_project_removal", project: project.id })).rejects.toMatchObject({ code: refusal.code, params: refusal.params });
  });

  it("can fail a project's inspection, and leave a session inspection unanswered", async () => {
    const error = { code: "internal_error", params: { detail: "no git" }, message: "no git" };
    const failing = createFixtureDaemon({ ...scenario, projectRemovalBranches: { [project.id]: error } });
    await expect(failing.request({ type: "inspect_project_removal", project: project.id })).rejects.toMatchObject({ code: "internal_error" });
    const silent = createFixtureDaemon({ ...scenario, sessionBranches: "pending" });
    const answered = vi.fn();
    void silent.request({ type: "inspect_session_branches", sessions: [owner.id] }).then(answered);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(answered).not.toHaveBeenCalled();
  });
});

describe("the fixture daemon's native session records", () => {
  const unused = sessionOf("unused", "c", "p", "Unused", "idle", { has_conversation: false });
  const owner = sessionOf("owner", "c", "p", "Used archived lead", "archived");
  const child = sessionOf("child", "c", "p", "Archived child", "archived", { bound_to: owner.id });
  const live = sessionOf("live", "c", "p", "Live child", "idle", { bound_to: owner.id });
  const scenario = { state: snapshotState({ sessions: [unused, owner, child, live] }) };

  it("keeps absence explicit and aggregates child files ahead of unknown or absent records", async () => {
    const daemon = createFixtureDaemon({ ...scenario, sessionRecords: { owner: "absent", child: "present" } });
    await expect(daemon.request({ type: "inspect_session_records", sessions: [unused.id] })).resolves.toMatchObject({ presence: "unknown" });
    await expect(daemon.request({ type: "inspect_session_records", sessions: [owner.id] })).resolves.toMatchObject({ presence: "absent" });
    await expect(daemon.request({ type: "inspect_session_records", sessions: [owner.id, "missing"] })).resolves.toMatchObject({ presence: "unknown" });
    await expect(daemon.request({ type: "inspect_session_records", sessions: [owner.id, child.id, "missing"] })).resolves.toMatchObject({ presence: "present" });
  });

  it.each(["absent", "present", "unknown"] as const)("removes an unused session with %s records", async (presence) => {
    const daemon = createFixtureDaemon({ ...scenario, sessionRecords: { unused: presence } });
    await daemon.request({ type: "delete_session", session: unused.id, only_if_unused: true });
    expect(daemon.store.getState().sessions.has(unused.id)).toBe(false);
  });

  it("refuses guarded deletion for used, bound and owner sessions, and any deletion while starting", async () => {
    const bound = { ...unused, id: "unused-bound", bound_to: owner.id, origin: "console" as const };
    const unusedOwner = { ...unused, id: "unused-owner" };
    const boundChild = { ...unused, id: "unused-child", bound_to: unusedOwner.id };
    const starting = { ...unused, id: "starting", status: "starting" as const };
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ sessions: [unused, owner, child, live, bound, unusedOwner, boundChild, starting] }) });
    for (const session of [owner, bound, unusedOwner, starting]) {
      await expect(daemon.request({ type: "delete_session", session: session.id, only_if_unused: true })).rejects.toMatchObject({ code: "session_not_deletable" });
      expect(daemon.store.getState().sessions.has(session.id)).toBe(true);
    }
    await expect(daemon.request({ type: "delete_session", session: starting.id })).rejects.toMatchObject({ code: "session_not_deletable" });
  });

  it("removes an unused session the user opened under an owner, and keeps one an agent started", async () => {
    const opened = { ...unused, id: "opened", bound_to: owner.id, origin: "user" as const };
    const started = { ...unused, id: "started", bound_to: owner.id, origin: "console" as const };
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ sessions: [owner, opened, started] }) });
    await expect(daemon.request({ type: "delete_session", session: started.id, only_if_unused: true })).rejects.toMatchObject({ code: "session_not_deletable" });
    await daemon.request({ type: "delete_session", session: opened.id, only_if_unused: true });
    expect([...daemon.store.getState().sessions.keys()]).toEqual([owner.id, started.id]);
  });

  it("takes archived children with a confirmed archived lead and preserves live children", async () => {
    const daemon = createFixtureDaemon(scenario);
    await daemon.request({ type: "delete_session", session: owner.id });
    expect([...daemon.store.getState().sessions.keys()]).toEqual([unused.id, live.id]);
  });
});


describe("the fixture daemon's structured notices", () => {
  const scenario = { };

  it("preserves a UI notice's structured content alongside its plain text", () => {
    const daemon = createFixtureDaemon(scenario);
    const listener = vi.fn();
    const unsubscribe = daemon.onNotification(listener);
    const content = createElement("span", null, "saved-file.json");
    daemon.showNotice("Saved saved-file.json", content);
    expect(listener).toHaveBeenCalledWith({ kind: "notice", message: "Saved saved-file.json", content });
    unsubscribe();
  });

  it("settles the scenario's notifications as raised only once the stack has been handed them, a delay after it subscribed", async () => {
    vi.useFakeTimers();
    try {
      const daemon = createFixtureDaemon({ ...scenario, notifications: [{ kind: "notice", message: "Could not save" }] });
      const notifications: NotificationRequest[] = [];
      const raised = vi.fn();
      void daemon.scenarioNotificationsRaised.then(raised);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(raised).not.toHaveBeenCalled();
      const unsubscribe = daemon.onNotification((notification) => notifications.push(notification));
      await vi.advanceTimersByTimeAsync(299);
      expect(raised).not.toHaveBeenCalled();
      expect(notifications).toEqual([]);
      await vi.advanceTimersByTimeAsync(1);
      expect(notifications).toHaveLength(1);
      expect(raised).toHaveBeenCalledOnce();
      unsubscribe();
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps a failed launch's title metadata, nested path and complete plain message", async () => {
    vi.useFakeTimers();
    try {
      const params = { title: "Release notes", reason_code: "config_dir_unreachable", agent: "Claude Code", path: "/Users/dev/.claude-work", reason: "Permission denied" };
      const daemon = createFixtureDaemon({ ...scenario, notifications: [sessionNoticeNotification("s-gone", "launch_failed", params)] });
      const notifications: NotificationRequest[] = [];
      const unsubscribe = daemon.onNotification((notification) => notifications.push(notification));
      await vi.runAllTimersAsync();
      expect(notifications).toHaveLength(1);
      expect(notifications[0]?.daemonTitle).toMatchObject({ code: "launch_failed.title", params });
      expect(notifications[0]?.daemonError).toMatchObject({ code: "config_dir_unreachable", params: expect.objectContaining({ path: params.path }) });
      expect(notifications[0]?.message).toContain(params.title);
      expect(notifications[0]?.message).toContain(params.path);
      unsubscribe();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("the fixture daemon's scenario notifications", () => {
  const state = () => snapshotState({ sessions: [sessionOf("s-1", "c", "p", "Review index", "idle")] });

  /** The notifications a scenario's fixtures raise once the stack has subscribed, with the records they were worded
   * from. */
  async function raised(notifications: Fixture["notifications"]): Promise<{ got: NotificationRequest[]; records: State }> {
    vi.useFakeTimers();
    try {
      const daemon = createFixtureDaemon({ state: state(), notifications });
      const got: NotificationRequest[] = [];
      const unsubscribe = daemon.onNotification((notification) => got.push(notification));
      await vi.runAllTimersAsync();
      unsubscribe();
      return { got, records: daemon.store.getState() };
    } finally {
      vi.useRealTimers();
    }
  }

  it("routes a broadcast fixture through the code the store routes the daemon's event with", async () => {
    const notice = { type: "session_notice", session: "s-1", code: "message_not_sent", params: { agent: "Codex" }, message: "" } as const;
    const failure = { type: "error", id: null, code: "a_code_from_a_newer_daemon", params: {}, message: "Its own text." } as const;
    const { got, records } = await raised([{ kind: "broadcast", event: notice }, { kind: "broadcast", event: failure }]);
    expect(got).toEqual([broadcastNotification(notice, records, currentLanguage()), broadcastNotification(failure, records, currentLanguage())]);
    expect(got.map((notification) => notification.kind)).toEqual(["error", "error"]);
  });

  it("shows a session notice that is no failure as a timed notice, and a broadcast error as an alert naming no session", async () => {
    const { got: [notice, failure] } = await raised([sessionNoticeNotification("s-1", "claude_workspace_untrusted"), broadcastErrorNotification("internal_error", { detail: "boom" }, "boom")]);
    expect(notice).toMatchObject({ kind: "notice", session: "s-1" });
    expect(failure).toMatchObject({ kind: "error", daemonError: { code: "internal_error" } });
    expect(failure).not.toHaveProperty("session");
  });

  it("raises a raised fixture as the action's alert, its message worded as a rejected request carries it", async () => {
    const { got: [notification] } = await raised([{ kind: "raised", code: "file_not_found", params: { path: "src/main.ts" }, message: "", title: "Could not reveal", session: "s-1", recovery: "hostPathGone" }]);
    expect(notification).toMatchObject({ kind: "error", title: "Could not reveal", session: "s-1", recovery: "hostPathGone", daemonError: { code: "file_not_found", params: { path: "src/main.ts" } } });
    expect(notification?.message).toContain("src/main.ts");
    expect(notification?.error).toBeInstanceOf(Error);
    expect((notification?.error as Error).message).toBe(notification?.message);
  });

  it("raises a notice fixture as the UI's own timed notice, worded in the viewing language when a function", async () => {
    const { got } = await raised([{ kind: "notice", message: "Saved" }, { kind: "notice", message: (t) => t("common.pathCopied") }]);
    expect(got).toEqual([{ kind: "notice", message: "Saved" }, { kind: "notice", message: t("common.pathCopied") }]);
  });
});

describe("the fixture daemon's file search", () => {
  it("refuses a search with the scenario's search error instead of answering it", async () => {
    const error = { code: "source_unavailable", params: { path: "", cause: "missing", detail: "gone" }, message: " is unavailable: gone" };
    const daemon = createFixtureDaemon({ state: snapshotState({ projects: [projectOf("p", "c", "Website")] }), searchError: error });
    await expect(daemon.request({ type: "search_project_files", project: "p", query: "main" })).rejects.toMatchObject({ code: "source_unavailable", params: { path: "" } });
  });
});

describe("the fixture daemon's error titles", () => {
  const scenario = { };

  it("gives a scenario's notification the title it names, worded in the viewing language when it is a function", async () => {
    vi.useFakeTimers();
    try {
      const daemon = createFixtureDaemon({
        ...scenario,
        notifications: [
          { kind: "raised", message: "one", title: "Could not open the folder" },
          { kind: "raised", message: "two", title: (t) => t("settings.saveFailedTitle") },
        ],
      });
      const notifications: NotificationRequest[] = [];
      const unsubscribe = daemon.onNotification((notification) => notifications.push(notification));
      await vi.runAllTimersAsync();
      expect(notifications.map((notification) => notification.title)).toEqual(["Could not open the folder", t("settings.saveFailedTitle")]);
      unsubscribe();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("the fixture daemon's settings and rail removals", () => {
  it("clears deleted account references while preserving explicit defaults and existing session directories", async () => {
    const account = accountOf("a", "claude", "Work", "/Users/dev/.claude-work");
    const console_ = consoleOf("c", "Main", { accounts: { claude: account.id } });
    const project = projectOf("p", console_.id, "Website", { accounts: { claude: { kind: "account", id: account.id }, codex: { kind: "default" } } });
    const session = sessionOf("s", console_.id, project.id, "Work", "idle", { account_id: account.id, config_dir: account.config_dir });
    const daemon = createFixtureDaemon({ state: { ...snapshotState({ consoles: [console_], projects: [project], sessions: [session] }), settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "", accounts: [account] } } });
    await daemon.request({ type: "delete_account", account: account.id });
    const current = daemon.store.getState();
    expect(current.settings.accounts).toEqual([]);
    expect(current.consoles.get(console_.id)?.accounts.claude).toBeNull();
    expect(current.projects.get(project.id)?.accounts.claude).toEqual({ kind: "inherit" });
    expect(current.projects.get(project.id)?.accounts.codex).toEqual({ kind: "default" });
    expect(current.sessions.get(session.id)).toEqual(session);
    await expect(daemon.request({ type: "delete_account", account: account.id })).rejects.toMatchObject({ code: "unknown_account" });
  });

  it("refuses console removal while sessions run, then removes its projects, archived sessions and pages", async () => {
    const console_ = consoleOf("c", "Main");
    const other = consoleOf("other", "Other");
    const project = projectOf("p", console_.id, "Website");
    const session = sessionOf("s", console_.id, project.id, "Work", "working");
    const hub = sessionOf("hub", console_.id, undefined, "Hub", "archived");
    const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [console_, other], projects: [project], sessions: [session, hub], pages: { [hub.id]: [pageOf("report", hub.id, "<p>Report</p>", 1)] }, gitStatuses: [gitStatusOf(project.id)] }) });
    await expect(daemon.request({ type: "delete_console", console: console_.id })).rejects.toMatchObject({ code: "console_has_running_sessions" });
    daemon.store.setState({ sessions: new Map([[session.id, { ...session, status: "archived" }], [hub.id, hub]]) });
    await daemon.request({ type: "delete_console", console: console_.id });
    expect([...daemon.store.getState().consoles.keys()]).toEqual([other.id]);
    expect(daemon.store.getState().projects.size).toBe(0);
    expect(daemon.store.getState().sessions.size).toBe(0);
    expect(daemon.store.getState().pages.size).toBe(0);
    expect(daemon.store.getState().gitStatuses.size).toBe(0);
    await expect(daemon.request({ type: "delete_console", console: "missing" })).rejects.toMatchObject({ code: "unknown_console" });
  });
});

describe("the fixture daemon's permission approval", () => {
  const scenario = { state: snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts: [] } }) };

  it("saves and clears Codex's reviewer and Claude Code's mode, one for the agent, without changing other settings", async () => {
    const daemon = createFixtureDaemon(scenario);
    await daemon.request({ type: "set_codex_approval_reviewer", reviewer: "auto_review" });
    await daemon.request({ type: "set_claude_approval_mode", mode: "bypass" });
    expect(daemon.store.getState().settings).toEqual({ ...scenario.state.settings, claude_compatibility: { codex: false, grok: true }, codex_approval_reviewer: "auto_review", claude_approval_mode: "bypass" });
    await daemon.request({ type: "set_codex_approval_reviewer", reviewer: null });
    await daemon.request({ type: "set_claude_approval_mode", mode: null });
    expect(daemon.store.getState().settings).toEqual({ ...scenario.state.settings, claude_compatibility: { codex: false, grok: true }, codex_approval_reviewer: null, claude_approval_mode: null });
  });

  it("retains every selection when saving fails", async () => {
    const error = { code: "store_write_failed", params: { cause: "access", detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" }, message: "Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" };
    const daemon = createFixtureDaemon({ ...scenario, accountApprovalError: error });
    const before = daemon.store.getState().settings;
    await expect(daemon.request({ type: "set_codex_approval_reviewer", reviewer: "auto_review" })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "set_claude_approval_mode", mode: "bypass" })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(daemon.store.getState().settings).toEqual(before);
  });
});

describe("the fixture daemon's agent tags", () => {
  it("sets an agent's tags normalised as the daemon does, clears them with an empty list and refuses an unknown agent", async () => {
    const daemon = createFixtureDaemon({ state: snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts: [] } }) });
    await daemon.request({ type: "update_settings", agent_tags: { codex: [" Review ", "", "review", "tests"] } });
    expect(daemon.store.getState().settings.agent_tags).toEqual({ codex: ["Review", "tests"] });
    await expect(daemon.request({ type: "update_settings", agent_tags: { claude: ["ui"], nobody: ["x"] } as never })).rejects.toMatchObject({ code: "internal_error" });
    expect(daemon.store.getState().settings.agent_tags).toEqual({ codex: ["Review", "tests"] });
    await daemon.request({ type: "update_settings", agent_tags: { codex: [] } });
    expect(daemon.store.getState().settings.agent_tags).toEqual({});
  });
  it("refuses a save of agent tags staged to fail and keeps the stored tags, while other settings still save", async () => {
    const error = { code: "store_write_failed", params: { cause: "access", detail: "attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" }, message: "Octoboard could not read or write its own data: attempt to write a readonly database: Error code 1544: SQLITE_READONLY_DIRECTORY" };
    const daemon = createFixtureDaemon({ state: snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts: [], agent_tags: { codex: ["review"] } } }), agentTagsError: error });
    await expect(daemon.request({ type: "update_settings", agent_tags: { codex: ["review", "tests"] } })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(daemon.store.getState().settings.agent_tags).toEqual({ codex: ["review"] });
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_pull_repositories).toBe(true);
  });
});

describe("the fixture daemon's Claude compatibility", () => {
  const settings = (added_agents: Agent[], claude_compatibility?: Partial<Record<Agent, boolean>>) => ({ ...DEFAULT_SETTINGS, added_agents, default_clone_dir: "/Users/dev/Projects", accounts: [], ...(claude_compatibility && { claude_compatibility }) });
  const daemonOf = (added: Agent[], choices?: Partial<Record<Agent, boolean>>) => createFixtureDaemon({ state: snapshotState({ settings: settings(added, choices) }) });

  it("reports Codex on only while Claude Code is added, and Grok Build on, until the user chooses", () => {
    const daemon = daemonOf(["codex", "grok"]);
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: false, grok: true });
    daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: ["claude", "codex", "grok"] } });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: true });
    daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: null } });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: false, grok: true });
  });

  it("keeps the user's choice whatever agents are added, and reads a scenario's value as a choice", async () => {
    const daemon = daemonOf(["codex"], { grok: false });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: false, grok: false });
    await daemon.request({ type: "update_settings", claude_compatibility: { codex: true } });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: false });
    daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: ["claude"] } });
    daemon.store.setState({ settings: { ...daemon.store.getState().settings, added_agents: [] } });
    expect(daemon.store.getState().settings.claude_compatibility).toEqual({ codex: true, grok: false });
  });
});

describe("the fixture daemon's account sharing", () => {
  const accounts = [
    accountOf("claude", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("same", "claude", "Mirror", "/Users/dev/.claude/"),
    accountOf("codex", "codex", "Personal", "/Users/dev/.codex-personal"),
  ];
  const scenario = {
    state: {
      ...snapshotState({ settings: { ...DEFAULT_SETTINGS, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts } }),
      agentAvailability: new Map([
        ["claude" as const, { agent: "claude" as const, availability: "available" as const, default_account_dir: "/Users/dev/.claude" }],
        ["codex" as const, { agent: "codex" as const, availability: "available" as const, default_account_dir: "/Users/dev/.codex" }],
      ]),
    },
  };
  const sharing = (daemon: ReturnType<typeof createFixtureDaemon>, id: string) => daemon.store.getState().settings.accounts.find((account) => account.id === id);

  it("carries a change out at once when nothing is in the way", async () => {
    const daemon = createFixtureDaemon(scenario);
    await daemon.request({ type: "set_account_sharing", account: "claude", share: true });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: true, share_default_config_incomplete: false });
    expect(sharing(daemon, "codex")).toEqual(accounts[2]);
    await daemon.request({ type: "set_account_sharing", account: "claude", share: true });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: true, share_default_config_incomplete: false });
    await daemon.request({ type: "set_account_sharing", account: "claude", share: false });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: false, share_default_config_incomplete: false });
    // Absent is off, so asking for off again records nothing.
    await daemon.request({ type: "set_account_sharing", account: "codex", share: false });
    expect(sharing(daemon, "codex")).toEqual(accounts[2]);
    expect(daemon.store.getState().accountSharing.size).toBe(0);
  });

  it("retries a change that did not finish when asked again for the recorded choice, and clears the failure it kept", async () => {
    const daemon = createFixtureDaemon({
      ...scenario,
      state: {
        ...scenario.state,
        accountSharing: accountSharingOf([{ account: "claude", failure: { share: true, cause: "permission_denied", items: [{ item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" }], detail: "`skills/` left unchanged" } }]),
      },
    });
    daemon.store.setState({
      settings: {
        ...daemon.store.getState().settings,
        accounts: accounts.map((account) => account.id === "claude" ? { ...account, share_default_config: true, share_default_config_incomplete: true } : account),
      },
    });
    await daemon.request({ type: "set_account_sharing", account: "claude", share: true });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: true, share_default_config_incomplete: false });
    expect(daemon.store.getState().accountSharing.size).toBe(0);
  });

  it("turns a request away while the account is in use or its change runs, recording nothing", async () => {
    const daemon = createFixtureDaemon({
      ...scenario,
      state: {
        ...scenario.state,
        accountSharing: accountSharingOf([
          { account: "claude", in_use: { sessions: 2, signing_in: true } },
          { account: "codex", progress: { share: true, phase: "changing", done: 1, total: 4 } },
        ]),
      },
    });
    const before = daemon.store.getState();
    await expect(daemon.request({ type: "set_account_sharing", account: "claude", share: true })).rejects.toMatchObject({
      code: "account_in_use",
      params: { account: "claude", name: "Work", sessions: "2", signing_in: "true" },
    });
    await expect(daemon.request({ type: "set_account_sharing", account: "codex", share: true })).rejects.toMatchObject({
      code: "account_sharing_in_progress",
      params: { account: "codex", name: "Personal" },
    });
    expect(daemon.store.getState().settings).toEqual(before.settings);
    expect(daemon.store.getState().accountSharing).toEqual(before.accountSharing);
  });

  it("refuses the default account and a directory that is the default's, recording nothing", async () => {
    const daemon = createFixtureDaemon(scenario);
    const before = daemon.store.getState().settings;
    await expect(daemon.request({ type: "set_account_sharing", account: null, share: true })).rejects.toMatchObject({ code: "default_account_not_shareable", params: {} });
    await expect(daemon.request({ type: "set_account_sharing", account: "same", share: true })).rejects.toMatchObject({ code: "default_account_not_shareable", params: { account: "same", name: "Mirror" } });
    await expect(daemon.request({ type: "set_account_sharing", account: "missing", share: true })).rejects.toMatchObject({ code: "unknown_account", params: { account: "missing" } });
    expect(daemon.store.getState().settings).toEqual(before);
    daemon.store.setState({
      settings: {
        ...before,
        accounts: accounts.map((account) => account.id === "same" ? { ...account, share_default_config: true } : account),
      },
    });
    await daemon.request({ type: "set_account_sharing", account: "same", share: false });
    expect(sharing(daemon, "same")).toMatchObject({ share_default_config: false, share_default_config_incomplete: false });
  });

  it("refuses opening, resuming, switching and handing off from an account whose change runs", async () => {
    const hub = sessionOf("hub", "c", undefined, "Hub", "idle", { account_id: "claude", config_dir: "/Users/dev/.claude-work" });
    const stopped = sessionOf("stopped", "c", undefined, "Stopped", "interrupted", { account_id: "claude", config_dir: "/Users/dev/.claude-work" });
    const other = sessionOf("other", "c", undefined, "Other", "idle");
    const daemon = createFixtureDaemon({
      ...scenario,
      state: {
        ...snapshotState({ consoles: [consoleOf("c", "Main")], sessions: [hub, stopped, other], settings: { ...scenario.state.settings!, added_agents: ["claude", "codex"] } }),
        agentAvailability: scenario.state.agentAvailability,
        accountSharing: accountSharingOf([{ account: "claude", progress: { share: true, phase: "changing", done: 1, total: 9, item: "skills/" } }]),
      },
    });
    const refused = { code: "account_sharing_in_progress", params: { account: "claude", name: "Work" } };
    await expect(daemon.request({ type: "open_session", console_id: "c", agent: "claude", account: "claude" })).rejects.toMatchObject(refused);
    await expect(daemon.request({ type: "resume_session", session: "stopped" })).rejects.toMatchObject(refused);
    await expect(daemon.request({ type: "switch_session_account", session: "hub", account: null })).rejects.toMatchObject(refused);
    await expect(daemon.request({ type: "switch_session_account", session: "other", account: "claude" })).rejects.toMatchObject(refused);
    await expect(daemon.request({ type: "handoff_session", session: "hub", agent: "codex" })).rejects.toMatchObject(refused);
    await expect(daemon.request({ type: "open_session", console_id: "c", agent: "claude", account: null })).resolves.toMatchObject({ type: "session_opened" });
  });

  it("records the choice, keeps the failure and rejects when the change fails part-way", async () => {
    const items = [
      { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" },
      { item: "abc", left: "unchanged", cause: "permission_denied", detail: "permission denied", sessions: ["s1"] },
    ];
    const error = {
      code: "account_sharing_failed",
      params: { account: "claude", name: "Work", cause: "permission_denied", items: JSON.stringify(items), detail: "`skills/` left unchanged (permission denied)" },
      message: "ending the sharing of the default configuration with account `Work` did not complete: `skills/` left unchanged (permission denied)",
    };
    const daemon = createFixtureDaemon({ ...scenario, accountSharingError: error });
    daemon.store.setState({
      settings: {
        ...daemon.store.getState().settings,
        accounts: accounts.map((account) => account.id === "claude" ? { ...account, share_default_config: true } : account),
      },
    });
    await expect(daemon.request({ type: "set_account_sharing", account: "claude", share: false })).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: false, share_default_config_incomplete: true });
    expect(daemon.store.getState().accountSharing.get("claude")).toEqual({ account: "claude", failure: { share: false, cause: "permission_denied", items, detail: error.params.detail } });
    expect(sharing(daemon, "codex")).toEqual(accounts[2]);
    const before = daemon.store.getState().settings;
    await expect(daemon.request({ type: "set_account_sharing", account: null, share: true })).rejects.toMatchObject({ code: "default_account_not_shareable" });
    await daemon.request({ type: "set_account_sharing", account: "codex", share: false });
    expect(daemon.store.getState().settings).toEqual(before);
  });

  it("keeps an account whose removal could not turn its sharing off, incomplete with the failure", async () => {
    const error = {
      code: "account_sharing_failed",
      params: { account: "claude", name: "Work", cause: "default_moved", items: "[]", detail: "the default directory moved" },
      message: "did not complete",
    };
    const daemon = createFixtureDaemon({ ...scenario, accountSharingError: error });
    daemon.store.setState({
      settings: { ...daemon.store.getState().settings, accounts: accounts.map((account) => account.id === "claude" ? { ...account, share_default_config: true } : account) },
    });
    await expect(daemon.request({ type: "delete_account", account: "claude" })).rejects.toMatchObject({ code: "account_sharing_failed" });
    expect(sharing(daemon, "claude")).toMatchObject({ share_default_config: false, share_default_config_incomplete: true });
    expect(daemon.store.getState().accountSharing.get("claude")?.failure).toEqual({ share: false, cause: "default_moved", items: [], detail: "the default directory moved" });
    await daemon.request({ type: "delete_account", account: "codex" });
    expect(sharing(daemon, "codex")).toBeUndefined();
  });

  it("records nothing when the scenario's error is a refusal", async () => {
    const error = { code: "account_in_use", params: { account: "claude", name: "Work", sessions: "1", signing_in: "false" }, message: "in use" };
    const daemon = createFixtureDaemon({ ...scenario, accountSharingError: error });
    const before = daemon.store.getState();
    await expect(daemon.request({ type: "set_account_sharing", account: "claude", share: true })).rejects.toMatchObject({ code: "account_in_use" });
    expect(daemon.store.getState().settings).toEqual(before.settings);
    expect(daemon.store.getState().accountSharing.size).toBe(0);
  });
});

describe("the fixture daemon's path link probe", () => {
  it("finds the scenario's paths, in order, whether written absolute, home-relative or relative to the project", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [owner], projects: [project] }),
      pathLinks: { [`${project.path}/src/a.ts`]: { view: { path: "src/a.ts" } }, [`${project.path}/docs`]: "directory", "/tmp/out.log": "file" },
    });
    const viewed = { kind: "file", view: { path: "src/a.ts" }, viewer_only: true };
    await expect(
      daemon.request({ type: "probe_path_links", project: project.id, paths: ["src/a.ts", "~/code/website/docs", "./src/../src/a.ts", "src/b.ts", `${project.path}/src/a.ts`, "/tmp/out.log"] }),
    ).resolves.toEqual({
      type: "path_links",
      id: null,
      links: [
        { path: "src/a.ts", ...viewed },
        { path: "~/code/website/docs", kind: "directory" },
        { path: "./src/../src/a.ts", ...viewed },
        { path: "src/b.ts", kind: "none" },
        { path: `${project.path}/src/a.ts`, ...viewed },
        { path: "/tmp/out.log", kind: "file" },
      ],
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/tmp/out.log" })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "~/code/website/docs" })).resolves.toEqual({ type: "ack", id: null });
  });

  it("refuses opening a file the viewer shows from a link with path_link_refused and cause launchable, as the probe answers it viewer-only", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [owner], projects: [project] }),
      pathLinks: { [`${project.path}/src/a.ts`]: { view: { path: "src/a.ts" } } },
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "src/a.ts" })).rejects.toMatchObject({
      code: "path_link_refused",
      params: { path: "src/a.ts", cause: "launchable" },
    });
  });

  it("opens what the probe found, but refuses one gone since with file_not_found, the path as given and the kind of root it was in", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [owner], projects: [project] }),
      pathLinks: { "/private/tmp/sheet.png": { kind: "file", gone: true }, [`${project.path}/docs`]: { kind: "directory", gone: true } },
    });
    await expect(daemon.request({ type: "probe_path_links", project: project.id, paths: ["/private/tmp/sheet.png", "docs"] })).resolves.toEqual({
      type: "path_links",
      id: null,
      links: [
        { path: "/private/tmp/sheet.png", kind: "file" },
        { path: "docs", kind: "directory" },
      ],
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet.png" })).rejects.toMatchObject({
      code: "file_not_found",
      params: { path: "/private/tmp/sheet.png", root: "temporary" },
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "docs" })).rejects.toMatchObject({ code: "file_not_found", params: { path: "docs", root: "project" } });
  });

  it("answers the probe for a path the daemon will not open from a link, and refuses opening it with path_link_refused, the path as given, the cause and, for unreadable, the system's text", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [owner], projects: [project] }),
      pathLinks: {
        [`${project.path}/docs`]: { kind: "directory", refused: "ambiguous" },
        [`${project.path}/bin/helper`]: { kind: "file", refused: "launchable", view: { path: "bin/helper" } },
        "/private/tmp/sheet.png": { kind: "file", refused: "not_private" },
        "/private/tmp/sheet2.png": { kind: "file", refused: "unreadable", detail: "Input/output error (os error 5)" },
        "/private/tmp/sheet3.png": { kind: "file", refused: "other" },
        "/private/tmp/sheet4.png": { kind: "file", outside: true },
      },
    });
    await expect(daemon.request({ type: "probe_path_links", project: project.id, paths: ["docs", "bin/helper", "/private/tmp/sheet.png"] })).resolves.toEqual({
      type: "path_links",
      id: null,
      links: [
        { path: "docs", kind: "directory" },
        { path: "bin/helper", kind: "file", view: { path: "bin/helper" }, viewer_only: true },
        { path: "/private/tmp/sheet.png", kind: "file" },
      ],
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "docs" })).rejects.toMatchObject({
      code: "path_link_refused",
      params: { path: "docs", cause: "ambiguous" },
    });
    // Only a refusal of cause unreadable carries the system's text.
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet.png" })).rejects.toMatchObject({
      code: "path_link_refused",
      params: { path: "/private/tmp/sheet.png", cause: "not_private" },
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet.png" })).rejects.not.toHaveProperty("params.detail");
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet2.png" })).rejects.toMatchObject({
      code: "path_link_refused",
      params: { path: "/private/tmp/sheet2.png", cause: "unreadable", detail: "Input/output error (os error 5)" },
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet3.png" })).rejects.toMatchObject({
      code: "path_link_refused",
      params: { path: "/private/tmp/sheet3.png", cause: "other" },
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet3.png" })).rejects.not.toHaveProperty("params.detail");
    // A path that lies outside every root now is refused as outside_scope for an open, which words no remedy.
    await expect(daemon.request({ type: "probe_path_links", project: project.id, paths: ["/private/tmp/sheet4.png"] })).resolves.toEqual({
      type: "path_links",
      id: null,
      links: [{ path: "/private/tmp/sheet4.png", kind: "file" }],
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "/private/tmp/sheet4.png" })).rejects.toMatchObject({
      code: "outside_scope",
      params: { path: "/private/tmp/sheet4.png", action: "open" },
    });
  });

  it("sends where a file the viewer shows (or a folder's README) is read from, and every file of a project as viewer-only, and reads a worktree's files from the worktree", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [owner], projects: [project] }),
      files: { "a.ts": { text: "const a = 1;\n" }, "bin/tool": { binary: { size: 2048 } } },
      worktreeFiles: { "wt-side": { "a.ts": { text: "const a = 2;\n" } } },
      pathLinks: {
        [`${project.path}/a.ts`]: { view: { path: "a.ts" } },
        "/Users/dev/side/a.ts": { view: { worktree: "wt-side", path: "a.ts" } },
        "/tmp/b.png": "file",
        [`${project.path}/bin/tool`]: { view: { path: "bin/tool" } },
        [`${project.path}/docs/plan`]: { kind: "directory", view: { path: "docs/plan/README.md" } },
      },
    });
    await expect(daemon.request({ type: "probe_path_links", project: project.id, paths: ["a.ts", "/Users/dev/side/a.ts", "/tmp/b.png", "c.ts", "bin/tool", "docs/plan"] })).resolves.toEqual({
      type: "path_links",
      id: null,
      links: [
        { path: "a.ts", kind: "file", view: { path: "a.ts" }, viewer_only: true },
        { path: "/Users/dev/side/a.ts", kind: "file", view: { worktree: "wt-side", path: "a.ts" }, viewer_only: true },
        { path: "/tmp/b.png", kind: "file" },
        { path: "c.ts", kind: "none" },
        { path: "bin/tool", kind: "file", view: { path: "bin/tool" }, viewer_only: true },
        { path: "docs/plan", kind: "directory", view: { path: "docs/plan/README.md" } },
      ],
    });
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "a.ts" })).resolves.toMatchObject({ worktree: null, file: { text: "const a = 1;\n" } });
    await expect(daemon.request({ type: "read_project_file", project: project.id, path: "bin/tool" })).resolves.toMatchObject({ file: { kind: "binary", size: 2048, media_type: null } });
    await expect(daemon.request({ type: "read_project_file", project: project.id, worktree: "wt-side", path: "a.ts" })).resolves.toMatchObject({ worktree: "wt-side", file: { text: "const a = 2;\n" } });
    await expect(daemon.request({ type: "read_project_file", project: project.id, worktree: "wt-gone", path: "a.ts" })).rejects.toMatchObject({ code: "file_not_found" });
  });
});

describe("the fixture daemon's added agents", () => {
  const settings = { ...DEFAULT_SETTINGS, default_clone_dir: "/Users/dev/Projects", accounts: [] };
  const daemonWith = (added: ("claude" | "codex" | "grok")[] | null, availability: [("claude" | "codex" | "grok"), "available" | "unavailable" | "not_determined"][]) => {
    const daemon = createFixtureDaemon({ state: snapshotState({ settings: { ...settings, added_agents: added } }) });
    daemon.store.setState({ agentAvailability: new Map(availability.map(([agent, state]) => [agent, { agent, availability: state, default_account_dir: null }])) });
    return daemon;
  };
  const added = (daemon: ReturnType<typeof daemonWith>) => daemon.store.getState().settings.added_agents;

  it("adds an installed agent at the end and refuses one that is added, not installed or not determined", async () => {
    const daemon = daemonWith(["claude"], [["claude", "available"], ["codex", "available"], ["grok", "unavailable"]]);
    await daemon.request({ type: "add_agent", agent: "codex" });
    expect(added(daemon)).toEqual(["claude", "codex"]);
    await expect(daemon.request({ type: "add_agent", agent: "codex" })).rejects.toMatchObject({ code: "agent_already_added" });
    await expect(daemon.request({ type: "add_agent", agent: "grok" })).rejects.toMatchObject({ code: "agent_not_installed" });
    daemon.store.setState({ agentAvailability: new Map() });
    await expect(daemon.request({ type: "add_agent", agent: "grok" })).rejects.toMatchObject({ code: "agent_availability_pending" });
    expect(added(daemon)).toEqual(["claude", "codex"]);
  });

  it("removes an agent, answering one that is not added with an acknowledgement", async () => {
    const daemon = daemonWith(["claude", "codex"], []);
    await daemon.request({ type: "remove_agent", agent: "claude" });
    await daemon.request({ type: "remove_agent", agent: "grok" });
    expect(added(daemon)).toEqual(["codex"]);
  });

  it("takes a full new order, refusing one that is not a permutation of the added agents", async () => {
    const daemon = daemonWith(["claude", "codex", "grok"], []);
    await daemon.request({ type: "reorder_agents", agents: ["grok", "claude", "codex"] });
    expect(added(daemon)).toEqual(["grok", "claude", "codex"]);
    for (const agents of [["claude"], ["claude", "claude", "codex"], ["claude", "codex", "grok", "grok"]] as const) {
      await expect(daemon.request({ type: "reorder_agents", agents: [...agents] })).rejects.toMatchObject({ code: "agent_order_invalid" });
    }
    expect(added(daemon)).toEqual(["grok", "claude", "codex"]);
  });

  it("refuses every change to the list with the error a scenario names, leaving the list as it was", async () => {
    const refusal = { code: "agent_order_invalid", params: {}, message: "the new order must name each added agent exactly once" };
    const daemon = createFixtureDaemon({ agentListError: refusal, state: snapshotState({ settings: { ...settings, added_agents: ["claude", "codex"] } }) });
    daemon.store.setState({ agentAvailability: new Map([["grok" as const, { agent: "grok" as const, availability: "available" as const, default_account_dir: null }]]) });
    const bodies: Parameters<typeof daemon.request>[0][] = [{ type: "add_agent", agent: "grok" }, { type: "remove_agent", agent: "claude" }, { type: "reorder_agents", agents: ["codex", "claude"] }];
    for (const body of bodies) {
      await expect(daemon.request(body)).rejects.toMatchObject({ code: "agent_order_invalid" });
    }
    expect(added(daemon)).toEqual(["claude", "codex"]);
  });

  it("refuses to remove or reorder while the list is undecided", async () => {
    const daemon = daemonWith(null, []);
    await expect(daemon.request({ type: "remove_agent", agent: "claude" })).rejects.toMatchObject({ code: "added_agents_undecided" });
    await expect(daemon.request({ type: "reorder_agents", agents: ["claude"] })).rejects.toMatchObject({ code: "added_agents_undecided" });
    expect(added(daemon)).toBeNull();
  });
});

describe("the fixture daemon's trust answers", () => {
  const refusal = { code: "trust_directory_unreachable", params: { path: "/Users/dev/code/web" }, message: "unreachable" };
  const daemon = () => createFixtureDaemon({ trustParentDirError: refusal, state: snapshotState({}) });

  it("refuses a go-ahead that trusts the parent folder with the error a scenario names", async () => {
    await expect(daemon().request({ type: "confirm_trust", session: "s", remember: true, trust_parent_dir: true })).rejects.toMatchObject({ code: "trust_directory_unreachable", params: { path: "/Users/dev/code/web" } });
  });

  it("acknowledges a go-ahead for the project alone", async () => {
    await expect(daemon().request({ type: "confirm_trust", session: "s", remember: true, trust_parent_dir: false })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon().request({ type: "confirm_trust", session: "s", remember: true })).resolves.toEqual({ type: "ack", id: null });
  });
});

describe("the fixture daemon's session request answers", () => {
  const answer = { type: "answer_session_request", request_id: "r-1", approve: true } as const;

  it("refuses an answer with the error a scenario names", async () => {
    const refusal = { code: "session_request_not_waiting", params: {}, message: "not waiting" };
    await expect(createFixtureDaemon({ sessionRequestAnswerError: refusal, state: snapshotState({}) }).request(answer)).rejects.toMatchObject({ code: "session_request_not_waiting" });
  });

  it("acknowledges an answer otherwise", async () => {
    await expect(createFixtureDaemon({ state: snapshotState({}) }).request(answer)).resolves.toEqual({ type: "ack", id: null });
  });
});

describe("the fixture daemon's account login and default account name", () => {
  const settings = { ...DEFAULT_SETTINGS, default_clone_dir: "/Users/dev/Projects", added_agents: ["claude", "grok"] as ("claude" | "grok")[], accounts: [accountOf("work", "claude", "Work", "/Users/dev/.claude-work")] };
  const daemonWith = (outcome?: "succeeded" | "failed" | "timed_out") => createFixtureDaemon({ state: snapshotState({ settings }), loginOutcome: outcome });
  const login = (daemon: ReturnType<typeof daemonWith>, agent: string, account: string | null) =>
    daemon.store.getState().accountLogins.find((held) => held.agent === agent && held.account === account);

  it("walks a login from starting to waiting with its page, Grok Build's with the device code the agent shows itself", async () => {
    vi.useFakeTimers();
    try {
      const daemon = daemonWith();
      await daemon.request({ type: "start_account_login", agent: "claude", account: "work" });
      expect(login(daemon, "claude", "work")?.status).toBe("starting");
      await expect(daemon.request({ type: "start_account_login", agent: "claude", account: "work" })).rejects.toMatchObject({ code: "login_already_running" });
      await daemon.request({ type: "start_account_login", agent: "grok" });
      await vi.advanceTimersByTimeAsync(100);
      expect(login(daemon, "claude", "work")).toMatchObject({ status: "waiting", opened_by_agent: false, device_code: null });
      expect(login(daemon, "grok", null)).toMatchObject({ status: "waiting", opened_by_agent: true, device_code: "ABCD-1234" });
      await daemon.request({ type: "cancel_account_login", agent: "claude", account: "work" });
      expect(login(daemon, "claude", "work")?.status).toBe("cancelled");
      await expect(daemon.request({ type: "cancel_account_login", agent: "claude", account: "work" })).rejects.toMatchObject({ code: "login_not_running" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("ends a login as the scenario says", async () => {
    vi.useFakeTimers();
    try {
      const daemon = daemonWith("failed");
      await daemon.request({ type: "start_account_login", agent: "claude" });
      await vi.advanceTimersByTimeAsync(500);
      expect(login(daemon, "claude", null)).toMatchObject({ status: "failed", failure: { code: "login_failed" } });
    } finally {
      vi.useRealTimers();
    }
  });

  it("names and unnames the default account, refusing a name an account of the agent has, both ways", async () => {
    const daemon = daemonWith();
    await expect(daemon.request({ type: "set_default_account_name", agent: "claude", name: " work " })).rejects.toMatchObject({ code: "account_name_taken" });
    await daemon.request({ type: "set_default_account_name", agent: "claude", name: " Personal " });
    await daemon.request({ type: "set_default_account_name", agent: "grok", name: "Work" });
    expect(daemon.store.getState().settings.default_account_names).toEqual({ claude: "Personal", grok: "Work" });
    await expect(daemon.request({ type: "create_account", agent: "claude", name: "personal", config_dir: "/Users/dev/.p" })).rejects.toMatchObject({ code: "account_name_taken" });
    await daemon.request({ type: "set_default_account_name", agent: "claude", name: "" });
    expect(daemon.store.getState().settings.default_account_names).toEqual({ grok: "Work" });
    await daemon.request({ type: "set_default_account_name", agent: "grok", name: " DEFAULT " });
    expect(daemon.store.getState().settings.default_account_names).toEqual({});
  });
});

describe("the fixture daemon's shell terminals", () => {
  const daemonWithProject = (extra: Partial<Fixture> = {}) => createFixtureDaemon({ state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")] }), ...extra });

  it("creates a running, idle terminal, renames it and closes it", async () => {
    const daemon = daemonWithProject();
    const created = await daemon.request({ type: "create_shell_terminal", project: "p" });
    if (created.type !== "shell_terminal_created") throw new Error("expected shell_terminal_created");
    expect(created.shell_terminal).toMatchObject({ project_id: "p", worktree: null, title: "zsh", renamed: false, state: "running", busy: false });
    expect(daemon.store.getState().shellTerminals.get(created.shell_terminal.id)).toEqual(created.shell_terminal);

    await daemon.request({ type: "rename_shell_terminal", shell_terminal: created.shell_terminal.id, title: " server " });
    expect(daemon.store.getState().shellTerminals.get(created.shell_terminal.id)).toMatchObject({ title: "server", renamed: true });
    await expect(daemon.request({ type: "rename_shell_terminal", shell_terminal: created.shell_terminal.id, title: "  " })).rejects.toMatchObject({ code: "field_required" });

    await daemon.request({ type: "close_shell_terminal", shell_terminal: created.shell_terminal.id });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
    await expect(daemon.request({ type: "close_shell_terminal", shell_terminal: created.shell_terminal.id })).rejects.toMatchObject({ code: "unknown_shell_terminal" });
  });

  it("refuses a terminal for an unknown project and gives each terminal its own id", async () => {
    const daemon = daemonWithProject();
    await expect(daemon.request({ type: "create_shell_terminal", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
    const first = await daemon.request({ type: "create_shell_terminal", project: "p" });
    const second = await daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-side" });
    expect(new Set(daemon.store.getState().shellTerminals.keys()).size).toBe(2);
    if (first.type !== "shell_terminal_created" || second.type !== "shell_terminal_created") throw new Error("expected shell_terminal_created");
    expect(first.shell_terminal).toMatchObject({ worktree: null, cwd: "/Users/dev/code/site" });
    expect(second.shell_terminal).toMatchObject({ worktree: "wt-side", cwd: "/Users/dev/code/search-api-experiments" });
  });

  it("refuses a worktree the repository does not have, and one whose scope is missing", async () => {
    const daemon = daemonWithProject();
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", worktree: "w1" })).rejects.toMatchObject({ code: "worktree_unavailable" });
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({
      code: "source_unavailable",
      params: { path: "/Users/dev/.worktrees/search-api/review", cause: "missing", detail: "the project's directory is not in this worktree" },
    });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("refuses a worktree with the cause the worktree itself carries", async () => {
    const withCause = (cause: string | undefined) =>
      daemonWithProject({ git: { ...SAMPLE_GIT, worktrees: SAMPLE_GIT.worktrees.map((tree) => (tree.id === "wt-detached" ? { ...tree, scope_cause: cause } : tree)) } });
    await expect(withCause("not_a_directory").request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({
      code: "source_unavailable",
      params: { cause: "not_a_directory", detail: "not a directory" },
    });
    await expect(withCause("other").request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({ params: { cause: "other" } });
    await expect(withCause(undefined).request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({ params: { cause: "missing" } });
    await expect(withCause("something-new").request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({ params: { cause: "other" } });
    await expect(withCause("permission_denied").request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({
      code: "permission_denied",
      params: { path: "/Users/dev/.worktrees/search-api/review", detail: "Permission denied (os error 13)" },
    });
    const daemon = withCause("link");
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({
      code: "source_unavailable",
      params: { cause: "link", detail: "the path to the project's directory goes through a link in this worktree" },
    });
  });

  it("refuses the project's place in another worktree at the worktree's root joined with the scope", async () => {
    const daemon = daemonWithProject({ git: { ...SAMPLE_GIT, scope: "apps/web", worktrees: SAMPLE_GIT.worktrees.map((tree) => (tree.id === "wt-detached" ? { ...tree, scope_cause: "link" } : tree)) } });
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({
      code: "source_unavailable",
      params: { path: "/Users/dev/.worktrees/search-api/review/apps/web", cause: "link" },
    });
    const source = await daemon.request({ type: "get_project_source", project: "p" });
    if (source.type !== "project_source") throw new Error("expected project_source");
    expect(source.source).toMatchObject({ root: "/Users/dev/code/search-api/apps/web", git: { scope: "apps/web" } });
  });

  it("starts a terminal in the project's place in a worktree that has it", async () => {
    const daemon = daemonWithProject({ git: { ...SAMPLE_GIT, scope: "apps/web" } });
    const started = await daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-side" });
    if (started.type !== "shell_terminal_created") throw new Error("expected shell_terminal_created");
    expect(started.shell_terminal.cwd).toBe("/Users/dev/code/search-api-experiments/apps/web");
  });

  it("starts a terminal in the folder asked for below the place it starts in, and draws a fresh prompt there", async () => {
    const daemon = daemonWithProject({ git: { ...SAMPLE_GIT, scope: "apps/web" } });
    const own = await daemon.request({ type: "create_shell_terminal", project: "p", path: "src/ui" });
    if (own.type !== "shell_terminal_created") throw new Error("expected shell_terminal_created");
    expect(own.shell_terminal.cwd).toBe("/Users/dev/code/site/src/ui");
    expect(daemon.terminalUrl(own.shell_terminal.id)).toBe(terminalFixtureUrl("shell", own.shell_terminal.id, "/Users/dev/code/site/src/ui"));
    const side = await daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-side", path: "src" });
    if (side.type !== "shell_terminal_created") throw new Error("expected shell_terminal_created");
    expect(side.shell_terminal.cwd).toBe("/Users/dev/code/search-api-experiments/apps/web/src");
  });

  it("refuses a start with the scenario's error, after checking the project", async () => {
    const error = { code: "path_not_a_directory", params: { path: "src/ui" }, message: "not a directory" };
    const daemon = daemonWithProject({ shellTerminalStartError: error });
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", path: "src/ui" })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "create_shell_terminal", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("leaves a start unanswered when the scenario says so", async () => {
    const daemon = createFixtureDaemon({ shellTerminalStart: "pending", state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")] }) });
    const outcome = await Promise.race([daemon.request({ type: "create_shell_terminal", project: "p" }).then(() => "answered"), new Promise((resolve) => setTimeout(() => resolve("pending"), 50))]);
    expect(outcome).toBe("pending");
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("leaves a close unanswered when the scenario says so", async () => {
    const daemon = createFixtureDaemon({ shellTerminalClose: "pending", state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")], shellTerminals: [shellTerminalOf("sh-1", projectOf("p", "c", "Site"), "zsh", 0)] }) });
    const outcome = await Promise.race([daemon.request({ type: "close_shell_terminal", shell_terminal: "sh-1" }).then(() => "answered"), new Promise((resolve) => setTimeout(() => resolve("pending"), 50))]);
    expect(outcome).toBe("pending");
    expect(daemon.store.getState().shellTerminals.size).toBe(1);
  });

  it("drops the terminals of a deleted console's projects with them", async () => {
    const daemon = daemonWithProject();
    await daemon.request({ type: "create_shell_terminal", project: "p" });
    await daemon.request({ type: "delete_console", console: "c" });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("points a shell terminal's socket at its own behaviour, an exited one at its last screen, and a session's at the scenario's", () => {
    const shell = (id: string, state: "running" | "exited") => shellTerminalOf(id, projectOf("p", "c", "Site"), "zsh", 0, { state });
    const daemon = createFixtureDaemon({ terminal: "paths", shellTerminal: "refuse",
      state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")], shellTerminals: [shell("running", "running"), shell("done", "exited")] }),
    });
    expect(daemon.terminalUrl("running")).toBe(terminalFixtureUrl("refuse", "running"));
    expect(daemon.terminalUrl("done")).toBe(terminalFixtureUrl("exited", "done"));
    expect(daemon.terminalUrl("session")).toBe(terminalFixtureUrl("paths", "session"));
  });
});

describe("the fixture daemon's quota gate", () => {
  const accounts = [
    accountOf("a-spent", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-free", "claude", "Personal", "/Users/dev/.claude-personal"),
    accountOf("a-unread", "claude", "Client", "/Users/dev/.claude-client"),
    accountOf("a-model", "claude", "Model", "/Users/dev/.claude-model"),
    accountOf("a-stale", "claude", "Stale", "/Users/dev/.claude-stale"),
  ];
  const readings = [
    quotaOf("claude", "a-spent", "pro", [quotaWindow("session", 100, 30), quotaWindow("weekly", 100, 90)]),
    quotaOf("claude", "a-free", "pro", [quotaWindow("session", 99, 30)]),
    quotaOf("claude", "a-model", "pro", [quotaWindow("weekly_model", 100, 30, "Fable")]),
    quotaOf("claude", "a-stale", "pro", [quotaWindow("session", 100, -5)]),
    quotaOf("claude", null, "max", [quotaWindow("session", 100, null)]),
  ];
  const daemon = (withAccounts = accounts, withReadings = readings) => createFixtureDaemon({
    state: snapshotState({
      consoles: [consoleOf("c", "Main")],
      projects: [projectOf("p", "c", "Website")],
      sessions: [sessionOf("s", "c", "p", "Work", "idle")],
      settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex", "grok"], default_clone_dir: "/Users/dev/Projects", accounts: withAccounts },
      accountQuotas: withReadings,
    }),
  });

  it("refuses a session on an account whose window is used up, naming the window that resets last and when", async () => {
    const reading = readings[0].windows[1];
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: "a-spent" })).rejects.toMatchObject({
      code: "account_quota_exhausted",
      params: { agent: "Claude Code", account: "Work", window: "Weekly", window_kind: "weekly", window_minutes: String(reading.window_minutes), resets_at: String(reading.resets_at) },
    });
  });

  it("names the default account, and leaves out a reset time the agent did not give", async () => {
    const refused = await daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: null }).catch((error) => error);
    expect(refused).toMatchObject({
      code: "account_quota_exhausted",
      params: { agent: "Claude Code", account: "Default", window: "5-hour" },
    });
    expect(refused.params).not.toHaveProperty("resets_at");
  });

  it("keeps the session window's wire name `5-hour` and words the refusal as each agent's own interface does", async () => {
    const codex = [quotaOf("codex", null, "plus", [quotaWindow("session", 100, 30)])];
    const claudeRefusal = await daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: null }).catch((error) => error);
    expect(claudeRefusal).toMatchObject({ params: { agent: "Claude Code", window: "5-hour", window_kind: "session" } });
    expect(claudeRefusal.message).toContain("session limit");
    const codexRefusal = await daemon([], codex).request({ type: "handoff_session", session: "s", agent: "codex" }).catch((error) => error);
    expect(codexRefusal).toMatchObject({ params: { agent: "Codex", window: "5-hour", window_kind: "session", window_minutes: "300" } });
    expect(codexRefusal.message).toContain("5h limit");
  });

  it("carries the window's kind and length, and `model_weekly` only when the same reading has a model's own weekly window", async () => {
    const refusal = (windows: ReturnType<typeof quotaWindow>[]) =>
      daemon([], [quotaOf("claude", null, "max", windows)]).request({ type: "open_session", console_id: "c", project_id: "p", account: null }).catch((error) => error);
    const beside = await refusal([quotaWindow("weekly", 100, 30), quotaWindow("weekly_model", 10, 30, "Fable")]);
    expect(beside.params).toMatchObject({ window: "Weekly", window_kind: "weekly", window_minutes: "10080", model_weekly: "true" });
    expect(beside.message).toContain("its weekly limit for all models.");
    const alone = await refusal([quotaWindow("weekly", 100, 30)]);
    expect(alone.params).toMatchObject({ window: "Weekly", window_kind: "weekly" });
    expect(alone.params).not.toHaveProperty("model_weekly");
    expect(alone.message).toContain("its weekly limit.");
    const unknownLength = await refusal([{ ...quotaWindow("session", 100, 30), window_minutes: null }]);
    expect(unknownLength.params).toMatchObject({ window_kind: "session" });
    expect(unknownLength.params).not.toHaveProperty("window_minutes");
  });

  it("lets through an account nearly spent, one never read, one with only a model's window used up, and one whose window has reset", async () => {
    for (const account of ["a-free", "a-unread", "a-model", "a-stale"]) {
      await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account })).resolves.toMatchObject({ type: "session_opened" });
    }
  });

  it("refuses a switch to a used-up account and a handoff onto one, with the same code", async () => {
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-spent" })).rejects.toMatchObject({ code: "account_quota_exhausted" });
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-free" })).resolves.toEqual({ type: "ack", id: null });
    const grok = [quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 100, 60)])];
    await expect(daemon([], grok).request({ type: "handoff_session", session: "s", agent: "grok" })).rejects.toMatchObject({ code: "account_quota_exhausted", params: { window: "Weekly", window_kind: "credit_period", window_minutes: "10080" } });
  });
});

describe("the fixture daemon's disabled accounts", () => {
  const accounts = [
    accountOf("a-off", "claude", "Work", "/Users/dev/.claude-work", { disabled: true }),
    accountOf("a-on", "claude", "Personal", "/Users/dev/.claude-personal"),
    accountOf("a-codex", "codex", "Team", "/Users/dev/.codex-team"),
  ];
  const daemon = (disabledDefaults?: Agent[]) => createFixtureDaemon({
    state: snapshotState({
      consoles: [consoleOf("c", "Main")],
      projects: [projectOf("p", "c", "Website")],
      sessions: [sessionOf("s", "c", "p", "Work", "idle", { account_id: "a-off", config_dir: "/Users/dev/.claude-work" })],
      settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts, ...(disabledDefaults && { disabled_default_accounts: disabledDefaults }) },
    }),
  });

  it("refuses a session on an account that is off, and on the default account when it is, naming the account", async () => {
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: "a-off" })).rejects.toMatchObject({
      code: "account_disabled",
      params: { agent: "Claude Code", account: "Work" },
    });
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: null })).resolves.toMatchObject({ type: "session_opened" });
    await expect(daemon(["claude"]).request({ type: "open_session", console_id: "c", project_id: "p", account: null })).rejects.toMatchObject({
      code: "account_disabled",
      params: { agent: "Claude Code", account: "Default" },
    });
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: "a-on" })).resolves.toMatchObject({ type: "session_opened" });
  });

  it("refuses a switch to an account that is off and a handoff onto one, and lets the running session's own account be left", async () => {
    await expect(daemon(["codex"]).request({ type: "handoff_session", session: "s", agent: "codex", account: null })).rejects.toMatchObject({ code: "account_disabled", params: { agent: "Codex" } });
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-on" })).resolves.toEqual({ type: "ack", id: null });
    await expect(daemon(["claude"]).request({ type: "switch_session_account", session: "s", account: null })).rejects.toMatchObject({ code: "account_disabled" });
  });

  it("turns an account, or an agent's default account, off and on, and refuses an id that is no account of the agent", async () => {
    const fixture = daemon();
    await fixture.request({ type: "set_account_disabled", agent: "claude", account: "a-on", disabled: true });
    await fixture.request({ type: "set_account_disabled", agent: "claude", account: "a-off", disabled: false });
    await fixture.request({ type: "set_account_disabled", agent: "claude", account: null, disabled: true });
    await fixture.request({ type: "set_account_disabled", agent: "codex", account: null, disabled: true });
    await fixture.request({ type: "set_account_disabled", agent: "claude", account: null, disabled: true });
    const { settings } = fixture.store.getState();
    expect(settings.accounts.map((account) => account.disabled === true)).toEqual([false, true, false]);
    expect(settings.disabled_default_accounts).toEqual(["claude", "codex"]);
    await fixture.request({ type: "set_account_disabled", agent: "claude", account: null, disabled: false });
    expect(fixture.store.getState().settings.disabled_default_accounts).toEqual(["codex"]);
    await expect(fixture.request({ type: "set_account_disabled", agent: "claude", account: "a-codex", disabled: true })).rejects.toMatchObject({ code: "unknown_account" });
  });
});

describe("the fixture daemon's accounts that are not logged in", () => {
  const accounts = [
    accountOf("a-out", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-new", "claude", "Client", "/Users/dev/.claude-client"),
    accountOf("a-slow", "claude", "Personal", "/Users/dev/.claude-personal"),
    accountOf("a-team", "codex", "Team", "/Users/dev/.codex-team"),
  ];
  const daemon = () => createFixtureDaemon({
    state: snapshotState({
      consoles: [consoleOf("c", "Main")],
      projects: [projectOf("p", "c", "Website")],
      sessions: [sessionOf("s", "c", "p", "Default", "idle")],
      settings: { ...DEFAULT_SETTINGS, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts },
      accountQuotas: [
        quotaOf("codex", "a-team", null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Codex" }, "not signed in") }),
        quotaOf("claude", "a-out", null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "not signed in") }),
        quotaOf("claude", "a-new", null, [], { unavailable: quotaFailure("quota_config_dir_missing", { agent: "Claude Code" }, "no directory") }),
        quotaOf("claude", "a-slow", null, [], { unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "timed out") }),
      ],
    }),
  });

  it("refuses a session on and a switch to an account whose last reading says it is not logged in, naming the account", async () => {
    for (const account of ["a-out", "a-new"]) {
      const name = account === "a-out" ? "Work" : "Client";
      await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account })).rejects.toMatchObject({ code: "account_not_signed_in", params: { agent: "Claude Code", account: name } });
      await expect(daemon().request({ type: "switch_session_account", session: "s", account })).rejects.toMatchObject({ code: "account_not_signed_in", params: { account: name } });
    }
  });

  it("refuses a handoff to such an account of another agent", async () => {
    await expect(daemon().request({ type: "handoff_session", session: "s", agent: "codex", account: "a-team" })).rejects.toMatchObject({ code: "account_not_signed_in", params: { agent: "Codex", account: "Team" } });
  });

  it("lets a reading that failed for another reason through", async () => {
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: "a-slow" })).resolves.toMatchObject({ type: "session_opened" });
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-slow" })).resolves.toEqual({ type: "ack", id: null });
  });
});

describe("the fixture daemon's session history", () => {
  const archived = sessionOf("archived", "c", "p", "Archived", "archived");
  const base = { state: snapshotState({ sessions: [archived] }) };
  const messages = [{ speaker: "user" as const, text: "Add an index", turn: 1 }];

  it("answers a session's messages, an empty history for one not listed, and unavailable for a record it cannot read", async () => {
    const request = { type: "read_session_history" as const, session: archived.id };
    await expect(createFixtureDaemon({ ...base, sessionHistory: { [archived.id]: messages } }).request(request)).resolves.toEqual({ type: "session_history", id: null, session: archived.id, messages });
    await expect(createFixtureDaemon(base).request(request)).resolves.toEqual({ type: "session_history", id: null, session: archived.id, messages: [] });
    await expect(createFixtureDaemon({ ...base, sessionHistory: { [archived.id]: "unavailable" } }).request(request)).resolves.toMatchObject({ messages: [], unavailable: true });
    await expect(createFixtureDaemon(base).request({ type: "read_session_history", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("rejects a refused history read and answers a later retry with the current fixture result", async () => {
    const error = { code: "internal_error", params: { detail: "Resource temporarily unavailable" }, message: "Resource temporarily unavailable" };
    const sessionHistory: NonNullable<Parameters<typeof createFixtureDaemon>[0]["sessionHistory"]> = { [archived.id]: error };
    const daemon = createFixtureDaemon({ ...base, sessionHistory });
    const request = { type: "read_session_history" as const, session: archived.id };
    await expect(daemon.request(request)).rejects.toMatchObject({ code: error.code, params: error.params });
    expect(daemon.store.getState().sessions.get(archived.id)?.status).toBe("archived");
    sessionHistory[archived.id] = messages;
    await expect(daemon.request(request)).resolves.toEqual({ type: "session_history", id: null, session: archived.id, messages });
  });

  it("can leave a read, and a reopen, unanswered", async () => {
    const daemon = createFixtureDaemon({ ...base, sessionHistory: { [archived.id]: "pending" }, sessionResumes: { [archived.id]: "pending" } });
    const answered = vi.fn();
    void daemon.request({ type: "read_session_history", session: archived.id }).then(answered);
    void daemon.request({ type: "resume_session", session: archived.id }).then(answered);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(answered).not.toHaveBeenCalled();
    expect(daemon.store.getState().sessions.get(archived.id)?.status).toBe("archived");
  });
});

describe("the fixture daemon's configuration export", () => {
  const exportProjects = async (include_paths?: boolean) => {
    const daemon = createFixtureDaemon({
      state: snapshotState({
        consoles: [consoleOf("c-1", "Work")],
        projects: [
          projectOf("p-1", "c-1", "Website", { path: "/Users/ana/code/website", source: "git", remote_url: "git@github.com:acme/website.git" }),
          projectOf("p-2", "c-1", "Local", { path: "/Users/ana/code/local", source: "git", remote_url: "/Users/ana/origin.git" }),
          projectOf("p-3", "c-1", "File", { path: "/Users/ana/code/file", source: "git", remote_url: "file:///Users/ana/origin.git" }),
          projectOf("p-4", "c-1", "Root", { path: "/", source: "git", remote_url: "https://example.com/root.git" }),
        ],
      }),
    });
    const reply = (await daemon.request({ type: "export_configuration", include_paths })) as { document: ReturnType<typeof exportedDocument> };
    return reply.document.consoles.flatMap((console) => console.projects.map((project) => ({ name: project.name, path: project.path, remote_url: project.remote_url })));
  };

  it("records a project's path and remote in full, unless asked to leave paths out", async () => {
    const full = [
      { name: "File", path: "/Users/ana/code/file", remote_url: "file:///Users/ana/origin.git" },
      { name: "Local", path: "/Users/ana/code/local", remote_url: "/Users/ana/origin.git" },
      { name: "Root", path: "/", remote_url: "https://example.com/root.git" },
      { name: "Website", path: "/Users/ana/code/website", remote_url: "git@github.com:acme/website.git" },
    ];
    expect(await exportProjects()).toEqual(full);
    expect(await exportProjects(true)).toEqual(full);
  });

  it("records only the directory's name of a project, and only a remote that can be cloned, when paths are left out", async () => {
    expect(await exportProjects(false)).toEqual([
      { name: "File", path: "file", remote_url: null },
      { name: "Local", path: "local", remote_url: null },
      { name: "Root", path: "", remote_url: "https://example.com/root.git" },
      { name: "Website", path: "website", remote_url: "git@github.com:acme/website.git" },
    ]);
  });

  describe("regions", () => {
    const account = accountOf;
    const daemon = createFixtureDaemon({
      state: snapshotState({
        consoles: [consoleOf("c-1", "Work"), consoleOf("c-2", "Home")],
        projects: [projectOf("p-1", "c-1", "Website")],
        settings: {
          ...DEFAULT_SETTINGS,
          added_agents: ["codex", "claude"],
          default_clone_dir: "/Users/dev/Projects",
          accounts: [
            account("a-1", "claude", "Work", "/Users/dev/.claude-work", { share_default_config: true }),
            account("a-2", "claude", "Studio", "/Volumes/Shared/claude-studio", { disabled: true }),
            account("a-3", "codex", "Home", "/Users/dev"),
          ],
          account_order: { claude: ["a-2", null, "a-1"] },
          default_account_names: { claude: "Personal" },
        },
      }),
    });
    const exported = async (request: Partial<Extract<ClientRequestBody, { type: "export_configuration" }>> = {}) =>
      ((await daemon.request({ type: "export_configuration", ...request })) as { document: ReturnType<typeof exportedDocument> }).document;

    it("settles the regions an absent field leaves open as the daemon does", async () => {
      const everything = await exported();
      expect(everything.consoles.map((console) => console.name).sort()).toEqual(["Home", "Work"]);
      expect(everything.settings).toBeDefined();
      expect(everything).not.toHaveProperty("accounts");
      const one = await exported({ console: "c-1" });
      expect(one.consoles.map((console) => console.name)).toEqual(["Work"]);
      expect(one.settings).toBeNull();
      expect(one).not.toHaveProperty("accounts");
    });

    it("chooses the consoles, the settings and the accounts independently", async () => {
      const none = await exported({ consoles: "none", include_settings: true });
      expect(none.consoles).toEqual([]);
      expect(none.settings).toBeDefined();
      const oneWithSettings = await exported({ consoles: "one", console: "c-2", include_settings: true, include_accounts: true });
      expect(oneWithSettings.consoles.map((console) => console.name)).toEqual(["Home"]);
      expect(oneWithSettings.settings).toBeDefined();
      expect(oneWithSettings.accounts).toBeDefined();
      const allWithoutSettings = await exported({ consoles: "all", include_settings: false });
      expect(allWithoutSettings.consoles).toHaveLength(2);
      expect(allWithoutSettings.settings).toBeNull();
    });

    it("refuses an export of nothing, one of a console with none named, and one of a console it does not have", async () => {
      await expect(exported({ consoles: "none" })).rejects.toMatchObject({ code: "export_empty" });
      await expect(exported({ consoles: "none", include_settings: false, include_accounts: false })).rejects.toMatchObject({ code: "export_empty" });
      await expect(exported({ consoles: "one" })).rejects.toMatchObject({ code: "field_required" });
      await expect(exported({ console: "c-9" })).rejects.toMatchObject({ code: "unknown_console" });
    });

    it("exports the accounts in their display order with the default account's place, and no id", async () => {
      const { accounts } = await exported({ consoles: "none", include_accounts: true });
      expect(accounts).toEqual({
        added_agents: ["codex", "claude"],
        agents: [
          {
            agent: "claude",
            default_account: { name: "Personal", position: 1 },
            accounts: [
              { name: "Studio", directory: "/Volumes/Shared/claude-studio", share_default_config: false, disabled: true },
              { name: "Work", directory: "~/.claude-work", share_default_config: true, disabled: false },
            ],
          },
          { agent: "codex", default_account: { name: null, position: 0 }, accounts: [{ name: "Home", directory: "~", share_default_config: false, disabled: false }] },
        ],
      });
    });

    it("writes every account directory, home-relative or absolute, whatever the paths choice", async () => {
      const { accounts } = await exported({ consoles: "none", include_accounts: true, include_paths: false });
      expect(accounts?.agents[0].accounts.map((account) => account.directory)).toEqual(["/Volumes/Shared/claude-studio", "~/.claude-work"]);
    });
  });
});

describe("the fixture daemon's git workflow settings", () => {
  const console_ = consoleOf("c", "Main");
  const daemonWith = (own = FOLLOWING_GIT_WORKFLOW) =>
    createFixtureDaemon({
      state: snapshotState({ consoles: [console_], projects: [projectOf("p", console_.id, "Website", { git_workflow: own })] }),
    });

  it("changes only the application-wide items a settings request names", async () => {
    const daemon = daemonWith();
    await daemon.request({ type: "update_settings", git_workflow: { delivery: "pull_request", revert_gate: false } });
    expect(daemon.store.getState().settings.git_workflow).toEqual({ ...DEFAULT_GIT_WORKFLOW, delivery: "pull_request", revert_gate: false });
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.git_workflow).toEqual({ ...DEFAULT_GIT_WORKFLOW, delivery: "pull_request", revert_gate: false });
  });

  it("sets, keeps and clears a project's own items as update_project says", async () => {
    const daemon = daemonWith({ ...FOLLOWING_GIT_WORKFLOW, delivery: "merge" });
    await daemon.request({ type: "update_project", project: "p", git_workflow: { revert_gate: false } });
    expect(daemon.store.getState().projects.get("p")?.git_workflow).toEqual({ ...FOLLOWING_GIT_WORKFLOW, delivery: "merge", revert_gate: false });
    await daemon.request({ type: "update_project", project: "p", git_workflow: { delivery: null } });
    expect(daemon.store.getState().projects.get("p")?.git_workflow).toEqual({ ...FOLLOWING_GIT_WORKFLOW, revert_gate: false });
    await daemon.request({ type: "update_project", project: "p", name: "Site" });
    expect(daemon.store.getState().projects.get("p")?.git_workflow).toEqual({ ...FOLLOWING_GIT_WORKFLOW, revert_gate: false });
  });

  it("gives an added project the items the request carries and lets the rest follow", async () => {
    const daemon = daemonWith();
    await daemon.request({ type: "add_project", console_id: console_.id, source: "local", path: "/Users/dev/code/new", git_workflow: { development: "in_place", delivery: null } });
    const added = [...daemon.store.getState().projects.values()].find((project) => project.path === "/Users/dev/code/new");
    expect(added?.git_workflow).toEqual({ ...FOLLOWING_GIT_WORKFLOW, development: "in_place" });
  });
});

describe("the fixture daemon's process ledger", () => {
  const project = projectOf("p", "c", "Website");
  const other = projectOf("p-other", "c", "Search API");
  const running = sessionOf("running", "c", project.id, "Running", "idle");
  const working = sessionOf("working", "c", project.id, "Working", "working");
  const idleElsewhere = sessionOf("elsewhere", "c", other.id, "Elsewhere", "idle");
  const interrupted = sessionOf("interrupted", "c", project.id, "Interrupted", "interrupted");
  const old = processTreeOf("1:1", running, "pnpm dev", 90);
  const young = processTreeOf("2:1", running, "pnpm test --watch", 0.1);
  const mine = processTreeOf("3:1", working, "node watch.mjs", 30);
  const away = processTreeOf("4:1", idleElsewhere, "caddy run", 30);
  const stale = processTreeOf("5:1", interrupted, "tail -f log", 30);
  const left = leftoverOf("6:1", "vite --port 5180", 600);
  const scenario = {
    state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [project, other], sessions: [running, working, idleElsewhere, interrupted] }),
    processes: { trees: [young, old, mine, away, stale], projectProcesses: { [project.id]: [left] } },
  };
  const list = (daemon: ReturnType<typeof createFixtureDaemon>, target: { session_id: string } | { project_id: string }) =>
    daemon.request({ type: "list_processes", ...target }).then((reply) => (reply.type === "processes_listed" ? reply.trees.map((tree) => tree.id) : []));

  it("lists a running session's trees oldest first, and refuses one with no process or no record", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(list(daemon, { session_id: running.id })).resolves.toEqual(["1:1", "2:1"]);
    await expect(daemon.request({ type: "list_processes", session_id: interrupted.id })).rejects.toMatchObject({ code: "session_not_running" });
    await expect(daemon.request({ type: "list_processes", session_id: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("lists a project's running sessions' trees, then its leftovers, and refuses an unknown project", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(list(daemon, { project_id: project.id })).resolves.toEqual(["1:1", "2:1", "3:1", "6:1"]);
    await expect(list(daemon, { project_id: other.id })).resolves.toEqual(["4:1"]);
    await expect(daemon.request({ type: "list_processes", project_id: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
  });

  it("counts every project's trees in the store's order, then each console session's, and leaves out a project that is gone", async () => {
    const hub = sessionOf("hub", "c", undefined, "Hub", "idle");
    const quiet = sessionOf("quiet", "c", undefined, "Quiet", "idle");
    const hubTree = processTreeOf("7:1", hub, "caddy run", 30);
    const kept = keptOf("8:1", "node serve.mjs", 300);
    const daemon = createFixtureDaemon({
      state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [project, other], sessions: [running, working, idleElsewhere, interrupted, hub, quiet] }),
      processes: { trees: [young, old, mine, away, stale, hubTree], projectProcesses: { [project.id]: [left], gone: [kept] } },
    });
    const counts = async () => {
      const reply = await daemon.request({ type: "list_all_processes" });
      if (reply.type !== "all_processes_listed") throw new Error(`unexpected reply ${reply.type}`);
      return reply.counts;
    };
    expect(await counts()).toEqual([
      { project_id: project.id, trees: 4 },
      { project_id: other.id, trees: 1 },
      { session_id: hub.id, trees: 1 },
    ]);
    await daemon.request({ type: "stop_processes", ids: ["7:1", "4:1"] });
    expect(await counts()).toEqual([{ project_id: project.id, trees: 4 }]);
  });

  it("lists in an archive's inspection only the trees that have run for 30 seconds, and none unless asked", async () => {
    const daemon = createFixtureDaemon(scenario);
    const inspect = (processes?: boolean) => daemon.request({ type: "inspect_session_branches", sessions: [running.id, working.id, interrupted.id], ...(processes ? { processes } : {}) });
    await expect(inspect(true)).resolves.toMatchObject({ processes: [old, mine] });
    expect(await inspect()).not.toHaveProperty("processes");
    await expect(createFixtureDaemon({ ...scenario, processes: undefined }).request({ type: "inspect_session_branches", sessions: [running.id], processes: true })).resolves.not.toHaveProperty("processes");
  });

  it("stops the trees that still qualify, and leaves out the ones that do not", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "stop_processes", ids: ["1:1", "5:1", "6:1", "nope"] })).resolves.toEqual({ type: "processes_stopped", id: null, ids: ["1:1", "6:1"] });
    await expect(list(daemon, { project_id: project.id })).resolves.toEqual(["2:1", "3:1"]);
  });

  it("keeps the trees of a running session that still qualify, which then stay listed as the project's, and nothing of a project process", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "keep_processes", ids: ["1:1", "5:1", "6:1"] })).resolves.toEqual({ type: "processes_kept", id: null, ids: ["1:1"] });
    await expect(list(daemon, { project_id: project.id })).resolves.toEqual(["2:1", "3:1", "6:1", "1:1"]);
    await expect(list(daemon, { session_id: running.id })).resolves.toEqual(["2:1"]);
    const kept = await daemon.request({ type: "list_processes", project_id: project.id });
    expect(kept.type === "processes_listed" ? kept.trees.filter((tree) => tree.kept).map((tree) => [tree.id, tree.session_id]) : []).toEqual([["1:1", undefined]]);
  });

  it("keeps nothing of a console session", async () => {
    const hub = sessionOf("hub", "c", undefined, "Hub", "idle");
    const own = processTreeOf("7:1", hub, "caddy run", 30);
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [project], sessions: [hub] }), processes: { trees: [own] } });
    await expect(daemon.request({ type: "keep_processes", ids: ["7:1"] })).resolves.toEqual({ type: "processes_kept", id: null, ids: [] });
    await expect(list(daemon, { session_id: hub.id })).resolves.toEqual(["7:1"]);
  });

  it("ends an archived session's trees but the ones the user keeps, which stay kept when the archive is refused", async () => {
    const archived = createFixtureDaemon(scenario);
    await archived.request({ type: "archive_session", session: running.id, keep_process_ids: ["1:1"] });
    await expect(list(archived, { project_id: project.id })).resolves.toEqual(["3:1", "6:1", "1:1"]);
    const refused = createFixtureDaemon({ ...scenario, sessionArchiveError: { code: "store_write_failed", params: { cause: "disk_full", detail: "database or disk is full: Error code 13: Insertion failed because database is full" }, message: "Octoboard could not read or write its own data: database or disk is full: Error code 13: Insertion failed because database is full" } });
    await expect(refused.request({ type: "archive_session", session: running.id, keep_process_ids: ["1:1"] })).rejects.toMatchObject({ code: "store_write_failed" });
    await expect(list(refused, { session_id: running.id })).resolves.toEqual(["2:1"]);
  });

  it("answers the first list and refuses the polls that follow it once a second has passed, when the scenario says so", async () => {
    const error = { code: "internal_error", params: { detail: "no" }, message: "no" };
    vi.useFakeTimers();
    try {
      const daemon = createFixtureDaemon({ ...scenario, processes: { ...scenario.processes, listLater: error } });
      await expect(list(daemon, { session_id: running.id })).resolves.toEqual(["1:1", "2:1"]);
      await expect(list(daemon, { session_id: running.id })).resolves.toEqual(["1:1", "2:1"]);
      vi.advanceTimersByTime(3000);
      await expect(daemon.request({ type: "list_processes", session_id: running.id })).rejects.toMatchObject({ code: "internal_error" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("refuses the list of just the project the scenario names, leaving the others and the session lists answered", async () => {
    const error = { code: "internal_error", params: { detail: "no" }, message: "no" };
    const daemon = createFixtureDaemon({ ...scenario, processes: { ...scenario.processes, projectList: { [project.id]: error } } });
    await expect(daemon.request({ type: "list_processes", project_id: project.id })).rejects.toMatchObject({ code: "internal_error" });
    await expect(list(daemon, { project_id: other.id })).resolves.toEqual(["4:1"]);
    await expect(list(daemon, { session_id: running.id })).resolves.toEqual(["1:1", "2:1"]);
  });

  it("answers what the scenario stages: a list or an action refused, or never answered", async () => {
    const error = { code: "internal_error", params: { detail: "no" }, message: "no" };
    await expect(createFixtureDaemon({ ...scenario, processes: { list: error } }).request({ type: "list_processes", session_id: running.id })).rejects.toMatchObject({ code: "internal_error" });
    await expect(createFixtureDaemon({ ...scenario, processes: { stop: error } }).request({ type: "stop_processes", ids: ["1:1"] })).rejects.toMatchObject({ code: "internal_error" });
    await expect(createFixtureDaemon({ ...scenario, processes: { keep: error } }).request({ type: "keep_processes", ids: ["1:1"] })).rejects.toMatchObject({ code: "internal_error" });
    for (const processes of [{ list: "pending" }, { stop: "pending" }, { keep: "pending" }] as const) {
      const answered = vi.fn();
      const daemon = createFixtureDaemon({ ...scenario, processes });
      const request = "list" in processes ? { type: "list_processes" as const, session_id: running.id } : "stop" in processes ? { type: "stop_processes" as const, ids: ["1:1"] } : { type: "keep_processes" as const, ids: ["1:1"] };
      void daemon.request(request).then(answered, answered);
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(answered).not.toHaveBeenCalled();
    }
  });
});

describe("the fixture daemon's rate limit reset credits", () => {
  const personal = accountOf("personal", "codex", "Personal", "/Users/dev/.codex-personal");
  const withCredits = (credits: ReturnType<typeof resetCreditsOf> | undefined) => ({
    state: snapshotState({
      settings: { ...DEFAULT_SETTINGS, added_agents: [], accounts: [personal, accountOf("work", "claude", "Work", "/Users/dev/.claude-work")] },
      accountQuotas: [quotaOf("codex", "personal", "pro", [quotaWindow("session", 40, 120), quotaWindow("weekly", 100, 3 * 24 * 60)], { reset_credits: credits })],
    }),
  });
  const rereadDone = () => new Promise((resolve) => setTimeout(resolve, 150));
  const reading = (daemon: ReturnType<typeof createFixtureDaemon>) => daemon.store.getState().accountQuotas[0]!;

  it("takes the used credit off the held reading at once and reads the quota again after, with the windows full", async () => {
    const daemon = createFixtureDaemon(withCredits(resetCreditsOf([resetCreditOf("a", 60), resetCreditOf("b", 600)])));
    const fetched = reading(daemon).fetched_at;
    await expect(daemon.request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toEqual({ type: "reset_credit_used", id: null, outcome: "reset" });
    expect(reading(daemon)).toMatchObject({ fetched_at: fetched, windows: [{ used_percent: 40 }, { used_percent: 100 }], reset_credits: { available_count: 1, credits: [expect.objectContaining({ id: "b" })] } });
    await rereadDone();
    expect(reading(daemon).windows.map((window) => window.used_percent)).toEqual([0, 0]);
    expect(reading(daemon).fetched_at).toBeGreaterThan(fetched);
    await daemon.request({ type: "use_codex_reset_credit", account: "personal" });
    expect(reading(daemon).reset_credits).toBeUndefined();
  });

  it("takes the first credit that has not expired, and says none is left where only expired ones remain", async () => {
    const daemon = createFixtureDaemon(withCredits(resetCreditsOf([resetCreditOf("old", -5), resetCreditOf("b", 600)])));
    await daemon.request({ type: "use_codex_reset_credit", account: "personal" });
    expect(reading(daemon).reset_credits).toEqual({ available_count: 1, credits: [expect.objectContaining({ id: "old" })] });
    const expired = createFixtureDaemon(withCredits(resetCreditsOf([resetCreditOf("old", -5)])));
    await expect(expired.request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toMatchObject({ outcome: "no_credit" });
  });

  it("spends a credit only on reset: already_redeemed and nothing_to_reset leave the credits and refill nothing", async () => {
    const credits = resetCreditsOf([resetCreditOf("a", 60), resetCreditOf("b", 600)]);
    for (const outcome of ["already_redeemed", "nothing_to_reset"] as const) {
      const daemon = createFixtureDaemon({ ...withCredits(credits), resetCreditUse: outcome });
      const fetched = reading(daemon).fetched_at;
      await expect(daemon.request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toMatchObject({ outcome });
      expect(reading(daemon).reset_credits).toEqual(credits);
      await rereadDone();
      expect(reading(daemon)).toMatchObject({ reset_credits: credits, windows: [{ used_percent: 40 }, { used_percent: 100 }] });
      expect(reading(daemon).fetched_at).toBeGreaterThan(fetched);
    }
  });

  it("keeps the count and drops the list where the credit used was not named, and reads again after a refusal", async () => {
    const unnamed = createFixtureDaemon(withCredits(resetCreditsOf(null, 3)));
    await unnamed.request({ type: "use_codex_reset_credit", account: "personal" });
    expect(reading(unnamed).reset_credits).toEqual({ available_count: 2, credits: null });
    const refused = createFixtureDaemon({ ...withCredits(resetCreditsOf([resetCreditOf("a", 60)])), resetCreditUse: { code: "reset_credit_failed", params: { agent: "Codex", detail: "x" }, message: "x" } });
    const fetched = reading(refused).fetched_at;
    await expect(refused.request({ type: "use_codex_reset_credit", account: "personal" })).rejects.toMatchObject({ code: "reset_credit_failed" });
    await rereadDone();
    expect(reading(refused).fetched_at).toBeGreaterThan(fetched);
    expect(reading(refused).reset_credits?.available_count).toBe(1);
    for (const code of ["binary_not_found", "reset_credit_in_progress", "shell_environment_timeout"]) {
      const staged = { code, params: { agent: "Codex" }, message: "x" };
      const early = createFixtureDaemon({ ...withCredits(resetCreditsOf([resetCreditOf("a", 60)])), resetCreditUse: staged });
      const before = reading(early).fetched_at;
      const request = early.request({ type: "use_codex_reset_credit", account: "personal" });
      await expect(request).rejects.toMatchObject({ code });
      await rereadDone();
      expect(reading(early).fetched_at).toBe(before);
    }
  });

  it("reads again later, never, or with a failure that keeps the windows and credits, as the scenario stages", async () => {
    const credits = resetCreditsOf([resetCreditOf("a", 60), resetCreditOf("b", 600)]);
    const failure = quotaFailure("quota_timed_out", { agent: "Codex" }, "Codex took too long to report its usage.");
    const later = createFixtureDaemon({ ...withCredits(credits), resetCreditReading: { after: 300 } });
    await later.request({ type: "use_codex_reset_credit", account: "personal" });
    await rereadDone();
    expect(reading(later).windows[1]!.used_percent).toBe(100);
    await new Promise((resolve) => setTimeout(resolve, 250));
    expect(reading(later).windows[1]!.used_percent).toBe(0);
    const never = createFixtureDaemon({ ...withCredits(credits), resetCreditReading: "never" });
    await never.request({ type: "use_codex_reset_credit", account: "personal" });
    await rereadDone();
    expect(reading(never).windows[1]!.used_percent).toBe(100);
    const failed = createFixtureDaemon({ ...withCredits(credits), resetCreditReading: { unavailable: failure } });
    const before = reading(failed).fetched_at;
    await failed.request({ type: "use_codex_reset_credit", account: "personal" });
    await rereadDone();
    expect(reading(failed)).toMatchObject({ fetched_at: before, unavailable: failure, reset_credits: { available_count: 1, credits: [expect.objectContaining({ id: "b" })] }, windows: [{ used_percent: 40 }, { used_percent: 100 }] });
  });

  it("keeps the reading for nothing_to_reset, drops the credits for no_credit, and answers no_credit where none is held", async () => {
    const credits = resetCreditsOf([resetCreditOf("a", 60)]);
    const kept = createFixtureDaemon({ ...withCredits(credits), resetCreditUse: "nothing_to_reset" });
    await expect(kept.request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toMatchObject({ outcome: "nothing_to_reset" });
    await rereadDone();
    expect(reading(kept)).toMatchObject({ reset_credits: credits, windows: [{ used_percent: 40 }, { used_percent: 100 }] });
    const gone = createFixtureDaemon({ ...withCredits(credits), resetCreditUse: "no_credit" });
    await expect(gone.request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toMatchObject({ outcome: "no_credit" });
    await rereadDone();
    expect(reading(gone).reset_credits).toBeUndefined();
    await expect(createFixtureDaemon(withCredits(undefined)).request({ type: "use_codex_reset_credit", account: "personal" })).resolves.toMatchObject({ outcome: "no_credit" });
  });

  it("refuses an account that is not a Codex one, a staged failure, and keeps a staged request pending", async () => {
    const daemon = createFixtureDaemon(withCredits(resetCreditsOf([resetCreditOf("a", 60)])));
    await expect(daemon.request({ type: "use_codex_reset_credit", account: "work" })).rejects.toMatchObject({ code: "unknown_account" });
    const failing = createFixtureDaemon({ ...withCredits(resetCreditsOf([resetCreditOf("a", 60)])), resetCreditUse: { code: "reset_credit_failed", params: { agent: "Codex", detail: "x" }, message: "x" } });
    await expect(failing.request({ type: "use_codex_reset_credit", account: "personal" })).rejects.toMatchObject({ code: "reset_credit_failed" });
    const answered = vi.fn();
    void createFixtureDaemon({ ...withCredits(resetCreditsOf([resetCreditOf("a", 60)])), resetCreditUse: "pending" }).request({ type: "use_codex_reset_credit", account: "personal" }).then(answered, answered);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(answered).not.toHaveBeenCalled();
  });
});
