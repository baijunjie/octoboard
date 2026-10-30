// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../../daemon-client";
import type { ChangeEntry, ChangeSide, Event, RequestBody, SideRead, SideVersion } from "../../protocol";
import { hasHunks, joinedPatch, linesAfterLastHunk } from "../../viewer/patch";
import { FixtureUnfillableError } from "../banner";
import { createFixtureDaemon } from "../fixtureDaemon";
import { EMPTY_TREE, listedCommits, type FixtureChange, type FixtureGit } from "./changeFixtures";
import { objectIdCheck } from "./invariants";
import { SCENARIOS } from ".";
import { FORKED_GIT, LONG_HISTORY_GIT, NEVER_PUSHED_GIT, projectGitScenarios, SAMPLE_GIT, UNPUSHED_GIT } from "./projectGit";

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
  /** The repositories whose comparisons and commit lists the checks below hold: the sample, and the
   * one with a branch ahead of its upstream, and the one with a branch never pushed. */
  const REPOSITORIES = [SAMPLE_GIT, UNPUSHED_GIT, NEVER_PUSHED_GIT];
  /** The two ends a comparison keyed `<left>..<right>` is read from, at the commits the branch list
   * names (a remote-tracking branch at the one its branch's upstream names, else the default branch the
   * list gives), and the fork it is made from, as the window names them back. */
  const endsOf = (git: FixtureGit, comparison: string) => {
    const commitOf = (branch: string) =>
      branch === ""
        ? EMPTY_TREE
        : git.branches?.find((b) => b.name === branch)?.commit ??
          git.branches?.find((b) => b.upstream?.name === branch)?.upstream?.commit ??
          (git.defaultBranch?.name === branch ? git.defaultBranch.commit : "");
    const [left, right] = comparison.split("..");
    return {
      left: { branch: left, commit: commitOf(left) },
      right: { branch: right, commit: commitOf(right) },
      base: git.forks?.[comparison] ?? commitOf(left),
    };
  };
  /** The comparisons of the sample repository, each list once: two pairs may be answered with the
   * same changes, which reading twice would only repeat. */
  const comparisonsOnce = () => {
    const seen = new Set<unknown>();
    return Object.entries(SAMPLE_GIT.comparisons ?? {}).filter(([, changes]) => !seen.has(changes) && seen.add(changes));
  };
  /** Every commit of the sample repository's commit lists, with its changes: the commits that have
   * none are listed all the same, and have nothing to read. */
  const commitsOf = (git: FixtureGit) => Object.entries(git.commits ?? {}).flatMap(([pair, list]) => listedCommits(list).map((commit) => ({ pair, ...commit })));
  const short = (commit: string) => commit.slice(0, 7);
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
    expect(objectIdCheck(UNPUSHED_GIT).conflicts).toEqual([]);
    expect(objectIdCheck(NEVER_PUSHED_GIT).conflicts).toEqual([]);
  });

  // A comparison is made from the fork of the two branches, so every committed side the sample
  // lists names the commit the fork is at (old) or the right branch's tip (new), and a side is
  // never read from the left branch's own tip.
  it("list a comparison's changes between its fork and its right branch's tip", () => {
    for (const git of REPOSITORIES) {
      for (const [comparison, changes] of Object.entries(git.comparisons ?? {})) {
        const { base, right } = endsOf(git, comparison);
        for (const { entry } of changes) {
          if (entry.group !== "committed") throw new Error(`${comparison} lists a ${entry.group} change`);
          // From the empty tree nothing is there to be the old side.
          if (entry.old.state === "present") expect([comparison, entry.old.source]).toEqual([comparison, expect.objectContaining({ kind: "commit", commit: base })]);
          if (base === EMPTY_TREE) expect([comparison, entry.old.state]).not.toEqual([comparison, "present"]);
          if (entry.new.state === "present") expect([comparison, entry.new.source]).toEqual([comparison, expect.objectContaining({ kind: "commit", commit: right.commit })]);
        }
      }
    }
  });

  // A commit's changes are against its first parent, so every committed side names the commit's
  // parent (old) or the commit itself (new), and a commit with no parent has no old side.
  it("list a commit's changes between its parent and the commit", () => {
    for (const { pair, commit, parent, changes } of REPOSITORIES.flatMap(commitsOf)) {
      for (const { entry } of changes) {
        if (entry.group !== "committed") throw new Error(`${pair} ${short(commit)} lists a ${entry.group} change`);
        const at = `${pair} ${short(commit)}`;
        if (entry.old.state === "present") expect([at, entry.old.source]).toEqual([at, expect.objectContaining({ kind: "commit", commit: parent })]);
        if (parent === null) expect([at, entry.old.state]).not.toEqual([at, "present"]);
        if (entry.new.state === "present") expect([at, entry.new.source]).toEqual([at, expect.objectContaining({ kind: "commit", commit })]);
      }
    }
  });

  // The comparison's changes are the commits' put together, so a path the comparison lists was
  // changed by some commit. Not the other way round, which Git does not hold either: a commit may
  // change a path another undoes, or the same path again.
  it("list commits that between them change every path the comparison lists", () => {
    const pathsOf = (list: { entry: ChangeEntry }[]) => list.flatMap(({ entry }) => (entry.group === "conflicted" ? [] : [pathOf(entry.new) || pathOf(entry.old)]));
    for (const git of REPOSITORIES) {
      for (const [pair, changes] of Object.entries(git.comparisons ?? {})) {
        const changed = new Set(pathsOf(listedCommits(git.commits?.[pair] ?? { commits: [] }).flatMap(({ changes: made }) => made)));
        expect([pair, pathsOf(changes).filter((path) => !changed.has(path))]).toEqual([pair, []]);
      }
    }
  });

  // The commits are what `git log <fork>..<tip>` gives: each is reachable from the tip, through
  // first parents and the other parents of merges, and the first-parent line from the tip ends at
  // the fork, or, where the list was cut, at a commit the list leaves out. A commit with no parent
  // is only ever met as a merge's other parent.
  it("list commits that form a history the tip reaches", () => {
    for (const git of REPOSITORIES) {
      for (const [pair, list] of Object.entries(git.commits ?? {})) {
        const commits = listedCommits(list);
        if (commits.length === 0) continue;
        const byId = new Map(commits.map((listed) => [listed.commit, listed]));
        const tip = git.branches?.find((b) => b.name === pair.split("..")[1])?.commit ?? "";
        const reached = new Set<string>();
        const visit = (id: string | null | undefined) => {
          const listed = id ? byId.get(id) : undefined;
          if (!listed || reached.has(listed.commit)) return;
          reached.add(listed.commit);
          visit(listed.parent);
          visit(listed.merged);
        };
        visit(tip);
        expect([pair, commits.filter((listed) => !reached.has(listed.commit)).map(({ subject }) => subject)]).toEqual([pair, []]);

        let end = tip;
        for (let listed = byId.get(end); listed?.parent; listed = byId.get(end)) end = listed.parent;
        const cut = "cut" in list && list.cut === true;
        // A history from the empty tree ends at the root commit, which has no parent.
        const whole = endsOf(git, pair).base === EMPTY_TREE;
        expect([pair, whole ? byId.get(end)?.parent === null : cut ? !byId.has(end) && end !== endsOf(git, pair).base : end === endsOf(git, pair).base]).toEqual([pair, true]);

        const taken = new Set(commits.flatMap(({ merged }) => (merged ? [merged] : [])));
        expect([pair, commits.filter(({ parent, commit }) => parent === null && !taken.has(commit) && !(whole && commit === end)).map(({ subject }) => subject)]).toEqual([pair, []]);
      }
    }
  });

  // The daemon looks an upstream up only for the branch the main working copy has checked out.
  it("give an upstream only to the main working copy's branch", () => {
    const repositories = [...REPOSITORIES, ...projectGitScenarios.flatMap(({ git }) => (git ? [git] : []))];
    for (const git of repositories) {
      for (const branch of git.branches ?? []) {
        if (branch.upstream) expect([branch.name, git.worktrees.some((worktree) => worktree.main && worktree.branch === branch.name)]).toEqual([branch.name, true]);
      }
    }
  });

  it("give a default branch only to a repository whose main working copy's branch has no upstream", () => {
    const repositories = [...REPOSITORIES, FORKED_GIT, ...projectGitScenarios.flatMap(({ git }) => (git ? [git] : []))];
    for (const git of repositories) {
      if (!git.defaultBranch) continue;
      const main = git.worktrees.find((worktree) => worktree.main)?.branch;
      expect(git.branches?.find((branch) => branch.name === main)?.upstream).toBeUndefined();
    }
  });

  it("answer a comparison of a branch with the default branch the list gives", async () => {
    const daemon = sampleDaemon(FORKED_GIT);
    const listed = await daemon.request({ type: "list_project_branches", project: "p-1" });
    expect(listed).toMatchObject({ type: "project_branches", default_branch: FORKED_GIT.defaultBranch });
    const compared = await daemon.request({ type: "compare_project_branches", project: "p-1", left: "origin/main", left_remote: true, right: "feature/page-size" });
    expect(compared).toMatchObject({ type: "project_comparison", left: { branch: "origin/main", remote: true }, base: FORKED_GIT.defaultBranch?.commit });
    expect(compared.type === "project_comparison" ? compared.changes.length : 0).toBe(2);
    // A lookup past its budgets leaves the default branch out, as it does the upstream.
    const cut = await sampleDaemon({ ...FORKED_GIT, upstreamsCut: true }).request({ type: "list_project_branches", project: "p-1" });
    expect(cut).toMatchObject({ type: "project_branches", upstreams_complete: false });
    expect(cut.type === "project_branches" ? cut.default_branch : null).toBeUndefined();
  });

  it("answer a branch list incomplete when the lookup was cut short", async () => {
    const cut = await sampleDaemon({ ...FORKED_GIT, branchesCut: true }).request({ type: "list_project_branches", project: "p-1" });
    expect(cut).toMatchObject({ type: "project_branches", complete: false, branches: FORKED_GIT.branches });
  });

  it("answer a comparison of a branch with its upstream or with nothing", async () => {
    const daemon = sampleDaemon(UNPUSHED_GIT);
    const upstream = UNPUSHED_GIT.branches?.find((b) => b.name === "main")?.upstream;
    const compared = await daemon.request({ type: "compare_project_branches", project: "p-1", left: "origin/main", left_remote: true, right: "main" });
    expect(compared).toMatchObject({
      type: "project_comparison",
      left: { branch: "origin/main", commit: upstream?.commit, remote: true },
      right: { branch: "main" },
      base: upstream?.commit,
    });
    expect(compared.type === "project_comparison" ? compared.changes.length : 0).toBe(2);
    // A remote-tracking branch no branch has as its upstream is unknown.
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "origin/gone", left_remote: true, right: "main" })).rejects.toMatchObject({ code: "unknown_branch" });
    const tip = UNPUSHED_GIT.branches?.find((b) => b.name === "main")?.commit ?? "";
    const listed = await daemon.request({ type: "list_project_commits", project: "p-1", base: upstream?.commit ?? "", tip });
    expect(listed.type === "project_commits" ? listed.commits.map(({ subject }) => subject) : listed.type).toEqual(["Add a changelog", "Show 25 results to a page"]);

    // Only the left side can be a remote-tracking branch or the empty tree.
    const change = { old: { state: "absent" }, new: { state: "absent" } } as const;
    const endpoint = { branch: "main", commit: tip };
    await expect(
      daemon.request({ type: "read_project_comparison_change", project: "p-1", left: { branch: "origin/main", commit: upstream?.commit ?? "", remote: true }, right: { ...endpoint, remote: true }, base: upstream?.commit ?? "", change }),
    ).rejects.toMatchObject({ code: "invalid_branch_name", params: { branch: "main" } });
    await expect(
      daemon.request({ type: "read_project_comparison_change_bodies", project: "p-1", left: { branch: "", commit: EMPTY_TREE, empty: true }, right: { ...endpoint, empty: true }, base: EMPTY_TREE, change, old: { state: "absent" }, new: { state: "absent" } }),
    ).rejects.toMatchObject({ code: "invalid_branch_name" });

    // An upstream lookup past its budgets leaves the main branch without one, and the list says so.
    const cut = await sampleDaemon({ ...UNPUSHED_GIT, upstreamsCut: true }).request({ type: "list_project_branches", project: "p-1" });
    expect(cut).toMatchObject({ type: "project_branches", upstreams_complete: false });
    expect(cut.type === "project_branches" ? cut.branches.some((branch) => branch.upstream) : undefined).toBe(false);

    const whole = sampleDaemon(NEVER_PUSHED_GIT);
    const all = await whole.request({ type: "compare_project_branches", project: "p-1", left: "", left_empty: true, right: "main" });
    expect(all).toMatchObject({ type: "project_comparison", left: { branch: "", commit: EMPTY_TREE, empty: true }, base: EMPTY_TREE });
    expect(all.type === "project_comparison" ? all.changes.map((entry) => (entry.group !== "conflicted" && entry.new.state === "present" ? entry.new.path : "")) : []).toEqual(["src/server.ts", "src/search.ts", "docs/overview.md", "CHANGELOG.md"]);
    const history = await whole.request({ type: "list_project_commits", project: "p-1", base: EMPTY_TREE, tip });
    expect(history.type === "project_commits" ? history.commits.map(({ subject }) => subject) : history.type).toEqual(["Add a changelog", "Show 25 results to a page", "Start the search API"]);
  });

  it("answer a comparison with the commit the branches forked at", async () => {
    const compare = (git: FixtureGit, left: string, right: string) =>
      sampleDaemon(git).request({ type: "compare_project_branches", project: "p-1", left, right });
    const feature = SAMPLE_GIT.branches?.find((b) => b.name === "feature/ranking-experiments")?.commit;
    const main = SAMPLE_GIT.branches?.find((b) => b.name === "main")?.commit;
    const base = (reply: Event) => (reply.type === "project_comparison" ? [reply.base, reply.changes.length] : reply.type);

    // A branch made from main forks at main's tip; a branch with commits of its own forks before them.
    expect(base(await compare(SAMPLE_GIT, "main", "feature/ranking-experiments"))).toEqual([main, 9]);
    expect(base(await compare(SAMPLE_GIT, "main", "chore/update-dependencies"))).toEqual([SAMPLE_GIT.forks?.["main..chore/update-dependencies"], 2]);
    // The same commit, and a right branch that is an ancestor of the left one, change nothing.
    expect(base(await compare(SAMPLE_GIT, "main", "release/2.4"))).toEqual([main, 0]);
    expect(base(await compare(SAMPLE_GIT, "feature/ranking-experiments", "main"))).toEqual([feature, 0]);
    // No commit in common is refused rather than compared tip to tip.
    const orphaned: FixtureGit = { ...SAMPLE_GIT, branches: [...(SAMPLE_GIT.branches ?? []), { name: "gh-pages", commit: "3a2b1c0" }], forks: { "main..gh-pages": null } };
    await expect(compare(orphaned, "main", "gh-pages")).rejects.toMatchObject({ code: "no_merge_base", params: { left: "main", right: "gh-pages" } });
    await expect(compare({ ...orphaned, shallow: true }, "main", "gh-pages")).rejects.toMatchObject({ code: "no_merge_base_shallow", params: { left: "main", right: "gh-pages" } });
  });

  it("list the commits a branch made since the fork, the newest first", async () => {
    const tipOf = (branch: string) => SAMPLE_GIT.branches?.find((b) => b.name === branch)?.commit ?? "";
    const list = async (git: FixtureGit, base: string, tip: string) => {
      const reply = await sampleDaemon(git).request({ type: "list_project_commits", project: "p-1", base, tip });
      if (reply.type !== "project_commits") throw new Error(reply.type);
      return reply;
    };
    const ranking = await list(SAMPLE_GIT, tipOf("main"), tipOf("feature/ranking-experiments"));
    // A short history is listed whole, so the pane says nothing of a limit.
    expect([ranking.commits.length, ranking.commits_complete, ranking.complete]).toEqual([5, true, true]);
    expect(ranking.commits.map(({ subject }) => subject)).toEqual([
      "Merge the ranking notes from the archived repository",
      "Apply boosts after deduplication",
      "Import the ranking notes from the archived repository",
      "Cache query results",
      "Move the loader to config.ts and clear out the old files",
    ]);
    const chore = await list(SAMPLE_GIT, SAMPLE_GIT.forks?.["main..chore/update-dependencies"] ?? "", tipOf("chore/update-dependencies"));
    expect([chore.commits.map(({ subject }) => subject), chore.commits_complete]).toEqual([["Bump the test runner", "Raise the TypeScript target"], true]);
    // A comparison that lists changes has commits they are made of; a fixture without them is refused.
    const bare: FixtureGit = { ...SAMPLE_GIT, commits: {} };
    vi.spyOn(console, "error").mockImplementation(() => {});
    await expect(list(bare, tipOf("main"), tipOf("feature/ranking-experiments"))).rejects.toThrow(FixtureUnfillableError);
  });

  // A note that only the newest commits are listed has to look true: the list is the daemon's
  // hundred, each changing something in the folder (a commit that changed nothing has no row), cut
  // at a commit it does not hold, and the comparison is made of the same changes.
  it("list a hundred commits that each change something when a long history is cut", async () => {
    const tipOf = (branch: string) => LONG_HISTORY_GIT.branches?.find((b) => b.name === branch)?.commit ?? "";
    const reply = await sampleDaemon(LONG_HISTORY_GIT).request({ type: "list_project_commits", project: "p-1", base: tipOf("main"), tip: tipOf("feature/ranking-experiments") });
    if (reply.type !== "project_commits") throw new Error(reply.type);
    expect([reply.commits.length, reply.commits_complete]).toEqual([100, false]);
    expect(reply.commits.filter(({ changes }) => changes.length === 0)).toEqual([]);
    expect(new Set(reply.commits.map(({ subject }) => subject)).size).toBe(100);
    const listed = new Set(reply.commits.map(({ commit }) => commit));
    // Every parent is listed but the oldest commit's, which the cut left out.
    expect(reply.commits.filter(({ parent }) => parent !== null && !listed.has(parent))).toHaveLength(1);
    expect(reply.commits[0].commit).toBe(tipOf("feature/ranking-experiments"));
    expect(objectIdCheck(LONG_HISTORY_GIT).conflicts).toEqual([]);
    const comparison = LONG_HISTORY_GIT.comparisons?.["main..feature/ranking-experiments"] ?? [];
    expect(comparison.length).toBe(reply.commits.reduce((sum, { changes }) => sum + changes.length, 0));
  });

  it("find a gone branch only until a comparison has been made", async () => {
    const daemon = sampleDaemon({ ...SAMPLE_GIT, gone: ["feature/ranking-experiments"] });
    const compare = { type: "compare_project_branches", project: "p-1", left: "main", right: "feature/ranking-experiments" } as const;
    await expect(daemon.request(compare)).resolves.toMatchObject({ type: "project_comparison" });
    const remaining = SAMPLE_GIT.branches?.filter((b) => b.name !== "feature/ranking-experiments");
    await expect(daemon.request({ type: "list_project_branches", project: "p-1" })).resolves.toEqual({ type: "project_branches", id: null, project: "p-1", branches: remaining, complete: true });
    await expect(daemon.request(compare)).rejects.toMatchObject({ code: "unknown_branch", params: { branch: "feature/ranking-experiments" } });
  });

  it("take a branch of goneAfter away once its pair has been compared, and not for the comparison of any other pair", async () => {
    const daemon = sampleDaemon({ ...SAMPLE_GIT, goneAfter: { "main..chore/update-dependencies": ["main"] } });
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "origin/main", left_remote: true, right: "main" })).resolves.toMatchObject({ type: "project_comparison" });
    await expect(daemon.request({ type: "list_project_branches", project: "p-1" })).resolves.toMatchObject({ branches: SAMPLE_GIT.branches });
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "main", right: "chore/update-dependencies" })).resolves.toMatchObject({ type: "project_comparison" });
    const remaining = SAMPLE_GIT.branches?.filter((b) => b.name !== "main");
    await expect(daemon.request({ type: "list_project_branches", project: "p-1" })).resolves.toMatchObject({ branches: remaining });
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "main", right: "chore/update-dependencies" })).rejects.toMatchObject({ code: "unknown_branch", params: { branch: "main" } });
  });

  it("keep a gone branch listed while only other branches have been compared", async () => {
    const daemon = sampleDaemon({ ...SAMPLE_GIT, gone: ["chore/update-dependencies"] });
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "origin/main", left_remote: true, right: "main" })).resolves.toMatchObject({ type: "project_comparison" });
    await expect(daemon.request({ type: "list_project_branches", project: "p-1" })).resolves.toMatchObject({ branches: SAMPLE_GIT.branches });
    await expect(daemon.request({ type: "compare_project_branches", project: "p-1", left: "main", right: "chore/update-dependencies" })).resolves.toMatchObject({ type: "project_comparison" });
  });

  // A side with no line count leaves an expandable diff ending in a bar promising unmodified lines
  // it may have none of, which a count takes away (`docs/product/project-pane-git-change-viewer.md`,
  // "Expanding the collapsed lines"). The daemon counts every side it reads, so a sample change whose
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
      const ends = endsOf(SAMPLE_GIT, comparison);
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const reply = await daemon
          .request({ type: "read_project_comparison_change", project: "p-1", ...ends, change: entry })
          .catch(rethrowUnfillable);
        if (reply?.type !== "project_comparison_change" || !fileSide(entry.old) || !fileSide(entry.new)) continue;
        check(`${comparison}: ${pathOf(entry.new)}`, reply.patch?.text ?? null, reply.old, reply.new);
      }
    }

    for (const { commit, parent, changes } of commitsOf(SAMPLE_GIT)) {
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const reply = await daemon.request({ type: "read_project_commit_change", project: "p-1", commit, parent, change: entry }).catch(rethrowUnfillable);
        if (reply?.type !== "project_commit_change" || !fileSide(entry.old) || !fileSide(entry.new)) continue;
        check(`commit ${short(commit)}: ${pathOf(entry.new)}`, reply.patch?.text ?? null, reply.old, reply.new);
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
      "main..chore/update-dependencies: package.json",
      "main..chore/update-dependencies: tsconfig.json",
      "commit a7b6c5d: src/ranking.ts",
      "commit a7b6c5d: src/pipeline.ts",
      "commit 6f5e4d3: src/config.ts",
      "commit 6d5c4b3: package.json",
      "commit 3e2d1c0: tsconfig.json",
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
      if (reply.type !== "project_change_bodies" && reply.type !== "project_comparison_change_bodies" && reply.type !== "project_commit_change_bodies") throw new Error(`${at}: ${reply.type}`);
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
      const ends = endsOf(SAMPLE_GIT, comparison);
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const at = `${comparison} ${pathOf(entry.new) || pathOf(entry.old)}`;
        const request = { type: "read_project_comparison_change_bodies" as const, project: "p-1", ...ends, change: entry, ...versionsOf(entry) };
        asked.push(answer(at, daemon.request(request)));
      }
    }

    for (const { commit, parent, changes } of commitsOf(SAMPLE_GIT)) {
      for (const { entry } of changes) {
        if (entry.group === "conflicted") continue;
        const at = `commit ${short(commit)} ${pathOf(entry.new) || pathOf(entry.old)}`;
        const request = { type: "read_project_commit_change_bodies" as const, project: "p-1", commit, parent, change: entry, ...versionsOf(entry) };
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
      "main..feature/ranking-experiments docs/ranking-notes.md: old absent, new text 55/2",
      "main..feature/ranking-experiments docs/old-notes.md: old text 53/2, new absent",
      "main..feature/ranking-experiments docs/logo.png: old image/png 507/7, new image/png 516/8",
      "main..feature/ranking-experiments current: unsupported_file_type",
      "main..chore/update-dependencies package.json: old text 78/6, new text 78/6",
      "main..chore/update-dependencies tsconfig.json: old text 74/6, new text 74/6",
      "commit 9e8d7c6 docs/ranking-notes.md: old absent, new text 55/2",
      "commit a7b6c5d src/ranking.ts: old text 114/3, new text 180/4",
      "commit a7b6c5d src/pipeline.ts: old text 5304/120, new text 5349/120",
      "commit 5e4d3c2 docs/ranking-notes.md: old absent, new text 55/2",
      "commit 8d7c6b5 src/cache.ts: old absent, new text 49/1",
      // As a comparison's, a change with a side outside the project has no patch to expand.
      "commit 8d7c6b5 src/index.ts: invalid_change",
      "commit 6f5e4d3 src/config.ts: old text 92/3, new text 100/3",
      "commit 6f5e4d3 docs/old-notes.md: old text 53/2, new absent",
      "commit 6f5e4d3 docs/logo.png: old image/png 507/7, new image/png 516/8",
      "commit 6f5e4d3 current: unsupported_file_type",
      "commit 6d5c4b3 package.json: old text 78/6, new text 78/6",
      "commit 3e2d1c0 tsconfig.json: old text 74/6, new text 74/6",
    ]);
  });

  // The guard over every scenario: whatever a scenario's repository offers to read is read here, so
  // a reply the fixture daemon cannot fill faithfully fails the test run and not only the scenario
  // someone opens. The repositories are taken from the scenario catalog rather than listed, so a
  // fixture added later is read without being named; so are the repositories a branch switch or a
  // restore replaces a project's with, and the sample repository every scenario without a `git`
  // is answered from. A change shared by several repositories is read once. Each read has to reach
  // its target: a daemon error passes only where the fixture stages that very error for the change,
  // since scenarios answer with errors on purpose, and a commit list has the commits the fixture holds.
  it("answer every read a scenario's repository offers without a refusal", async () => {
    const repositories = new Set<FixtureGit>();
    for (const scenario of SCENARIOS) {
      if (scenario.git !== null) repositories.add(scenario.git ?? SAMPLE_GIT);
      for (const result of Object.values(scenario.projectRestores ?? {})) if (!("code" in result)) repositories.add(result.git);
      for (const switches of Object.values(scenario.projectBranchSwitches ?? {})) {
        for (const result of Object.values(switches)) if (!("code" in result)) repositories.add(result.git);
      }
    }
    // Several repositories are spread from one another, so most of their changes are one object.
    const read = { changes: new WeakSet<object>(), comparisons: new WeakSet<object>(), commits: new WeakSet<object>() };
    const count = { changes: 0, comparisons: 0, commits: 0, lists: 0 };
    const unseen = (seen: WeakSet<object>, change: object) => !seen.has(change) && !!seen.add(change);
    /** The error a read of the change is staged to end in, in the order the daemon meets them: what
     * the request alone refuses, the patch it cannot make, then (for the bodies) a side that moved
     * on, one that is no file, and the error reading them ends in. `undefined` for a read that is
     * answered. */
    const stagedError = (change: FixtureChange, entry: Exclude<ChangeEntry, { group: "conflicted" }>, bodies: boolean): string | undefined => {
      const { error, expand } = change;
      if (!bodies) return error?.code;
      if (entry.group === "untracked" || entry.old.state === "out_of_scope" || entry.new.state === "out_of_scope") return "invalid_change";
      if (error) return error.code;
      if (expand?.movedOn) return "source_changed";
      if ([entry.old, entry.new].some((side) => side.state === "present" && side.kind !== "file")) return "unsupported_file_type";
      return expand?.error?.code;
    };
    const asked: Promise<void>[] = [];

    for (const git of repositories) {
      const daemon = sampleDaemon(git);
      /** Asks the repository for a read and checks the reply is the one that was aimed at: the type
       * wanted, or the very error staged for it; a refusal, or any other error, fails. */
      const ask = (at: string, request: RequestBody, reply: Event["type"], error: string | undefined, then?: (event: Event) => void) => {
        asked.push(
          daemon.request(request).then(
            (event) => {
              expect(event.type, `${at} answers ${reply}`).toBe(reply);
              expect(error, `${at} is staged to end in an error`).toBeUndefined();
              then?.(event);
            },
            (failure: unknown) => {
              if (!(failure instanceof DaemonRequestError)) throw failure;
              expect(failure.code, `${at} fails with the error staged for it`).toBe(error);
            },
          ),
        );
      };

      for (const [worktree, changes] of Object.entries(git.changes)) {
        if (!Array.isArray(changes)) continue;
        for (const change of changes) {
          const { entry } = change;
          if (entry.group === "conflicted" || entry.group === "committed" || !unseen(read.changes, change)) continue;
          count.changes += 1;
          const at = `${worktree} ${entry.group} ${pathOf(entry.new) || pathOf(entry.old)}`;
          const named = { group: entry.group, old: entry.old, new: entry.new };
          ask(at, { type: "read_project_change", project: "p-1", worktree, change: named }, "project_change", stagedError(change, entry, false));
          ask(`${at} bodies`, { type: "read_project_change_bodies", project: "p-1", worktree, change: named, ...versionsOf(entry) }, "project_change_bodies", stagedError(change, entry, true));
        }
      }
      const pairs = new Set([...Object.keys(git.comparisons ?? {}), ...Object.keys(git.commits ?? {})]);
      for (const pair of pairs) {
        const ends = endsOf(git, pair);
        const commits = git.commits?.[pair];
        // A list that is never answered is not read, nor is one of a pair that cannot be compared: it
        // has no fork to name (`null`), or names a branch the branch list does not have (a repository
        // whose `main` is not called that), where the window never gets as far as asking. Any other
        // pair is read, so a pair key that does not name what the fixture holds fails here.
        const known = (name: string) =>
          name === "" || [git.branches?.some((b) => b.name === name || b.upstream?.name === name), git.defaultBranch?.name === name].includes(true);
        const comparable = git.forks?.[pair] !== null && known(ends.left.branch) && known(ends.right.branch);
        if (!(commits && "pending" in commits) && comparable) {
          count.lists += 1;
          const staged = commits && "error" in commits ? commits.error.code : undefined;
          ask(`${pair} commits`, { type: "list_project_commits", project: "p-1", base: ends.base, tip: ends.right.commit }, "project_commits", staged, (event) => {
            // The count is what shows the list read the pair asked for, and not another or none.
            expect(event.type === "project_commits" ? event.commits.length : -1, `${pair} lists the commits the fixture holds`).toBe(commits ? listedCommits(commits).length : 0);
          });
        }
        for (const change of git.comparisons?.[pair] ?? []) {
          const { entry } = change;
          if (entry.group === "conflicted" || !unseen(read.comparisons, change)) continue;
          count.comparisons += 1;
          const at = `${pair} ${pathOf(entry.new) || pathOf(entry.old)}`;
          ask(at, { type: "read_project_comparison_change", project: "p-1", ...ends, change: entry }, "project_comparison_change", stagedError(change, entry, false));
          ask(`${at} bodies`, { type: "read_project_comparison_change_bodies", project: "p-1", ...ends, change: entry, ...versionsOf(entry) }, "project_comparison_change_bodies", stagedError(change, entry, true));
        }
      }
      for (const { commit, parent, changes } of commitsOf(git)) {
        for (const change of changes) {
          const { entry } = change;
          if (entry.group === "conflicted" || !unseen(read.commits, change)) continue;
          count.commits += 1;
          const at = `commit ${short(commit)} ${pathOf(entry.new) || pathOf(entry.old)}`;
          ask(at, { type: "read_project_commit_change", project: "p-1", commit, parent, change: entry }, "project_commit_change", stagedError(change, entry, false));
          ask(`${at} bodies`, { type: "read_project_commit_change_bodies", project: "p-1", commit, parent, change: entry, ...versionsOf(entry) }, "project_commit_change_bodies", stagedError(change, entry, true));
        }
      }
    }

    // Every request is in flight before the first is awaited, as above.
    for (const one of await Promise.allSettled(asked)) if (one.status === "rejected") throw one.reason;
    // The catalog was walked, not an empty one: each kind of read has something to read.
    expect(repositories.has(SAMPLE_GIT)).toBe(true);
    expect(repositories.size).toBeGreaterThan(1);
    for (const kind of Object.values(count)) expect(kind).toBeGreaterThan(0);
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
