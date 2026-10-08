import { describe, expect, it } from "vitest";

import { gitStatusOf, snapshotState } from "./builders";

describe("snapshotState", () => {
  it("carries the git statuses map through, keyed by project id", () => {
    const status = gitStatusOf("p1", { behind: 2 });
    expect(snapshotState({ gitStatuses: [status] }).gitStatuses).toEqual(new Map([["p1", status]]));
  });

  it("defaults to an empty git statuses map and auto-sync off, and threads a given settings object through", () => {
    const empty = snapshotState({});
    expect(empty.gitStatuses).toEqual(new Map());
    expect(empty.settings).toEqual({ auto_sync_repositories: false, default_clone_dir: "/Users/dev/Projects", accounts: [] });

    expect(
      snapshotState({ settings: { auto_sync_repositories: true, default_clone_dir: "/p", accounts: [] } }).settings,
    ).toEqual({ auto_sync_repositories: true, default_clone_dir: "/p", accounts: [] });
  });
});
