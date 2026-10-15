// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../../daemon-client";
import type { ChangeSide, Event, SideRead, SideVersion } from "../../protocol";
import { hasHunks, joinedPatch, linesAfterLastHunk } from "../../viewer/patch";
import { FixtureUnfillableError } from "../banner";
import { createFixtureDaemon } from "../fixtureDaemon";
import type { FixtureGit } from "./changeFixtures";
import { objectIdCheck } from "./invariants";
import { SAMPLE_GIT } from "./projectGit";

describe("the sample repository's changes", () => {
  const fileSide = (side: ChangeSide) => side.state === "present" && side.kind === "file";
  const pathOf = (side: ChangeSide) => (side.state === "present" ? side.path : "");
  const lines = (side: SideRead) => (side.state === "present" ? side.lines : null);
  /** The daemon only the `git` of a scenario is read from; the rest of the scenario is unused. */
  const sampleDaemon = (git?: FixtureGit) => createFixtureDaemon({ id: "sample-git", group: "Project pane: Git", category: "normal", title: "", git });
  /** The version a request names a side at, which the daemon compares the side it finds against
   * before it reads any body: the one the diff reply reported, as the window names it back. */
  const versionOf = (side: ChangeSide): SideVersion => (side.state === "present" ? { state: "present", source: side.source } : { state: "absent" });
  const versionsOf = (change: { old: ChangeSide; new: ChangeSide }) => ({ old: versionOf(change.old), new: versionOf(change.new) });
  /** The two ends a comparison keyed `<left>..<right>` is read from, at the commits the branch list
   * names, and the fork it is made from, as the window names them back. */
  const endsOf = (comparison: string) => {
    const commitOf = (branch: string) => SAMPLE_GIT.branches?.find((b) => b.name === branch)?.commit ?? "";
    const [left, right] = comparison.split("..");
    return {
      left: { branch: left, commit: commitOf(left) },
      right: { branch: right, commit: commitOf(right) },
      base: SAMPLE_GIT.forks?.[comparison] ?? commitOf(left),
    };
  };
  /** The comparisons of the sample repository, each list once: two pairs may be answered with the
   * same changes, which reading twice would only repeat. */
  const comparisonsOnce = () => {
    const seen = new Set<unknown>();
    return Object.entries(SAMPLE_GIT.comparisons ?? {}).filter(([, changes]) => !seen.has(changes) && seen.add(changes));
  };
  /** Swallows the daemon errors a fixture answers on purpose, but never a refusal: that is a gap in
   * the gallery's own data, and the check below exists to name it. */
  const rethrowUnfillable = (error: unknown): undefined => {
    if (error instanceof FixtureUnfillableError) throw error;
    return undefined;
  };
  afterEach(() => vi.restoreAllMocks());

  // Git names an object by its content, so the same id is never two contents and one content is
  // never two ids. A change carries its sides' content, which the list entry does not, so the rule
  // is held over the changes and not over what the daemon replies with — and the ids it cannot be
  // held over are named, so that a side added later cannot drop out of the check unnoticed: the
  // link a type change ends at, whose target the patch alone holds, the rename whose content
  // cannot be read, and the change whose patch is past the daemon's limit.
  it("name an object by its content", () => {
    expect(objectIdCheck(SAMPLE_GIT)).toEqual({ conflicts: [], unchecked: ["1b2c3d4", "1f2e3d4", "8d2f1a0"] });
  });

  // A comparison is made from the fork of the two branches, so every committed side the sample
  // lists names the commit the fork is at (old) or the right branch's tip (new), and a side is
  // never read from the left branch's own tip.
  it("list a comparison's changes between its fork and its right branch's tip", () => {
    for (const [comparison, changes] of Object.entries(SAMPLE_GIT.comparisons ?? {})) {
      const { base, right } = endsOf(comparison);
      for (const { entry } of changes) {
        if (entry.group !== "committed") throw new Error(`${comparison} lists a ${entry.group} change`);
        if (entry.old.state === "present") expect([comparison, entry.old.source]).toEqual([comparison, expect.objectContaining({ kind: "commit", commit: base })]);
        if (entry.new.state === "present") expect([comparison, entry.new.source]).toEqual([comparison, expect.objectContaining({ kind: "commit", commit: right.commit })]);
      }
    }
  });

  it("answer a comparison with the commit the branches forked at", async () => {
    const compare = (git: FixtureGit, left: string, right: string) =>
      sampleDaemon(git).request({ type: "compare_project_branches", project: "p-1", left, right });
    const feature = SAMPLE_GIT.branches?.find((b) => b.name === "feature/ranking-experiments")?.commit;
    const main = SAMPLE_GIT.branches?.find((b) => b.name === "main")?.commit;
    const base = (reply: Event) => (reply.type === "project_comparison" ? [reply.base, reply.changes.length] : reply.type);

    // A branch made from main forks at main's tip; a branch with commits of its own forks before them.
    expect(base(await compare(SAMPLE_GIT, "main", "feature/ranking-experiments"))).toEqual([main, 8]);
    expect(base(await compare(SAMPLE_GIT, "chore/update-dependencies", "feature/ranking-experiments"))).toEqual([main, 8]);
    // The same commit, and a right branch that is an ancestor of the left one, change nothing.
    expect(base(await compare(SAMPLE_GIT, "main", "release/2.4"))).toEqual([main, 0]);
    expect(base(await compare(SAMPLE_GIT, "feature/ranking-experiments", "main"))).toEqual([feature, 0]);
    // No commit in common is refused rather than compared tip to tip.
    const orphaned: FixtureGit = { ...SAMPLE_GIT, branches: [...(SAMPLE_GIT.branches ?? []), { name: "gh-pages", commit: "3a2b1c0" }], forks: { "main..gh-pages": null } };
    await expect(compare(orphaned, "main", "gh-pages")).rejects.toMatchObject({ code: "no_merge_base", params: { left: "main", right: "gh-pages" } });
    await expect(compare({ ...orphaned, shallow: true }, "main", "gh-pages")).rejects.toMatchObject({ code: "no_merge_base_shallow", params: { left: "main", right: "gh-pages" } });
  });

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

    for (const [comparison, changes] of comparisonsOnce()) {
      const ends = endsOf(comparison);
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const reply = await daemon
          .request({ type: "read_project_comparison_change", project: "p-1", ...ends, change: entry })
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

  // What a request for a change's whole bodies is answered with, which follows the change's own
  // sides and nothing else: the daemon reads every present file side whatever the change's kind and
  // leaves an absent one absent, and refuses only what it refuses from the request alone or while
  // resolving the change. Every change a request can name is named, in the order its fixture lists
  // them rather than by section, so a reply that stops being given fails here rather than dropping
  // out unnoticed; a path in conflict is no two-sided change and no request reaches one. The
  // requests go together because an expandable change's fixture holds each one for
  // `EXPAND_DELAY_MS`. One is put for every change whether or not the viewer would ask — what is
  // pinned is the daemon's answer, and the viewer offers to expand only a change whose two sides
  // are both present files.
  it("answer a request for a change's bodies the way the daemon answers it", async () => {
    const daemon = sampleDaemon();
    /** A side of the reply, as its kind, the bytes it carries and the lines counted from them. */
    const side = (read: SideRead): string => {
      if (read.state !== "present") return read.state;
      if (!read.file) return `${read.kind} no body`;
      const kind = read.file.media_type ?? read.file.kind;
      return `${kind} ${read.file.size}/${read.lines}`;
    };
    const answer = async (at: string, request: Promise<Event>): Promise<string> => {
      const reply = await request.catch((error: unknown) => {
        rethrowUnfillable(error);
        if (!(error instanceof DaemonRequestError)) throw error;
        return error;
      });
      if (reply instanceof DaemonRequestError) {
        return `${at}: ${reply.code}${reply.params.limit ? ` (${reply.params.limit})` : ""}`;
      }
      if (reply.type !== "project_change_bodies" && reply.type !== "project_comparison_change_bodies") throw new Error(`${at}: ${reply.type}`);
      return `${at}: old ${side(reply.old)}, new ${side(reply.new)}`;
    };

    const asked: Promise<string>[] = [];
    for (const [worktree, changes] of Object.entries(SAMPLE_GIT.changes)) {
      if (!Array.isArray(changes)) continue;
      for (const { entry } of changes) {
        if (entry.group === "conflicted" || entry.group === "committed") continue;
        const change = { group: entry.group, old: entry.old, new: entry.new };
        const at = `${worktree} ${entry.group} ${pathOf(entry.new) || pathOf(entry.old)}`;
        const request = { type: "read_project_change_bodies" as const, project: "p-1", worktree, change, ...versionsOf(entry) };
        asked.push(answer(at, daemon.request(request)));
      }
    }
    for (const [comparison, changes] of comparisonsOnce()) {
      const ends = endsOf(comparison);
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const at = `${comparison} ${pathOf(entry.new) || pathOf(entry.old)}`;
        const request = { type: "read_project_comparison_change_bodies" as const, project: "p-1", ...ends, change: entry, ...versionsOf(entry) };
        asked.push(answer(at, daemon.request(request)));
      }
    }

    // Every request is in flight before the first is awaited, so a second refusal would go
    // unhandled under `Promise.all` and be reported against whichever test file ran next.
    const rows = (await Promise.allSettled(asked)).map((one) => {
      if (one.status === "rejected") throw one.reason;
      return one.value;
    });
    expect(rows).toEqual([
      "wt-main staged src/server.ts: old text 86/3, new text 134/4",
      "wt-main staged src/config.ts: old text 92/3, new text 100/3",
      // A side outside the project: no patch is made across the boundary, so there is nothing to expand.
      "wt-main staged src/index.ts: invalid_change",
      // The new side is a symbolic link, which has no body to read.
      "wt-main staged current: unsupported_file_type",
      "wt-main staged nest/leaf.ts: old text 5304/120, new text 5349/120",
      "wt-main staged nest2: old text 5349/120, new text 5349/120",
      "wt-main staged src/pipeline.ts: old text 5304/120, new text 5349/120",
      "wt-main staged src/tail.ts: old text 5304/120, new text 5349/120",
      "wt-main staged src/stages.ts: old text 3502/80, new text 3562/80",
      "wt-main staged src/huge.ts: old text 527788/11000, new text 527833/11000",
      "wt-main staged src/wide.ts: old text 56664/120, new text 56709/120",
      "wt-main staged src/limits.ts: limit_exceeded (file_bytes)",
      "wt-main staged src/moved.ts: source_changed",
      "wt-main staged src/flaky.ts: git_failed",
      "wt-main unstaged src/same.ts: old text 5349/120, new text 5349/120",
      "wt-main staged src/new-name.ts: git_failed",
      // The new side is the file on disk, which this reply counts though the diff reply does not:
      // it reads the side's whole body, and a body is counted from the very bytes it carries.
      "wt-main unstaged src/server.ts: old text 134/4, new text 161/6",
      // A deletion: the side that is there is read, and the one that is not stays absent.
      "wt-main unstaged docs/old-notes.md: old text 53/2, new absent",
      // A binary change: both sides are read and counted, the same bodies its diff reply carries.
      "wt-main unstaged docs/logo.png: old image/png 507/7, new image/png 516/8",
      // The patch is past the patch budget, and the bodies are resolved by making it first.
      "wt-main unstaged data/fixtures.bin: limit_exceeded (patch_bytes)",
      // An untracked file has no patch, so it has no collapsed lines to expand either.
      "wt-main untracked src/log.ts: invalid_change",
      "wt-main untracked tools/release/notes.md: invalid_change",
      "wt-main untracked src/config.local.ts: invalid_change",
      "wt-main untracked search-client: invalid_change",
      "wt-side unstaged README.md: old text 13/1, new text 35/1",
      "main..feature/ranking-experiments src/ranking.ts: old text 114/3, new text 180/4",
      "main..feature/ranking-experiments src/config.ts: old text 92/3, new text 100/3",
      "main..feature/ranking-experiments src/index.ts: invalid_change",
      "main..feature/ranking-experiments src/pipeline.ts: old text 5304/120, new text 5349/120",
      "main..feature/ranking-experiments src/cache.ts: old absent, new text 49/1",
      "main..feature/ranking-experiments docs/old-notes.md: old text 53/2, new absent",
      "main..feature/ranking-experiments docs/logo.png: old image/png 507/7, new image/png 516/8",
      "main..feature/ranking-experiments current: unsupported_file_type",
    ]);
  });

  // The refusal the check above leans on: the fixture daemon answers nothing where the real daemon
  // would carry a field it cannot fill, so a gap fails where it is used instead of shifting the UI
  // into a state the app cannot reach. It gets a fixture of its own, since no sample one has the
  // gap and nothing else would exercise it.
  const blob = (id: string): ChangeSide => ({ state: "present", path: "src/gap.ts", kind: "file", source: { kind: "index", worktree: "wt-gap", blob: id } });
  const entry = { group: "staged" as const, old: blob("1111111"), new: blob("2222222") };
  const change = { group: entry.group, old: entry.old, new: entry.new };
  /** A worktree holding one change that carries no whole text, with a patch whose hunk makes the
   * daemon count both of its sides. Two present file sides are bodies to read whatever else the
   * fixture says, so there is nothing to pass to make the request reach them. */
  const gap = (): FixtureGit => ({
    worktrees: [{ id: "wt-gap", root: "/Users/dev/code/search-api", main: true, head: "c0ffee0", branch: "main", scope_present: true }],
    changes: { "wt-gap": [{ entry, patch: ["diff --git a/src/gap.ts b/src/gap.ts", "@@ -1,1 +1,1 @@", "-one", "+two", ""].join("\n") }] },
  });
  const banner = () => document.querySelector("[data-gallery-failure]")?.textContent ?? "";

  // The check the daemon makes before it reads a body: a request naming a version other than the
  // one the daemon finds is refused rather than answered with a body of another revision. The
  // window names back what the diff reply just reported, so only a side that moved on under it
  // reaches this — which is the state `src/moved.ts` stages.
  it("refuse a request for bodies at a version a side is no longer at", async () => {
    const listed = SAMPLE_GIT.changes["wt-main"];
    if (!Array.isArray(listed)) throw new Error("the sample worktree lists its changes");
    const pipeline = listed.find(({ entry }) => "old" in entry && entry.old.state === "present" && entry.old.path === "src/pipeline.ts")?.entry;
    if (!pipeline || !("old" in pipeline) || pipeline.old.state !== "present") throw new Error("the sample worktree has a change of src/pipeline.ts");
    const change = { group: "staged" as const, old: { state: "present" as const, path: "src/pipeline.ts" }, new: { state: "present" as const, path: "src/pipeline.ts" } };
    const named = versionsOf(pipeline);
    // Everything the window would name back, with the old side's object alone moved on.
    const moved = { ...named, old: { state: "present" as const, source: { ...pipeline.old.source, blob: "0000001" } } };
    const at = { type: "read_project_change_bodies" as const, project: "p-1", worktree: "wt-main", change };

    await expect(sampleDaemon().request({ ...at, ...named })).resolves.toMatchObject({ type: "project_change_bodies" });
    await expect(sampleDaemon().request({ ...at, ...moved })).rejects.toMatchObject({ code: "source_changed", params: { path: "src/pipeline.ts" } });
  });

  it("refuse a reply whose side the daemon would count and the fixture cannot fill", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const read = sampleDaemon(gap()).request({ type: "read_project_change", project: "p-1", worktree: "wt-gap", change });

    // Not a daemon error, which a scenario answers with on purpose.
    await expect(read).rejects.toThrow(FixtureUnfillableError);
    await expect(read).rejects.not.toBeInstanceOf(DaemonRequestError);
    // Visible to whoever opened the scenario, not only to a test run, and it names the change it is
    // about, since a fixture is found by its worktree, its section and its path.
    expect(banner()).toContain("carries no whole text");
    expect(banner()).toContain("wt-gap staged (src/gap.ts)");
    expect(logged).toHaveBeenCalled();
  });

  // The same for a request for the bodies, whose rule is the stricter one: the daemon reads every
  // present file side there, wherever it is read from, so a side it would read and the fixture
  // carries nothing for is refused rather than answered with an empty body.
  it("refuse the bodies of a change the fixture carries none of", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const versions = { old: versionOf(entry.old), new: versionOf(entry.new) };
    const read = sampleDaemon(gap()).request({ type: "read_project_change_bodies", project: "p-1", worktree: "wt-gap", change, ...versions });

    await expect(read).rejects.toThrow(FixtureUnfillableError);
    await expect(read).rejects.not.toBeInstanceOf(DaemonRequestError);
    expect(banner()).toContain("wt-gap staged bodies (src/gap.ts): the daemon reads this side's whole body, and the fixture carries none");
    expect(logged).toHaveBeenCalled();
  });
});
