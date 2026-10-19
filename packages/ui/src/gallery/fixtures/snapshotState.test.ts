import { describe, expect, it } from "vitest";

import { gitStatusOf, snapshotState } from "./builders";

describe("snapshotState", () => {
  it("carries the git statuses map through, keyed by project id", () => {
    const status = gitStatusOf("p1", { behind: 2 });
    expect(snapshotState({ gitStatuses: [status] }).gitStatuses).toEqual(new Map([["p1", status]]));
  });

  it("defaults to an empty git statuses map and auto-pull off, and threads a given settings object through", () => {
    const empty = snapshotState({});
    expect(empty.gitStatuses).toEqual(new Map());
    expect(empty.settings).toEqual({ auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/Users/dev/Projects", accounts: [] });

    expect(
      snapshotState({ settings: { auto_pull_repositories: true, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/p", accounts: [] } }).settings,
    ).toEqual({ auto_pull_repositories: true, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/p", accounts: [] });
  });
});
