// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { createFixtureDaemon } from "./fixtureDaemon";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";

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
