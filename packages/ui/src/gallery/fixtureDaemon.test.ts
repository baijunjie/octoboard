// @vitest-environment jsdom
import { createElement } from "react";
import { describe, expect, it, vi } from "vitest";

import type { ToastRequest } from "../store";

import type { Agent, RequestBody } from "../protocol";
import { exportedDocument } from "./fixtureConfiguration";
import { createFixtureDaemon } from "./fixtureDaemon";
import { terminalFixtureUrl } from "./fakeTerminal";
import { RESTORE_FILES, RESTORE_GIT, RESTORED_PROJECT } from "./fixtures/restore";
import { quotaOf, quotaWindow } from "./fixtures/quotas";
import { accountSharingOf, branchItemOf, consoleOf, gitStatusOf, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState, worktreeItemOf } from "./fixtures/builders";

describe("the fixture daemon's session archiving", () => {
  it("archives running and interrupted descendants and supports reopening", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "idle");
    const lead = sessionOf("lead", "c", "p", "Lead", "interrupted", { bound_to: owner.id });
    const child = sessionOf("child", "c", "p", "Child", "idle", { bound_to: lead.id });
    const daemon = createFixtureDaemon({ id: "archive", group: "Focus mode", category: "normal", title: "", state: snapshotState({ sessions: [owner, lead, child] }) });
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
    const error = { code: "store_io_failed", params: { detail: "Disk full" }, message: "Disk full" };
    const daemon = createFixtureDaemon({ id: "archive-failed", group: "Archive view", category: "exceptional", title: "", state: snapshotState({ sessions: [owner, child] }), sessionArchiveError: error });
    await expect(daemon.request({ type: "archive_session", session: owner.id })).rejects.toMatchObject(error);
    expect([...daemon.store.getState().sessions.values()]).toEqual([owner, child]);
  });
});

describe("the fixture daemon's conversation lookup", () => {
  const ID = "3f2b8c1e-7a4d-4e9b-9c55-0d6a1b2e4f70";
  const ask = (conversationLocation?: NonNullable<Parameters<typeof createFixtureDaemon>[0]["conversationLocation"]>) =>
    createFixtureDaemon({ id: "locate", group: "Sidebar sessions", category: "normal", title: "", conversationLocation }).request({ type: "locate_conversation_to_resume", project: "p", agent_session_id: ID });

  it("answers with the location the scenario gives, and not found by default", async () => {
    await expect(ask({ kind: "record", agent: "codex", account: null })).resolves.toEqual({ type: "conversation_located", location: { kind: "record", agent: "codex", account: null } });
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

describe("the fixture daemon's session reopening", () => {
  it("restores an archived session and its archived owners while preserving its children", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "archived", { ended_at: 10 });
    const lead = sessionOf("lead", "c", "p", "Lead", "archived", { bound_to: owner.id, ended_at: 20 });
    const child = sessionOf("child", "c", "p", "Child", "archived", { bound_to: lead.id, ended_at: 30 });
    const daemon = createFixtureDaemon({ id: "reopen", group: "Focus mode", category: "normal", title: "", state: snapshotState({ sessions: [owner, lead, child] }) });
    await expect(daemon.request({ type: "resume_session", session: lead.id })).resolves.toEqual({ type: "ack" });
    expect(daemon.store.getState().sessions.get(lead.id)).toEqual({ ...lead, status: "idle", ended_at: undefined });
    expect(daemon.store.getState().sessions.get(owner.id)).toEqual({ ...owner, status: "idle", ended_at: undefined });
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
  const scenario = { id: "handoff", group: "Sidebar sessions" as const, category: "normal" as const, title: "", state };

  it("starts the receiving session in the original's place and archives the original", async () => {
    const daemon = createFixtureDaemon(scenario);
    const reply = await daemon.request({ type: "hand_off_session", session: lead.id, agent: "codex" });
    if (reply.type !== "session_opened") throw new Error("expected session_opened");
    const sessions = daemon.store.getState().sessions;
    expect(reply.session).toMatchObject({ agent: "codex", project_id: "p", role: "project", title: "Lead", bound_to: hub.id, pinned: true, status: "working" });
    expect(sessions.get(lead.id)).toMatchObject({ status: "archived", bound_to: hub.id });
    expect(sessions.get(child.id)?.bound_to).toBe(reply.session.id);
    expect(sessions.get(archivedChild.id)?.bound_to).toBe(reply.session.id);
  });

  it("gives a console session's receiving session the next ordinal and a colour no live session wears", async () => {
    const daemon = createFixtureDaemon(scenario);
    const reply = await daemon.request({ type: "hand_off_session", session: hub.id, agent: "grok" });
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
    const reply = await daemon.request({ type: "hand_off_session", session: hub.id, agent: "codex" });
    if (reply.type !== "session_opened") throw new Error("expected session_opened");
    expect(reply.session.colour).toBe("orange");
  });

  it("refuses what the daemon refuses, and a failure the scenario stages first", async () => {
    const working = sessionOf("working", "c", "p", "Working", "working");
    const archived = sessionOf("gone", "c", "p", "Gone", "archived");
    const locked = sessionOf("locked", "c", "p", "Locked", "idle", { locked: true });
    const staged = { code: "session_working", params: { session: lead.id }, message: "mid-turn" };
    const daemon = createFixtureDaemon({ ...scenario, state: snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [lead, working, archived, locked] }), sessionHandoffs: { [lead.id]: staged } });
    await expect(daemon.request({ type: "hand_off_session", session: lead.id, agent: "codex" })).rejects.toMatchObject({ code: "session_working" });
    await expect(daemon.request({ type: "hand_off_session", session: working.id, agent: "codex" })).rejects.toMatchObject({ code: "session_working" });
    await expect(daemon.request({ type: "hand_off_session", session: archived.id, agent: "codex" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "hand_off_session", session: locked.id, agent: "codex" })).rejects.toMatchObject({ code: "session_locked" });
    await expect(daemon.request({ type: "hand_off_session", session: "missing", agent: "codex" })).rejects.toMatchObject({ code: "unknown_session" });
    const plain = createFixtureDaemon(scenario);
    await expect(plain.request({ type: "hand_off_session", session: lead.id, agent: "claude" })).rejects.toMatchObject({ code: "handoff_to_same_agent", params: { agent: "Claude Code" } });
    await expect(plain.request({ type: "hand_off_session", session: lead.id, agent: "codex", account: "nobody" })).rejects.toMatchObject({ code: "unknown_account" });
  });
});

describe("the fixture daemon's added agents", () => {
  const lead = sessionOf("lead", "c", "p", "Lead", "idle");
  const state = snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [lead] });
  const withAdded = (added: Agent[] | null) => createFixtureDaemon({ id: "added", group: "Sidebar sessions" as const, category: "normal" as const, title: "", state: { ...state, settings: { ...state.settings!, added_agents: added } } });
  const open = (agent: Agent) => ({ type: "open_session", console_id: "c", project_id: "p", agent }) as const;

  it("refuses to open a session or hand one off to an agent that is not added", async () => {
    const daemon = withAdded(["claude"]);
    await expect(daemon.request(open("codex"))).rejects.toMatchObject({ code: "agent_not_added", params: { agent: "Codex" } });
    await expect(daemon.request({ type: "hand_off_session", session: lead.id, agent: "grok" })).rejects.toMatchObject({ code: "agent_not_added", params: { agent: "Grok Build" } });
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
  const scenario = { id: "directory-action", group: "Project pane: files" as const, category: "normal" as const, title: "", state };

  it("acknowledges a known project without opening a host application and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "open_project_directory", project: project.id })).resolves.toEqual({ type: "ack" });
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

describe("the fixture daemon's project removal", () => {
  const project = projectOf("p", "c", "Website");
  const error = { code: "project_directory_shared", params: { project: "Billing" }, message: "shared" };
  const scenario = { id: "removal", group: "Projects" as const, category: "exceptional" as const, title: "", state: snapshotState({ projects: [project] }), projectDeleteError: error };

  it("refuses only a removal that asks for the folder to be moved to the Trash", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "delete_project", project: project.id })).resolves.toEqual({ type: "ack" });
    await expect(daemon.request({ type: "delete_project", project: project.id, trash_directory: false })).resolves.toEqual({ type: "ack" });
    await expect(daemon.request({ type: "delete_project", project: project.id, trash_directory: true })).rejects.toMatchObject({ code: error.code, params: error.params });
    await expect(daemon.request({ type: "delete_project", project: "missing", trash_directory: true })).rejects.toMatchObject({ code: "unknown_project" });
  });
});

describe("the fixture daemon's session colour", () => {
  const owner = consoleOf("c", "Main");
  const hub = sessionOf("hub", owner.id, undefined, "Hub", "idle", { colour: "lime" });
  const scenario = { id: "colour", group: "Sidebar sessions" as const, category: "normal" as const, title: "", state: snapshotState({ consoles: [owner], sessions: [hub] }) };

  it("acknowledges a colour for a known session and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "set_session_colour", session: hub.id, colour: "blue" })).resolves.toEqual({ type: "ack" });
    await expect(daemon.request({ type: "set_session_colour", session: "missing", colour: "blue" })).rejects.toMatchObject({ code: "unknown_session" });
  });
});

describe("the fixture daemon's project file trees", () => {
  it("searches unopened descendants by filename and returns sorted project-relative paths", async () => {
    const daemon = createFixtureDaemon({
      id: "project-search", group: "Project pane: files", category: "normal", title: "",
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
      type: "project_file_matches", project: "web", query: " SERVER ", paths: ["Server.md", "src/server.ts"], complete: true,
    });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "missing" })).resolves.toMatchObject({ paths: [], complete: true });
    await expect(daemon.request({ type: "search_project_files", project: "web", query: "" })).resolves.toMatchObject({
      paths: ["Server.md", "server-docs/notes.md", "src/server.ts"], complete: true,
    });
  });

  it("leaves the system files, in any ASCII case, and everything beneath a left-out directory out of a search", async () => {
    const daemon = createFixtureDaemon({
      id: "project-search-left-out", group: "Project pane: files", category: "normal", title: "",
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
      id: "project-trees", group: "Project pane: files", category: "normal", title: "",
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
    const daemon = createFixtureDaemon({ id: "console-name", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ consoles: [owner], sessions }) });
    const renamed = { ...owner, name: "Release planning" };
    daemon.store.setState({ consoles: new Map([[owner.id, renamed]]) });
    await expect(daemon.request({ type: "preview_console_session_title", console: owner.id })).resolves.toMatchObject({ title: renamed.name });
    await expect(daemon.request({ type: "open_session", console_id: owner.id })).resolves.toMatchObject({ type: "session_opened", session: { title: renamed.name, ordinal: 4 } });
    for (const session of sessions) expect(daemon.store.getState().sessions.get(session.id)).toEqual(session);
  });

  it("honors a console session's explicit agent, account, name and launch overrides", async () => {
    const owner = consoleOf("c", "Release", { default_agent: "claude", codex_account_id: "work", codex_console_session_model: "inherited-model", codex_console_session_effort: "high" });
    const project = projectOf("p", owner.id, "Website");
    const existing = sessionOf("hub", owner.id, undefined, "Renamed release coordinator", "archived", { ordinal: 6 });
    const accounts = [{ id: "work", agent: "codex" as const, name: "Work", config_dir: "/Users/dev/.codex-work" }];
    const daemon = createFixtureDaemon({ id: "open", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ consoles: [owner], projects: [project], sessions: [existing], settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["claude", "codex"], default_clone_dir: "/Users/dev/Projects", accounts } }) });
    const preview = { type: "preview_console_session_title" as const, console: owner.id };
    for (let read = 0; read < 2; read++) {
      await expect(daemon.request(preview)).resolves.toEqual({ type: "console_session_title", console: owner.id, title: owner.name });
    }
    await daemon.request({ type: "delete_archived_sessions", console: owner.id });
    expect(daemon.store.getState().sessions.size).toBe(0);
    await expect(daemon.request(preview)).resolves.toMatchObject({ title: owner.name });
    const reply = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", title: "Coordinate release", model: "chosen-model", effort: "xhigh", bound_to: "ignored-console-binding" });
    expect(reply.type).toBe("session_opened");
    if (reply.type !== "session_opened") throw new Error("Missing opened session");
    expect(reply.session).toMatchObject({ agent: "codex", role: "console", origin: "user", status: "idle", title: "Coordinate release", account_id: "work", config_dir: accounts[0].config_dir, model: "chosen-model", effort: "xhigh", ordinal: 7 });
    expect(reply.session.bound_to).toBeUndefined();
    expect(daemon.store.getState().sessions.get(reply.session.id)).toEqual(reply.session);
    const defaults = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", account: null, model: null, effort: null });
    expect(defaults).toMatchObject({ type: "session_opened", session: { title: owner.name, ordinal: 8, model: null, effort: null } });
    if (defaults.type !== "session_opened") throw new Error("Missing default session");
    expect(defaults.session.account_id).toBeUndefined();
    const blank = await daemon.request({ type: "open_session", console_id: owner.id, title: "", task: "" });
    expect(blank).toMatchObject({ type: "session_opened", session: { origin: "user", status: "working", title: owner.name, ordinal: 9 } });
    const projectReply = await daemon.request({ type: "open_session", console_id: owner.id, project_id: project.id });
    expect(projectReply).toMatchObject({ type: "session_opened", session: { role: "project", origin: "user", status: "idle", title: project.name } });
    await expect(daemon.request(preview)).resolves.toMatchObject({ title: owner.name });
    const repeated = await daemon.request({ type: "open_session", console_id: owner.id });
    expect(repeated).toMatchObject({ type: "session_opened", session: { title: owner.name, ordinal: 10 } });
    await expect(daemon.request({ ...preview, console: "missing" })).rejects.toMatchObject({ code: "unknown_console" });
    expect(daemon.store.getState().consoles.get(owner.id)).toEqual(owner);
  });

  it("trims explicit names and falls back to owner names for whitespace-only names", async () => {
    const owner = consoleOf("c", "Release");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({ id: "trimmed-names", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ consoles: [owner], projects: [project] }) });
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
    const daemon = createFixtureDaemon({ id: "pins", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ sessions: [first, second] }) });
    await expect(daemon.request({ type: "set_session_pinned", session: first.id, pinned: false })).resolves.toEqual({ type: "ack" });
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
    const daemon = createFixtureDaemon({ id: "locks", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ sessions: [first, second] }) });
    await expect(daemon.request({ type: "set_session_locked", session: first.id, locked: true })).resolves.toEqual({ type: "ack" });
    expect([...daemon.store.getState().sessions]).toEqual([[first.id, { ...first, locked: true }], [second.id, second]]);
    await daemon.request({ type: "set_session_locked", session: first.id, locked: false });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual(first);
    await expect(daemon.request({ type: "set_session_locked", session: "missing", locked: true })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("refuses archiving a locked grandchild atomically and allows it after unlocking", async () => {
    const owner = sessionOf("owner", "c", undefined, "Hub", "interrupted");
    const lead = sessionOf("lead", "c", "p", "Lead", "interrupted", { bound_to: owner.id });
    const child = sessionOf("child", "c", "p", "Child", "interrupted", { bound_to: lead.id, locked: true });
    const daemon = createFixtureDaemon({ id: "locks", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ sessions: [owner, lead, child] }) });
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
    const daemon = createFixtureDaemon({ id: "locks", group: "Archive view", category: "normal", title: "", state: snapshotState({ consoles: [console_], projects: [project], sessions: [other, owner, lead, child] }) });
    const requests: RequestBody[] = [
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
      id: "t",
      group: "Archive view", category: "normal",
      title: "",
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
    const accounts = [{ id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }];
    const daemon = createFixtureDaemon({ id: "settings-contract", group: "Settings", category: "normal", title: "", state: snapshotState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts }, trustedDirectories: ["/Users/dev/code"] }) });
    await expect(daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "relative" })).rejects.toThrow();
    expect(daemon.store.getState().settings.auto_pull_repositories).toBe(false);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "~/work/../repos" });
    expect(daemon.store.getState().settings).toEqual({ auto_pull_repositories: true, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/repos", accounts });
    await daemon.request({ type: "update_settings", auto_pull_repositories: false, default_clone_dir: " " });
    expect(daemon.store.getState().settings).toEqual({ auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts });
    await daemon.request({ type: "update_settings", auto_delete_unresumable_archives: false });
    expect(daemon.store.getState().settings.auto_delete_unresumable_archives).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBeUndefined();
    expect(exportedDocument(daemon.store.getState()).settings).not.toHaveProperty("auto_approve_session_requests");
    expect(exportedDocument(daemon.store.getState()).settings).not.toHaveProperty("auto_approve_resume_requests");
    await daemon.request({ type: "update_settings", auto_approve_session_requests: true });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_session_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: false });
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBeUndefined();
    await daemon.request({ type: "update_settings", auto_approve_resume_requests: true });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(true);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(false);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: true });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(true);
    await daemon.request({ type: "update_settings", auto_approve_resume_requests: false });
    expect(daemon.store.getState().settings.auto_approve_resume_requests).toBe(false);
    expect(daemon.store.getState().settings.auto_approve_session_requests).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_approve_resume_requests).toBe(false);
    await daemon.request({ type: "update_settings", auto_approve_session_requests: false });
    expect(exportedDocument(daemon.store.getState()).settings).not.toHaveProperty("auto_select_account");
    await daemon.request({ type: "update_settings", auto_select_account: true });
    expect(daemon.store.getState().settings.auto_select_account).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_select_account).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_select_account).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings).not.toHaveProperty("sidebar_order_by_status");
    await daemon.request({ type: "update_settings", sidebar_order_by_status: false });
    expect(daemon.store.getState().settings.sidebar_order_by_status).toBe(false);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.sidebar_order_by_status).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings?.sidebar_order_by_status).toBe(false);
    expect(exportedDocument(daemon.store.getState()).settings).not.toHaveProperty("auto_start_quota_windows");
    await daemon.request({ type: "update_settings", auto_start_quota_windows: true });
    expect(daemon.store.getState().settings.auto_start_quota_windows).toBe(true);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true });
    expect(daemon.store.getState().settings.auto_start_quota_windows).toBe(true);
    expect(exportedDocument(daemon.store.getState()).settings?.auto_start_quota_windows).toBe(true);
    await daemon.request({ type: "remove_trusted_directory", path: "~/work/../code" });
    expect(daemon.store.getState().trustedDirectories).toEqual([]);
  });
});


describe("the fixture daemon's account directories", () => {
  const work = { id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" };
  const state = {
    ...snapshotState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex-work", agent: "codex" as const }] } }),
    agentAvailability: new Map([["claude" as const, { agent: "claude" as const, availability: "available" as const, default_account_dir: "/Users/dev/.claude" }]]),
  };
  const daemon = () => createFixtureDaemon({ id: "dirs", title: "", group: "Settings", category: "normal", state });

  it("refuses a directory another account of the agent or the default account uses, compared after normalising", async () => {
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "~/x/../.claude-work/" })).rejects.toMatchObject({ code: "config_dir_taken", params: { agent: "Claude Code", name: "Work" } });
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "~/.claude" })).rejects.toMatchObject({ code: "config_dir_taken", params: { agent: "Claude Code", name: "Default" } });
    await expect(daemon().request({ type: "update_account", account: "work", config_dir: "/Users/dev/.claude" })).rejects.toMatchObject({ code: "config_dir_taken" });
  });

  it("accepts another agent's path, an account's own directory and a new one", async () => {
    await expect(daemon().request({ type: "create_account", agent: "grok", name: "Studio", config_dir: "/Users/dev/.claude-work" })).resolves.toEqual({ type: "ack" });
    await expect(daemon().request({ type: "update_account", account: "work", name: "Renamed", config_dir: "/Users/dev/.claude-work" })).resolves.toEqual({ type: "ack" });
    await expect(daemon().request({ type: "create_account", agent: "claude", name: "Studio", config_dir: "/Users/dev/.claude-studio" })).resolves.toEqual({ type: "ack" });
  });
});


describe("the fixture daemon's session release", () => {
  const hub = sessionOf("hub", "c", undefined, "Hub", "idle", { colour: "pink", ordinal: 1 });
  const lead = sessionOf("lead", "c", "p", "Lead", "working", { bound_to: hub.id, pinned: true });
  const child = sessionOf("child", "c", "p", "Child", "idle", { bound_to: lead.id });
  const archived = sessionOf("old", "c", "p", "Old", "archived", { bound_to: hub.id });
  const loner = sessionOf("loner", "c", "p", "Loner", "idle");
  const state = snapshotState({ consoles: [consoleOf("c", "Main")], projects: [projectOf("p", "c", "Website")], sessions: [hub, lead, child, archived, loner] });
  const scenario = { id: "release", group: "Sidebar sessions" as const, category: "normal" as const, title: "", state };

  it("unbinds the session in any state, leaving what it owns bound to it", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "release_session", session: lead.id })).resolves.toEqual({ type: "ack" });
    await expect(daemon.request({ type: "release_session", session: archived.id })).resolves.toEqual({ type: "ack" });
    const { sessions } = daemon.store.getState();
    expect(sessions.get(lead.id)).toMatchObject({ status: "working", pinned: true, bound_to: undefined });
    expect(sessions.get(archived.id)).toMatchObject({ status: "archived", bound_to: undefined });
    expect(sessions.get(child.id)?.bound_to).toBe(lead.id);
  });

  it("refuses an unknown, a console and an unbound session", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "release_session", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
    await expect(daemon.request({ type: "release_session", session: hub.id })).rejects.toMatchObject({ code: "internal_error" });
    await expect(daemon.request({ type: "release_session", session: loner.id })).rejects.toMatchObject({ code: "internal_error" });
    expect(daemon.store.getState().sessions.get(hub.id)).toEqual(hub);
  });
});

describe("the fixture daemon's account switches", () => {
  const work = { id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" };
  const session = sessionOf("s", "c", "p", "Release", "working", { model: "opus", effort: "high", bound_to: "hub" });
  const state = snapshotState({ sessions: [session, { ...session, id: "archived", status: "archived" }], settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex", agent: "codex" }] } });
  const scenario = { id: "switch", title: "", group: "Sidebar sessions" as const, category: "normal" as const, state };

  it("records the new account while keeping session launch settings, and clears it for the default account", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).resolves.toEqual({ type: "ack" });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: work.id, config_dir: work.config_dir });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "session_already_on_account" });
    await daemon.request({ type: "switch_session_account", session: "s", account: null });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: undefined, config_dir: undefined });
  });

  it("broadcasts the session as switching while the switch holds it, and as it rests once the switch ends", async () => {
    const pending = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: "pending" } });
    void pending.request({ type: "switch_session_account", session: "s", account: "work" });
    expect(pending.store.getState().sessions.get("s")).toEqual({ ...session, switching_account: true });
    expect(pending.store.getState().daemonSwitching).toEqual(new Set(["s"]));
    const failing = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "internal_error", params: { detail: "x" }, message: "x" } } });
    await expect(failing.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "internal_error" });
    expect(failing.store.getState().sessions.get("s")).toEqual(session);
    expect(failing.store.getState().daemonSwitching).toEqual(new Set());
    const done = createFixtureDaemon(scenario);
    await done.request({ type: "switch_session_account", session: "s", account: "work" });
    expect(done.store.getState().sessions.get("s")?.switching_account).toBeUndefined();
    expect(done.store.getState().daemonSwitching).toEqual(new Set());
  });

  it("refuses invalid targets and configured failures without changing the session", async () => {
    const daemon = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "internal_error", params: { detail: "Could not restart the agent" }, message: "Could not restart the agent" } } });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "codex" })).rejects.toMatchObject({ code: "unknown_account" });
    await expect(daemon.request({ type: "switch_session_account", session: "archived", account: "work" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "internal_error", params: { detail: "Could not restart the agent" } });
    expect(daemon.store.getState().sessions.get("s")).toEqual(session);
  });
});

describe("the fixture daemon's project restore", () => {
  const project = projectOf("restore", "c", "Search API");
  const other = projectOf("other", "c", "Other project");
  const scenario = { id: "restore", group: "Projects" as const, category: "normal" as const, title: "", state: snapshotState({ projects: [project, other] }), git: RESTORE_GIT, files: RESTORE_FILES };

  it("uses the restored snapshot for only the requested project and never mutates shared fixtures", async () => {
    const daemon = createFixtureDaemon({ ...scenario, projectRestores: { [project.id]: RESTORED_PROJECT } });
    await expect(daemon.request({ type: "restore_project_changes", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
    await expect(daemon.request({ type: "restore_project_changes", project: project.id })).resolves.toEqual({ type: "ack" });
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
  const scenario = { id: "switch", group: "Projects" as const, category: "normal" as const, title: "", state: snapshotState({ projects: [project, other] }), git: RESTORE_GIT, files: RESTORE_FILES };

  it("applies the supplied branch snapshot and badge to only the requested project", async () => {
    const daemon = createFixtureDaemon({ ...scenario, projectBranchSwitches: { [project.id]: { "feature/search": { ...RESTORED_PROJECT, status } } } });
    await expect(daemon.request({ type: "switch_project_branch", project: "missing", branch: "feature/search" })).rejects.toMatchObject({ code: "unknown_project" });
    await expect(daemon.request({ type: "switch_project_branch", project: project.id, branch: "feature/search" })).resolves.toEqual({ type: "ack" });
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
  const owner = consoleOf("c", "Release", { console_session_prompt: "Be brief.", codex_console_session_model: "gpt-x" });
  const daemon = () => createFixtureDaemon({ id: "console-save", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ consoles: [owner] }) });

  it("keeps what a created console is given, the instructions trimmed and a blank one as none", async () => {
    const created = async (body: Extract<RequestBody, { type: "create_console" }>) => {
      const d = daemon();
      await d.request(body);
      return Array.from(d.store.getState().consoles.values()).find((c) => c.id !== owner.id);
    };
    expect(await created({ type: "create_console", name: " Ops ", default_agent: "codex", codex_account_id: "work", codex_console_session_effort: "high", console_session_prompt: "  Answer in Japanese.  " })).toMatchObject({
      name: "Ops", codex_account_id: "work", codex_console_session_effort: "high", console_session_prompt: "Answer in Japanese.",
    });
    expect((await created({ type: "create_console", name: "Plain", default_agent: "claude", console_session_prompt: "   " }))?.console_session_prompt).toBeNull();
  });

  it("leaves the instructions alone when absent, clears them with null or blank and exports what is saved", async () => {
    const d = daemon();
    await d.request({ type: "update_console", console: owner.id, name: "Renamed" });
    expect(d.store.getState().consoles.get(owner.id)).toMatchObject({ name: "Renamed", console_session_prompt: "Be brief.", codex_console_session_model: "gpt-x" });
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: " Use tables. " });
    expect(exportedDocument(d.store.getState()).consoles[0]?.console_session_prompt).toBe("Use tables.");
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: null });
    expect(d.store.getState().consoles.get(owner.id)?.console_session_prompt).toBeNull();
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: "x" });
    await d.request({ type: "update_console", console: owner.id, console_session_prompt: "  ", codex_console_session_model: null });
    expect(d.store.getState().consoles.get(owner.id)).toMatchObject({ console_session_prompt: null, codex_console_session_model: null });
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
    id: "branches",
    group: "Archive view" as const,
    category: "normal" as const,
    title: "",
    state: snapshotState({ projects: [project], sessions: [owner, other] }),
    sessionBranches: [worktree, branch, unbound],
    projectRemovalBranches: { [project.id]: [worktree, unbound] },
  };

  it("finds what is bound to exactly the sessions asked about that still exist", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "inspect_session_branches", sessions: [owner.id, "missing"] })).resolves.toEqual({ type: "session_branches_inspected", items: [worktree] });
    // A worktree bound to no session belongs to no inspection of sessions.
    await expect(daemon.request({ type: "inspect_session_branches", sessions: [owner.id, other.id] })).resolves.toMatchObject({ items: [worktree, branch] });
    await expect(createFixtureDaemon({ ...scenario, sessionBranches: undefined }).request({ type: "inspect_session_branches", sessions: [owner.id] })).resolves.toMatchObject({ items: [] });
  });

  it("answers a project removal per project and refuses an unknown one", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "inspect_project_removal", project: project.id })).resolves.toEqual({ type: "project_removal_inspected", items: [worktree, unbound] });
    await expect(createFixtureDaemon({ ...scenario, projectRemovalBranches: undefined }).request({ type: "inspect_project_removal", project: project.id })).resolves.toMatchObject({ items: [] });
    await expect(daemon.request({ type: "inspect_project_removal", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
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
  const scenario = { id: "records", group: "Sidebar sessions" as const, category: "normal" as const, title: "", state: snapshotState({ sessions: [unused, owner, child, live] }) };

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
  const scenario = { id: "notices", group: "Trust prompts and toasts" as const, category: "normal" as const, title: "" };

  it("preserves a UI notice's structured content alongside its plain text", () => {
    const daemon = createFixtureDaemon(scenario);
    const listener = vi.fn();
    const unsubscribe = daemon.onToast(listener);
    const content = createElement("span", null, "saved-file.json");
    daemon.toastNotice("Saved saved-file.json", content);
    expect(listener).toHaveBeenCalledWith({ kind: "notice", message: "Saved saved-file.json", content });
    unsubscribe();
  });

  it("keeps a failed launch's title metadata, nested path and complete plain message", async () => {
    vi.useFakeTimers();
    try {
      const params = { title: "Release notes", reason_code: "config_dir_unreachable", agent: "Claude Code", path: "/Users/dev/.claude-work", reason: "Permission denied" };
      const daemon = createFixtureDaemon({ ...scenario, toasts: [{ kind: "error", code: "launch_failed", params, message: "" }] });
      const toasts: ToastRequest[] = [];
      const unsubscribe = daemon.onToast((toast) => toasts.push(toast));
      await vi.runAllTimersAsync();
      expect(toasts).toHaveLength(1);
      expect(toasts[0]?.daemonTitle).toMatchObject({ code: "launch_failed.title", params });
      expect(toasts[0]?.daemonError).toMatchObject({ code: "config_dir_unreachable", params: expect.objectContaining({ path: params.path }) });
      expect(toasts[0]?.message).toContain(params.title);
      expect(toasts[0]?.message).toContain(params.path);
      unsubscribe();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("the fixture daemon's settings and rail removals", () => {
  it("clears deleted account references while preserving explicit defaults and existing session directories", async () => {
    const account = { id: "a", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" };
    const console_ = consoleOf("c", "Main", { claude_account_id: account.id });
    const project = projectOf("p", console_.id, "Website", { claude_account_id: { kind: "account", id: account.id }, codex_account_id: { kind: "default" } });
    const session = sessionOf("s", console_.id, project.id, "Work", "idle", { account_id: account.id, config_dir: account.config_dir });
    const daemon = createFixtureDaemon({ id: "account-removal", group: "Settings", category: "normal", title: "", state: { ...snapshotState({ consoles: [console_], projects: [project], sessions: [session] }), settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "", accounts: [account] } } });
    await daemon.request({ type: "delete_account", account: account.id });
    const current = daemon.store.getState();
    expect(current.settings.accounts).toEqual([]);
    expect(current.consoles.get(console_.id)?.claude_account_id).toBeNull();
    expect(current.projects.get(project.id)?.claude_account_id).toEqual({ kind: "inherit" });
    expect(current.projects.get(project.id)?.codex_account_id).toEqual({ kind: "default" });
    expect(current.sessions.get(session.id)).toEqual(session);
    await expect(daemon.request({ type: "delete_account", account: account.id })).rejects.toMatchObject({ code: "unknown_account" });
  });

  it("refuses console removal while sessions run, then removes its projects, archived sessions and pages", async () => {
    const console_ = consoleOf("c", "Main");
    const other = consoleOf("other", "Other");
    const project = projectOf("p", console_.id, "Website");
    const session = sessionOf("s", console_.id, project.id, "Work", "working");
    const hub = sessionOf("hub", console_.id, undefined, "Hub", "archived");
    const daemon = createFixtureDaemon({ id: "rail-removal", group: "Window layout", category: "normal", title: "", state: snapshotState({ consoles: [console_, other], projects: [project], sessions: [session, hub], pages: { [hub.id]: [pageOf("report", hub.id, "<p>Report</p>", 1)] }, gitStatuses: [gitStatusOf(project.id)] }) });
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

describe("the fixture daemon's account approval modes", () => {
  const accounts = [
    { id: "codex", agent: "codex" as const, name: "Build", config_dir: "/Users/dev/.codex-build" },
    { id: "claude", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" },
  ];
  const scenario = { id: "account-approval", group: "Settings" as const, category: "normal" as const, title: "", state: snapshotState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts } }) };

  it("saves and clears default and custom modes independently without changing other settings", async () => {
    const daemon = createFixtureDaemon(scenario);
    await daemon.request({ type: "set_account_approval_reviewer", account: null, reviewer: "auto_review" });
    await daemon.request({ type: "set_account_approval_reviewer", account: "codex", reviewer: "user" });
    expect(daemon.store.getState().settings).toEqual({ ...scenario.state.settings, codex_default_approval_reviewer: "auto_review", accounts: [{ ...accounts[0], approval_reviewer: "user" }, accounts[1]] });
    await daemon.request({ type: "set_account_approval_reviewer", account: null, reviewer: null });
    await daemon.request({ type: "set_account_approval_reviewer", account: "codex", reviewer: null });
    expect(daemon.store.getState().settings.codex_default_approval_reviewer).toBeNull();
    expect(daemon.store.getState().settings.accounts[0]?.approval_reviewer).toBeNull();
    for (const account of ["missing", "claude"]) {
      await expect(daemon.request({ type: "set_account_approval_reviewer", account, reviewer: "auto_review" })).rejects.toMatchObject({ code: "unknown_account", params: { account } });
    }
  });

  it("saves and clears Claude Code's default and custom modes, refusing an account of another agent", async () => {
    const daemon = createFixtureDaemon(scenario);
    await daemon.request({ type: "set_account_approval_mode", account: null, mode: "auto" });
    await daemon.request({ type: "set_account_approval_mode", account: "claude", mode: "bypass" });
    expect(daemon.store.getState().settings).toEqual({ ...scenario.state.settings, claude_default_approval_mode: "auto", accounts: [accounts[0], { ...accounts[1], approval_mode: "bypass" }] });
    await daemon.request({ type: "set_account_approval_mode", account: null, mode: null });
    await daemon.request({ type: "set_account_approval_mode", account: "claude", mode: null });
    expect(daemon.store.getState().settings.claude_default_approval_mode).toBeNull();
    expect(daemon.store.getState().settings.accounts[1]?.approval_mode).toBeNull();
    for (const account of ["missing", "codex"]) {
      await expect(daemon.request({ type: "set_account_approval_mode", account, mode: "manual" })).rejects.toMatchObject({ code: "unknown_account", params: { account } });
    }
  });

  it("retains every selection when saving fails", async () => {
    const error = { code: "store_io_failed", params: { detail: "Permission denied" }, message: "Permission denied" };
    const daemon = createFixtureDaemon({ ...scenario, accountApprovalError: error });
    const before = daemon.store.getState().settings;
    for (const account of [null, "codex"]) {
      await expect(daemon.request({ type: "set_account_approval_reviewer", account, reviewer: "auto_review" })).rejects.toMatchObject(error);
      expect(daemon.store.getState().settings).toEqual(before);
    }
    for (const account of [null, "claude"]) {
      await expect(daemon.request({ type: "set_account_approval_mode", account, mode: "bypass" })).rejects.toMatchObject(error);
      expect(daemon.store.getState().settings).toEqual(before);
    }
  });
});

describe("the fixture daemon's account sharing", () => {
  const accounts = [
    { id: "claude", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" },
    { id: "same", agent: "claude" as const, name: "Mirror", config_dir: "/Users/dev/.claude/" },
    { id: "codex", agent: "codex" as const, name: "Personal", config_dir: "/Users/dev/.codex-personal" },
  ];
  const scenario = {
    id: "account-sharing",
    group: "Settings" as const,
    category: "normal" as const,
    title: "",
    state: {
      ...snapshotState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/Users/dev/Projects", accounts } }),
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
        accountSharing: accountSharingOf([{ account: "claude", failure: { share: true, items: [{ item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied" }], detail: "`skills/` left unchanged" } }]),
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
    await expect(daemon.request({ type: "hand_off_session", session: "hub", agent: "codex" })).rejects.toMatchObject(refused);
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
      id: "path-links",
      group: "Terminal",
      category: "normal",
      title: "",
      state: snapshotState({ consoles: [owner], projects: [project] }),
      pathLinks: { [`${project.path}/src/a.ts`]: "file", "/Users/dev/notes": "directory" },
    });
    await expect(
      daemon.request({ type: "probe_path_links", project: project.id, paths: ["src/a.ts", "~/notes", "./src/../src/a.ts", "src/b.ts", `${project.path}/src/a.ts`] }),
    ).resolves.toEqual({
      type: "path_links",
      links: [
        { path: "src/a.ts", kind: "file" },
        { path: "~/notes", kind: "directory" },
        { path: "./src/../src/a.ts", kind: "file" },
        { path: "src/b.ts", kind: "none" },
        { path: `${project.path}/src/a.ts`, kind: "file" },
      ],
    });
    await expect(daemon.request({ type: "open_path_link", project: project.id, path: "src/a.ts" })).resolves.toEqual({ type: "ack" });
  });

  it("sends where a file the viewer shows (or a folder's README) is read from, and reads a worktree's files from the worktree", async () => {
    const owner = consoleOf("c", "Main");
    const project = projectOf("p", owner.id, "Website");
    const daemon = createFixtureDaemon({
      id: "path-link-views",
      group: "Terminal",
      category: "normal",
      title: "",
      state: snapshotState({ consoles: [owner], projects: [project] }),
      files: { "a.ts": { text: "const a = 1;\n" }, "bin/tool": { binary: { size: 2048 } } },
      worktreeFiles: { "wt-side": { "a.ts": { text: "const a = 2;\n" } } },
      pathLinks: {
        [`${project.path}/a.ts`]: { view: { path: "a.ts" } },
        "/Users/dev/side/a.ts": { view: { worktree: "wt-side", path: "a.ts" } },
        "/tmp/b.png": "file",
        [`${project.path}/bin/tool`]: { view: { path: "bin/tool" }, viewerOnly: true },
        [`${project.path}/docs/plan`]: { kind: "directory", view: { path: "docs/plan/README.md" } },
      },
    });
    await expect(daemon.request({ type: "probe_path_links", project: project.id, paths: ["a.ts", "/Users/dev/side/a.ts", "/tmp/b.png", "c.ts", "bin/tool", "docs/plan"] })).resolves.toEqual({
      type: "path_links",
      links: [
        { path: "a.ts", kind: "file", view: { path: "a.ts" } },
        { path: "/Users/dev/side/a.ts", kind: "file", view: { worktree: "wt-side", path: "a.ts" } },
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
  const settings = { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "/Users/dev/Projects", accounts: [] };
  const daemonWith = (added: ("claude" | "codex" | "grok")[] | null, availability: [("claude" | "codex" | "grok"), "available" | "unavailable" | "not_determined"][]) => {
    const daemon = createFixtureDaemon({ id: "added-agents", group: "Settings", category: "normal", title: "", state: snapshotState({ settings: { ...settings, added_agents: added } }) });
    daemon.store.setState({ agentAvailability: new Map(availability.map(([agent, state]) => [agent, { agent, availability: state }])) });
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
    const daemon = createFixtureDaemon({ id: "added-agents-refused", group: "Settings", category: "normal", title: "", agentListError: refusal, state: snapshotState({ settings: { ...settings, added_agents: ["claude", "codex"] } }) });
    daemon.store.setState({ agentAvailability: new Map([["grok" as const, { agent: "grok" as const, availability: "available" as const }]]) });
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

describe("the fixture daemon's account login and default account name", () => {
  const settings = { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "/Users/dev/Projects", added_agents: ["claude", "grok"] as ("claude" | "grok")[], accounts: [{ id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" }] };
  const daemonWith = (outcome?: "succeeded" | "failed" | "timed_out") => createFixtureDaemon({ id: "login", group: "Settings", category: "normal", title: "", state: snapshotState({ settings }), loginOutcome: outcome });
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
  const daemonWithProject = () => createFixtureDaemon({ id: "shells", group: "Terminal", category: "normal", title: "", state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")] }) });

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
    await expect(daemon.request({ type: "create_shell_terminal", project: "p", worktree: "wt-detached" })).rejects.toMatchObject({ code: "source_unavailable" });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("leaves a start unanswered when the scenario says so", async () => {
    const daemon = createFixtureDaemon({ id: "shell-starting", group: "Terminal panel", category: "normal", title: "", shellTerminalStart: "pending", state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")] }) });
    const outcome = await Promise.race([daemon.request({ type: "create_shell_terminal", project: "p" }).then(() => "answered"), new Promise((resolve) => setTimeout(() => resolve("pending"), 50))]);
    expect(outcome).toBe("pending");
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("drops the terminals of a deleted console's projects with them", async () => {
    const daemon = daemonWithProject();
    await daemon.request({ type: "create_shell_terminal", project: "p" });
    await daemon.request({ type: "delete_console", console: "c" });
    expect(daemon.store.getState().shellTerminals.size).toBe(0);
  });

  it("points a shell terminal's socket at its own behaviour, an exited one at its last screen, and a session's at the scenario's", () => {
    const shell = (id: string, state: "running" | "exited") => shellTerminalOf(id, projectOf("p", "c", "Site"), "zsh", 0, { state });
    const daemon = createFixtureDaemon({
      id: "shells-refused", group: "Terminal panel", category: "exceptional", title: "", terminal: "paths", shellTerminal: "refuse",
      state: snapshotState({ consoles: [consoleOf("c", "Work")], projects: [projectOf("p", "c", "Site")], shellTerminals: [shell("running", "running"), shell("done", "exited")] }),
    });
    expect(daemon.terminalUrl("running")).toBe(terminalFixtureUrl("refuse", "running"));
    expect(daemon.terminalUrl("done")).toBe(terminalFixtureUrl("exited", "done"));
    expect(daemon.terminalUrl("session")).toBe(terminalFixtureUrl("paths", "session"));
  });
});

describe("the fixture daemon's quota gate", () => {
  const accounts = [
    { id: "a-spent", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" },
    { id: "a-free", agent: "claude" as const, name: "Personal", config_dir: "/Users/dev/.claude-personal" },
    { id: "a-unread", agent: "claude" as const, name: "Client", config_dir: "/Users/dev/.claude-client" },
    { id: "a-model", agent: "claude" as const, name: "Model", config_dir: "/Users/dev/.claude-model" },
    { id: "a-stale", agent: "claude" as const, name: "Stale", config_dir: "/Users/dev/.claude-stale" },
  ];
  const readings = [
    quotaOf("claude", "a-spent", "pro", [quotaWindow("session", 100, 30), quotaWindow("weekly", 100, 90)]),
    quotaOf("claude", "a-free", "pro", [quotaWindow("session", 99, 30)]),
    quotaOf("claude", "a-model", "pro", [quotaWindow("weekly_model", 100, 30, "Fable")]),
    quotaOf("claude", "a-stale", "pro", [quotaWindow("session", 100, -5)]),
    quotaOf("claude", null, "max", [quotaWindow("session", 100, null)]),
  ];
  const daemon = (withAccounts = accounts, withReadings = readings) => createFixtureDaemon({
    id: "quota-gate", group: "Sidebar sessions", category: "exceptional", title: "",
    state: snapshotState({
      consoles: [consoleOf("c", "Main")],
      projects: [projectOf("p", "c", "Website")],
      sessions: [sessionOf("s", "c", "p", "Work", "idle")],
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["claude", "codex", "grok"], default_clone_dir: "/Users/dev/Projects", accounts: withAccounts },
      accountQuotas: withReadings,
    }),
  });

  it("refuses a session on an account whose window is used up, naming the window that resets last and when", async () => {
    const reading = readings[0].windows[1];
    await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account: "a-spent" })).rejects.toMatchObject({
      code: "account_quota_exhausted",
      params: { agent: "Claude Code", account: "Work", window: "Weekly", resets_at: String(reading.resets_at) },
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

  it("lets through an account nearly spent, one never read, one with only a model's window used up, and one whose window has reset", async () => {
    for (const account of ["a-free", "a-unread", "a-model", "a-stale"]) {
      await expect(daemon().request({ type: "open_session", console_id: "c", project_id: "p", account })).resolves.toMatchObject({ type: "session_opened" });
    }
  });

  it("refuses a switch to a used-up account and a handoff onto one, with the same code", async () => {
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-spent" })).rejects.toMatchObject({ code: "account_quota_exhausted" });
    await expect(daemon().request({ type: "switch_session_account", session: "s", account: "a-free" })).resolves.toEqual({ type: "ack" });
    const grok = [quotaOf("grok", null, "SuperGrok", [quotaWindow("credit_period", 100, 60)])];
    await expect(daemon([], grok).request({ type: "hand_off_session", session: "s", agent: "grok" })).rejects.toMatchObject({ code: "account_quota_exhausted", params: { window: "Weekly" } });
  });
});

describe("the fixture daemon's session history", () => {
  const archived = sessionOf("archived", "c", "p", "Archived", "archived");
  const base = { id: "history", group: "Terminal" as const, category: "normal" as const, title: "", state: snapshotState({ sessions: [archived] }) };
  const messages = [{ speaker: "user" as const, text: "Add an index", turn: 1 }];

  it("answers a session's messages, an empty history for one not listed, and unavailable for a record it cannot read", async () => {
    const request = { type: "read_session_history" as const, session: archived.id };
    await expect(createFixtureDaemon({ ...base, sessionHistory: { [archived.id]: messages } }).request(request)).resolves.toEqual({ type: "session_history", session: archived.id, messages });
    await expect(createFixtureDaemon(base).request(request)).resolves.toEqual({ type: "session_history", session: archived.id, messages: [] });
    await expect(createFixtureDaemon({ ...base, sessionHistory: { [archived.id]: "unavailable" } }).request(request)).resolves.toMatchObject({ messages: [], unavailable: true });
    await expect(createFixtureDaemon(base).request({ type: "read_session_history", session: "missing" })).rejects.toMatchObject({ code: "unknown_session" });
  });

  it("rejects a refused history read and answers a later retry with the current fixture result", async () => {
    const error = { code: "store_io_failed", params: { detail: "Resource temporarily unavailable" }, message: "Resource temporarily unavailable" };
    const sessionHistory: NonNullable<Parameters<typeof createFixtureDaemon>[0]["sessionHistory"]> = { [archived.id]: error };
    const daemon = createFixtureDaemon({ ...base, sessionHistory });
    const request = { type: "read_session_history" as const, session: archived.id };
    await expect(daemon.request(request)).rejects.toMatchObject(error);
    expect(daemon.store.getState().sessions.get(archived.id)?.status).toBe("archived");
    sessionHistory[archived.id] = messages;
    await expect(daemon.request(request)).resolves.toEqual({ type: "session_history", session: archived.id, messages });
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
