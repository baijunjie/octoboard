import { describe, expect, it } from "vitest";

import { gitBadgeAriaLabel, withGitBadge } from "./gitStatusLabel";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { GitStatus } from "./protocol";

// Real English messages rather than a stub, so a wording change that breaks a placeholder is
// caught here too.
const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const status = (extra: Partial<GitStatus> = {}): GitStatus => ({
  project: "p",
  repository: true,
  branch: "main",
  detached: false,
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  activity: "idle",
  error: null,
  ...extra,
});

describe("gitBadgeAriaLabel", () => {
  it("names the branch alone when even with the remote", () => {
    expect(gitBadgeAriaLabel("en", t, status())).toBe("Branch main");
  });

  it("adds ahead and behind only when they are not zero", () => {
    expect(gitBadgeAriaLabel("en", t, status({ ahead: 2 }))).toBe("Branch main, 2 commits ahead");
    expect(gitBadgeAriaLabel("en", t, status({ behind: 1 }))).toBe("Branch main, 1 commit behind");
    expect(gitBadgeAriaLabel("en", t, status({ ahead: 2, behind: 1 }))).toBe("Branch main, 2 commits ahead, 1 commit behind");
  });

  it("names a detached HEAD by its commit id instead of a branch name", () => {
    expect(gitBadgeAriaLabel("en", t, status({ detached: true, branch: "a1b2c3d" }))).toBe("Detached at a1b2c3d");
  });

  it("says there are no commits yet for a branchless repository with no error", () => {
    expect(gitBadgeAriaLabel("en", t, status({ branch: null }))).toBe("No commits yet");
  });

  it("lets the error phrase stand alone for a branchless repository whose read failed", () => {
    expect(gitBadgeAriaLabel("en", t, status({ branch: null, error: "fetch: Could not resolve host" }))).toBe(
      "Git error: fetch: Could not resolve host",
    );
  });

  it("names the in-flight phase instead of the idle wording", () => {
    expect(gitBadgeAriaLabel("en", t, status({ activity: "checking" }))).toBe("Checking branch main");
    expect(gitBadgeAriaLabel("en", t, status({ activity: "syncing" }))).toBe("Fast-forwarding branch main");
    expect(gitBadgeAriaLabel("en", t, status({ activity: "checking", branch: null }))).toBe("Checking the branch");
  });

  it("appends the last error verbatim, after ahead and behind", () => {
    expect(gitBadgeAriaLabel("en", t, status({ behind: 1, error: "fetch: Could not resolve host" }))).toBe(
      "Branch main, 1 commit behind, Git error: fetch: Could not resolve host",
    );
  });
});

describe("withGitBadge", () => {
  it("leaves the row's label alone without a status, or when it is not a repository", () => {
    expect(withGitBadge("en", t, "Website", undefined)).toBe("Website");
    expect(withGitBadge("en", t, "Website", status({ repository: false }))).toBe("Website");
  });

  it("folds the badge's facts into the row's own label", () => {
    expect(withGitBadge("en", t, "Website", status({ ahead: 1 }))).toBe("Website, Branch main, 1 commit ahead");
  });
});
