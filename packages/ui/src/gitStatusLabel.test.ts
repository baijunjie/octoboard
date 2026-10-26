import { describe, expect, it } from "vitest";

import { gitBadgeAriaLabel, gitFailureLabel, withGitBadge } from "./gitStatusLabel";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
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
    expect(gitBadgeAriaLabel("en", t, status())).toBe(`Branch ${isolate("main")}`);
  });

  it("adds ahead and behind only when they are not zero", () => {
    expect(gitBadgeAriaLabel("en", t, status({ ahead: 2 }))).toBe(`Branch ${isolate("main")}, 2 commits ahead`);
    expect(gitBadgeAriaLabel("en", t, status({ behind: 1 }))).toBe(`Branch ${isolate("main")}, 1 commit behind`);
    expect(gitBadgeAriaLabel("en", t, status({ ahead: 2, behind: 1 }))).toBe(`Branch ${isolate("main")}, 2 commits ahead, 1 commit behind`);
  });

  it("names a detached HEAD by its commit id instead of a branch name", () => {
    expect(gitBadgeAriaLabel("en", t, status({ detached: true, branch: "a1b2c3d" }))).toBe(`Detached at ${isolate("a1b2c3d")}`);
  });

  it("says there are no commits yet for a branchless repository with no error", () => {
    expect(gitBadgeAriaLabel("en", t, status({ branch: null }))).toBe("No commits yet");
  });

  it("says nothing for a branchless repository whose read failed, the failure mark saying it", () => {
    expect(gitBadgeAriaLabel("en", t, status({ branch: null, error: "fetch: Could not resolve host" }))).toBe("");
  });

  it("names the in-flight phase instead of the idle wording", () => {
    expect(gitBadgeAriaLabel("en", t, status({ activity: "checking" }))).toBe(`Checking branch ${isolate("main")}`);
    expect(gitBadgeAriaLabel("en", t, status({ activity: "pushing" }))).toBe(`Pushing branch ${isolate("main")}`);
    expect(gitBadgeAriaLabel("en", t, status({ activity: "pushing", branch: null }))).toBe("Pushing the branch");
    expect(gitBadgeAriaLabel("en", t, status({ activity: "pulling" }))).toBe(`Pulling branch ${isolate("main")}`);
    expect(gitBadgeAriaLabel("en", t, status({ activity: "checking", branch: null }))).toBe("Checking the branch");
  });

  it("leaves a failed check and Git's own text out of the sentence, which is also the tooltip's", () => {
    expect(gitBadgeAriaLabel("en", t, status({ behind: 1, error: "fetch: Could not resolve host" }))).toBe(`Branch ${isolate("main")}, 1 commit behind`);
  });
});

describe("withGitBadge", () => {
  it("leaves the row's label alone without a status, or when it is not a repository", () => {
    expect(withGitBadge("en", t, "Website", undefined)).toBe("Website");
    expect(withGitBadge("en", t, "Website", status({ repository: false }))).toBe("Website");
  });

  it("folds the badge's facts into the row's own label", () => {
    expect(withGitBadge("en", t, "Website", status({ ahead: 1 }))).toBe(`Website, Branch ${isolate("main")}, 1 commit ahead`);
  });

  it("names a failed check in the application's words after the badge's facts, never with Git's own text", () => {
    const raw = "fetch: Could not resolve host";
    expect(withGitBadge("en", t, "Website", status({ behind: 1, error: raw }))).toBe(`Website, Branch ${isolate("main")}, 1 commit behind, Git check failed`);
    expect(withGitBadge("en", t, "Website", status({ branch: null, error: raw }))).toBe("Website, Git check failed");
    expect(withGitBadge("en", t, "Website", status({ activity: "checking", error: raw }))).toBe(`Website, Checking branch ${isolate("main")}`);
    expect(gitFailureLabel(t)).toBe("Git check failed");
  });
});
