// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { createFixtureDaemon } from "./fixtureDaemon";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";

describe("the fixture daemon's system directory action", () => {
  const project = projectOf("p", "c", "Website");
  const state = snapshotState({ projects: [project] });
  const scenario = { id: "directory-action", group: "Project pane: files" as const, title: "", state };

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

describe("the fixture daemon's project file trees", () => {
  it("uses each project's tree for listings and reads, retaining the shared fallback", async () => {
    const daemon = createFixtureDaemon({
      id: "project-trees", group: "Project pane: files", title: "",
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
  it("honors a console session's explicit agent, account, name and launch overrides", async () => {
    const owner = consoleOf("c", "Release", { default_agent: "claude", codex_account_id: "work", codex_console_session_model: "inherited-model", codex_console_session_effort: "high" });
    const project = projectOf("p", owner.id, "Website");
    const existing = sessionOf("hub", owner.id, undefined, "Renamed release coordinator", "archived", { ordinal: 6 });
    const accounts = [{ id: "work", agent: "codex" as const, name: "Work", config_dir: "/Users/dev/.codex-work" }];
    const daemon = createFixtureDaemon({ id: "open", group: "Dialogs", title: "", state: snapshotState({ consoles: [owner], projects: [project], sessions: [existing], settings: { auto_sync_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts } }) });
    const preview = { type: "preview_console_session_title" as const, console: owner.id };
    for (let read = 0; read < 2; read++) {
      await expect(daemon.request(preview)).resolves.toEqual({ type: "console_session_title", console: owner.id, title: "Hub 7" });
    }
    await daemon.request({ type: "delete_archived_sessions", console: owner.id });
    expect(daemon.store.getState().sessions.size).toBe(0);
    await expect(daemon.request(preview)).resolves.toMatchObject({ title: "Hub 7" });
    const reply = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", title: "Coordinate release", model: "chosen-model", effort: "xhigh", bound_to: "ignored-console-binding" });
    expect(reply.type).toBe("session_opened");
    if (reply.type !== "session_opened") throw new Error("Missing opened session");
    expect(reply.session).toMatchObject({ agent: "codex", role: "console", origin: "user", status: "idle", title: "Coordinate release", account_id: "work", config_dir: accounts[0].config_dir, model: "chosen-model", effort: "xhigh", ordinal: 7 });
    expect(reply.session.bound_to).toBeUndefined();
    expect(daemon.store.getState().sessions.get(reply.session.id)).toEqual(reply.session);
    const defaults = await daemon.request({ type: "open_session", console_id: owner.id, agent: "codex", account: null, model: null, effort: null });
    expect(defaults).toMatchObject({ type: "session_opened", session: { title: "Hub 8", ordinal: 8, model: null, effort: null } });
    if (defaults.type !== "session_opened") throw new Error("Missing default session");
    expect(defaults.session.account_id).toBeUndefined();
    const blank = await daemon.request({ type: "open_session", console_id: owner.id, title: "", task: "" });
    expect(blank).toMatchObject({ type: "session_opened", session: { origin: "user", status: "working", title: "" } });
    const projectReply = await daemon.request({ type: "open_session", console_id: owner.id, project_id: project.id });
    expect(projectReply).toMatchObject({ type: "session_opened", session: { role: "project", origin: "user", status: "idle", title: project.name } });
    await expect(daemon.request(preview)).resolves.toMatchObject({ title: "Hub 10" });
    await expect(daemon.request({ ...preview, console: "missing" })).rejects.toMatchObject({ code: "unknown_console" });
    expect(daemon.store.getState().consoles.get(owner.id)).toEqual(owner);
  });
});

describe("the fixture daemon's session pins", () => {
  it("updates only the requested session and refuses an unknown session", async () => {
    const first = sessionOf("first", "c-1", "p-1", "Pinned work", "working", { pinned: true, bound_to: "hub" });
    const second = sessionOf("second", "c-1", "p-1", "Other work", "idle");
    const daemon = createFixtureDaemon({ id: "pins", group: "Sidebar sessions", title: "", state: snapshotState({ sessions: [first, second] }) });
    await expect(daemon.request({ type: "set_session_pinned", session: first.id, pinned: false })).resolves.toEqual({ type: "ack" });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual({ ...first, pinned: false });
    expect(daemon.store.getState().sessions.get(second.id)).toEqual(second);
    await daemon.request({ type: "set_session_pinned", session: first.id, pinned: true });
    expect(daemon.store.getState().sessions.get(first.id)).toEqual(first);
    await expect(daemon.request({ type: "set_session_pinned", session: "missing", pinned: true })).rejects.toMatchObject({ code: "unknown_session", params: { session: "missing" } });
    expect(daemon.store.getState().sessions.size).toBe(2);
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
      group: "Archive view",
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
    const daemon = createFixtureDaemon({ id: "settings-contract", group: "Settings", title: "", state: snapshotState({ settings: { auto_sync_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts }, trustedDirectories: ["/Users/dev/code"] }) });
    await expect(daemon.request({ type: "update_settings", auto_sync_repositories: true, default_clone_dir: "relative" })).rejects.toThrow();
    expect(daemon.store.getState().settings.auto_sync_repositories).toBe(false);
    await daemon.request({ type: "update_settings", auto_sync_repositories: true, default_clone_dir: "~/work/../repos" });
    expect(daemon.store.getState().settings).toEqual({ auto_sync_repositories: true, default_clone_dir: "/Users/dev/repos", accounts });
    await daemon.request({ type: "update_settings", auto_sync_repositories: false, default_clone_dir: " " });
    expect(daemon.store.getState().settings).toEqual({ auto_sync_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts });
    await daemon.request({ type: "remove_trusted_directory", path: "~/work/../code" });
    expect(daemon.store.getState().trustedDirectories).toEqual([]);
  });
});


describe("the fixture daemon's account switches", () => {
  const work = { id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" };
  const session = sessionOf("s", "c", "p", "Release", "working", { model: "opus", effort: "high", bound_to: "hub" });
  const state = snapshotState({ sessions: [session, { ...session, id: "archived", status: "archived" }], settings: { auto_sync_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex", agent: "codex" }] } });
  const scenario = { id: "switch", title: "", group: "Dialogs" as const, state };

  it("records the new account while keeping session launch settings, and clears it for the default account", async () => {
    const daemon = createFixtureDaemon(scenario);
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).resolves.toEqual({ type: "ack" });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: work.id, config_dir: work.config_dir });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "session_already_on_account" });
    await daemon.request({ type: "switch_session_account", session: "s", account: null });
    expect(daemon.store.getState().sessions.get("s")).toEqual({ ...session, account_id: undefined, config_dir: undefined });
  });

  it("refuses invalid targets and configured failures without changing the session", async () => {
    const daemon = createFixtureDaemon({ ...scenario, sessionAccountSwitches: { s: { code: "internal_error", params: { detail: "Could not restart the agent" }, message: "Could not restart the agent" } } });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "codex" })).rejects.toMatchObject({ code: "unknown_account" });
    await expect(daemon.request({ type: "switch_session_account", session: "archived", account: "work" })).rejects.toMatchObject({ code: "session_archived" });
    await expect(daemon.request({ type: "switch_session_account", session: "s", account: "work" })).rejects.toMatchObject({ code: "internal_error", params: { detail: "Could not restart the agent" } });
    expect(daemon.store.getState().sessions.get("s")).toEqual(session);
  });
});
