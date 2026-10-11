// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import type { ChangeSide, SideRead } from "../../protocol";
import { hasHunks, joinedPatch, linesAfterLastHunk } from "../../viewer/patch";
import { createFixtureDaemon } from "../fixtureDaemon";
import { SAMPLE_GIT } from "./projectGit";

describe("the sample repository's changes", () => {
  const fileSide = (side: ChangeSide) => side.state === "present" && side.kind === "file";
  const pathOf = (side: ChangeSide) => (side.state === "present" ? side.path : "");
  const lines = (side: SideRead) => (side.state === "present" ? side.lines : null);
  /** The daemon only the `git` of a scenario is read from; the rest of the scenario is unused. */
  const sampleDaemon = () => createFixtureDaemon({ id: "sample-git", group: "Project pane: Git", title: "" });

  // A side with no line count leaves an expandable diff ending in a bar promising unmodified lines
  // it may have none of, which a count takes away (`docs/product/project-pane-git-mode.md`, "The
  // run beyond the last hunk"). The daemon counts every side it reads, so a sample change whose
  // whole text the fixture leaves out shows a bar no real change of its shape has. Every change the
  // check covers is named, so a reply the fixture stops giving is a failure rather than a silent
  // skip.
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
        if (!fileSide(entry.old) || !fileSide(entry.new)) continue;
        const change = { group: entry.group, old: entry.old, new: entry.new };
        const reply = await daemon.request({ type: "read_project_change", project: "p-1", worktree, change }).catch(() => undefined);
        if (reply?.type !== "project_change") continue;
        check(`${worktree} ${entry.group}: ${pathOf(entry.new)}`, reply.patch?.text ?? null, reply.old, reply.new);
      }
    }

    const commitOf = (branch: string) => SAMPLE_GIT.branches?.find((b) => b.name === branch)?.commit ?? "";
    for (const [comparison, changes] of Object.entries(SAMPLE_GIT.comparisons ?? {})) {
      const [left, right] = comparison.split("..");
      const sides = { left: { branch: left, commit: commitOf(left) }, right: { branch: right, commit: commitOf(right) } };
      for (const { entry } of changes) {
        if (entry.group === "conflicted" || !fileSide(entry.old) || !fileSide(entry.new)) continue;
        const reply = await daemon.request({ type: "read_project_comparison_change", project: "p-1", ...sides, change: entry }).catch(() => undefined);
        if (reply?.type !== "project_comparison_change") continue;
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
});
