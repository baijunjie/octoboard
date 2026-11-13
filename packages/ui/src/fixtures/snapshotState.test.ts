import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { describe, expect, it } from "vitest";

import { SETTINGS_VERSIONS, daemonUpdateOf, gitStatusOf, projectOf, shellTerminalOf, snapshotState } from "./builders";

describe("snapshotState", () => {
  it("carries the git statuses map through, keyed by project id", () => {
    const status = gitStatusOf("p1", { behind: 2 });
    expect(snapshotState({ gitStatuses: [status] }).gitStatuses).toEqual(new Map([["p1", status]]));
  });

  it("says nothing of the host unless given, and puts a given one and its home directory in the state", () => {
    expect(snapshotState({}).host).toBeUndefined();
    expect(snapshotState({}).homeDir).toBe("/Users/dev");
    const host = { os: "windows", path_style: "windows", file_links: true, trash: "recycle_bin", settings: "settings", keep_awake: false } as const;
    const windows = snapshotState({ host, homeDir: "C:\\Users\\dev" });
    expect(windows.host).toEqual(host);
    expect(windows.homeDir).toBe("C:\\Users\\dev");
  });

  it("defaults to an empty git statuses map and auto-pull off, and threads a given settings object through with every field's version", () => {
    const empty = snapshotState({});
    expect(empty.gitStatuses).toEqual(new Map());
    expect(empty.settings).toEqual({ ...DEFAULT_SETTINGS, added_agents: null, default_clone_dir: "/Users/dev/Projects", accounts: [], versions: SETTINGS_VERSIONS });

    expect(
      snapshotState({ settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [], default_clone_dir: "/p", accounts: [] } }).settings,
    ).toEqual({ ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [], default_clone_dir: "/p", accounts: [], versions: SETTINGS_VERSIONS });
    expect(snapshotState({ settings: { ...DEFAULT_SETTINGS, versions: { default_clone_dir: 7 } } }).settings?.versions).toEqual({ ...SETTINGS_VERSIONS, default_clone_dir: 7 });
  });

  it("carries the shell terminals map through, keyed by terminal id, and defaults to none", () => {
    const terminal = shellTerminalOf("t1", projectOf("p1", "c1", "Site"), "zsh", 0);
    expect(snapshotState({ shellTerminals: [terminal] }).shellTerminals).toEqual(new Map([["t1", terminal]]));
    expect(snapshotState({}).shellTerminals).toEqual(new Map());
  });

  it("has no daemon update unless given, so a window has nothing to update, and carries a given status through", () => {
    expect(snapshotState({}).daemonUpdate).toBeNull();
    const status = daemonUpdateOf({ phase: "installed", installed: { version: "0.2.0", holder_floor: null } });
    expect(snapshotState({ daemonUpdate: status }).daemonUpdate).toBe(status);
    expect(daemonUpdateOf()).toEqual({ phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null });
  });
});
