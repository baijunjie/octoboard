// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../../daemon-client";
import type { ChangeSide, SideRead } from "../../protocol";
import { hasHunks, joinedPatch, linesAfterLastHunk } from "../../viewer/patch";
import { FixtureUnfillableError, createFixtureDaemon } from "../fixtureDaemon";
import { SAMPLE_GIT, type FixtureGit } from "./projectGit";

describe("the sample repository's changes", () => {
  const fileSide = (side: ChangeSide) => side.state === "present" && side.kind === "file";
  const pathOf = (side: ChangeSide) => (side.state === "present" ? side.path : "");
  const lines = (side: SideRead) => (side.state === "present" ? side.lines : null);
  /** The daemon only the `git` of a scenario is read from; the rest of the scenario is unused. */
  const sampleDaemon = (git?: FixtureGit) => createFixtureDaemon({ id: "sample-git", group: "Project pane: Git", title: "", git });
  /** Swallows the daemon errors a fixture answers on purpose, but never a refusal: that is a gap in
   * the gallery's own data, and the check below exists to name it. */
  const rethrowUnfillable = (error: unknown): undefined => {
    if (error instanceof FixtureUnfillableError) throw error;
    return undefined;
  };
  afterEach(() => vi.restoreAllMocks());

  // A side with no line count leaves an expandable diff ending in a bar promising unmodified lines
  // it may have none of, which a count takes away (`docs/product/project-pane-git-mode.md`, "The
  // run beyond the last hunk"). The daemon counts every side it reads, so a sample change whose
  // whole text the fixture leaves out shows a bar no real change of its shape has. Every change the
  // check covers is named, so a reply the fixture stops giving is a failure rather than a silent
  // skip. Every change is read, the ones with no such bar included, so that a fixture the daemon
  // refuses outright fails here too and not only when its scenario is opened by hand.
  it("carry the line counts that settle the run past the last hunk", async () => {
    const daemon = sampleDaemon();
    const checked: string[] = [];
    const unsettled: string[] = [];
    /** Whether a change's run past the last hunk is settled, counting only the diffs that have one:
     * both sides files, and a patch the viewer draws as one expandable diff. */
    const check = (at: string, patch: string | null, old: SideRead, next: SideRead) => {
      const joined = patch === null ? "" : joinedPatch(patch);
      if (joined !== patch || !hasHunks(joined)) return;
      checked.push(at);
      if (linesAfterLastHunk(joined, { old: lines(old), new: lines(next) }) === undefined) unsettled.push(at);
    };

    for (const [worktree, changes] of Object.entries(SAMPLE_GIT.changes)) {
      if (!Array.isArray(changes)) continue;
      for (const { entry } of changes) {
        if (entry.group === "conflicted" || entry.group === "committed") continue;
        const change = { group: entry.group, old: entry.old, new: entry.new };
        const reply = await daemon.request({ type: "read_project_change", project: "p-1", worktree, change }).catch(rethrowUnfillable);
        if (reply?.type !== "project_change" || !fileSide(entry.old) || !fileSide(entry.new)) continue;
        check(`${worktree} ${entry.group}: ${pathOf(entry.new)}`, reply.patch?.text ?? null, reply.old, reply.new);
      }
    }

    const commitOf = (branch: string) => SAMPLE_GIT.branches?.find((b) => b.name === branch)?.commit ?? "";
    for (const [comparison, changes] of Object.entries(SAMPLE_GIT.comparisons ?? {})) {
      const [left, right] = comparison.split("..");
      const sides = { left: { branch: left, commit: commitOf(left) }, right: { branch: right, commit: commitOf(right) } };
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const reply = await daemon
          .request({ type: "read_project_comparison_change", project: "p-1", ...sides, change: entry })
          .catch(rethrowUnfillable);
        if (reply?.type !== "project_comparison_change" || !fileSide(entry.old) || !fileSide(entry.new)) continue;
        check(`${comparison}: ${pathOf(entry.new)}`, reply.patch?.text ?? null, reply.old, reply.new);
      }
    }

    expect(unsettled).toEqual([]);
    expect(checked).toEqual([
      "wt-main staged: src/server.ts",
      "wt-main staged: src/config.ts",
      "wt-main staged: nest/leaf.ts",
      "wt-main staged: src/pipeline.ts",
      "wt-main staged: src/tail.ts",
      "wt-main staged: src/stages.ts",
      "wt-main staged: src/huge.ts",
      "wt-main staged: src/wide.ts",
      "wt-main staged: src/limits.ts",
      "wt-main staged: src/moved.ts",
      "wt-main staged: src/flaky.ts",
      "wt-main unstaged: src/server.ts",
      "wt-side unstaged: README.md",
      "main..feature/ranking-experiments: src/ranking.ts",
      "main..feature/ranking-experiments: src/config.ts",
      "main..feature/ranking-experiments: src/pipeline.ts",
    ]);
  });

  // The refusal the check above leans on: the fixture daemon answers nothing where the real daemon
  // would carry a field it cannot fill, so a gap fails where it is used instead of shifting the UI
  // into a state the app cannot reach. It gets a fixture of its own, since no sample one has the
  // gap and nothing else would exercise it.
  it("refuse a reply whose side the daemon would count and the fixture cannot fill", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const blob = (id: string): ChangeSide => ({ state: "present", path: "src/gap.ts", kind: "file", source: { kind: "index", worktree: "wt-gap", blob: id } });
    const entry = { group: "staged" as const, old: blob("1111111"), new: blob("2222222") };
    // A patch with a hunk, two sides the daemon reads the content of, and no whole text to count.
    const patch = ["diff --git a/src/gap.ts b/src/gap.ts", "@@ -1,1 +1,1 @@", "-one", "+two", ""].join("\n");
    const git: FixtureGit = { worktrees: [{ id: "wt-gap", root: "/Users/dev/code/search-api", main: true, head: "c0ffee0", branch: "main", scope_present: true }], changes: { "wt-gap": [{ entry, patch }] } };
    const change = { group: entry.group, old: entry.old, new: entry.new };
    const read = sampleDaemon(git).request({ type: "read_project_change", project: "p-1", worktree: "wt-gap", change });

    // Not a daemon error, which a scenario answers with on purpose.
    await expect(read).rejects.toThrow(FixtureUnfillableError);
    await expect(read).rejects.not.toBeInstanceOf(DaemonRequestError);
    // Visible to whoever opened the scenario, not only to a test run, and it names the change it is
    // about, since a fixture is found by its worktree, its section and its path.
    const banner = document.querySelector("[data-gallery-failure]")?.textContent ?? "";
    expect(banner).toContain("carries no whole text");
    expect(banner).toContain("wt-gap staged (src/gap.ts)");
    expect(logged).toHaveBeenCalled();
  });
});
