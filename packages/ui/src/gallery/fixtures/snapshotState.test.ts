import { DEFAULT_SETTINGS } from "../../defaultSettings";
import { describe, expect, it } from "vitest";

import { gitStatusOf, projectOf, shellTerminalOf, snapshotState } from "./builders";

describe("snapshotState", () => {
  it("carries the git statuses map through, keyed by project id", () => {
    const status = gitStatusOf("p1", { behind: 2 });
    expect(snapshotState({ gitStatuses: [status] }).gitStatuses).toEqual(new Map([["p1", status]]));
  });

  it("defaults to an empty git statuses map and auto-pull off, and threads a given settings object through", () => {
    const empty = snapshotState({});
    expect(empty.gitStatuses).toEqual(new Map());
    expect(empty.settings).toEqual({ ...DEFAULT_SETTINGS, added_agents: null, default_clone_dir: "/Users/dev/Projects", accounts: [] });

    expect(
      snapshotState({ settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [], default_clone_dir: "/p", accounts: [] } }).settings,
    ).toEqual({ ...DEFAULT_SETTINGS, auto_pull_repositories: true, added_agents: [], default_clone_dir: "/p", accounts: [] });
  });

  it("carries the shell terminals map through, keyed by terminal id, and defaults to none", () => {
    const terminal = shellTerminalOf("t1", projectOf("p1", "c1", "Site"), "zsh", 0);
    expect(snapshotState({ shellTerminals: [terminal] }).shellTerminals).toEqual(new Map([["t1", terminal]]));
    expect(snapshotState({}).shellTerminals).toEqual(new Map());
  });
});
