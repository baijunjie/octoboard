// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import type { RequestBody } from "../protocol";
import { exportedDocument } from "./fixtureConfiguration";
import { createFixtureDaemon } from "./fixtureDaemon";
import { RESTORE_FILES, RESTORE_GIT, RESTORED_PROJECT } from "./fixtures/restore";
import { consoleOf, gitStatusOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";

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
    const daemon = createFixtureDaemon({ id: "open", group: "Sidebar sessions", category: "normal", title: "", state: snapshotState({ consoles: [owner], projects: [project], sessions: [existing], settings: { auto_pull_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts } }) });
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
    const daemon = createFixtureDaemon({ id: "settings-contract", group: "Settings", category: "normal", title: "", state: snapshotState({ settings: { auto_pull_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts }, trustedDirectories: ["/Users/dev/code"] }) });
    await expect(daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "relative" })).rejects.toThrow();
    expect(daemon.store.getState().settings.auto_pull_repositories).toBe(false);
    await daemon.request({ type: "update_settings", auto_pull_repositories: true, default_clone_dir: "~/work/../repos" });
    expect(daemon.store.getState().settings).toEqual({ auto_pull_repositories: true, default_clone_dir: "/Users/dev/repos", accounts });
    await daemon.request({ type: "update_settings", auto_pull_repositories: false, default_clone_dir: " " });
    expect(daemon.store.getState().settings).toEqual({ auto_pull_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts });
    await daemon.request({ type: "remove_trusted_directory", path: "~/work/../code" });
    expect(daemon.store.getState().trustedDirectories).toEqual([]);
  });
});


describe("the fixture daemon's account switches", () => {
  const work = { id: "work", agent: "claude" as const, name: "Work", config_dir: "/Users/dev/.claude-work" };
  const session = sessionOf("s", "c", "p", "Release", "working", { model: "opus", effort: "high", bound_to: "hub" });
  const state = snapshotState({ sessions: [session, { ...session, id: "archived", status: "archived" }], settings: { auto_pull_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts: [work, { ...work, id: "codex", agent: "codex" }] } });
  const scenario = { id: "switch", title: "", group: "Sidebar sessions" as const, category: "normal" as const, state };

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
