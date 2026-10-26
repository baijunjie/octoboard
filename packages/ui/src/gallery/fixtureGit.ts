// What the fixture daemon answers a project pane's Git mode with, over a fixture repository
// (`fixtures/changeFixtures.ts`): the source, the change lists, a change's patch and the whole
// bodies behind it, the branch list, a comparison of one branch with another (what it has changed
// since it forked from it) and the commits it made since.
//
// These are the replies that have to track the real daemon's own
// (`apps/daemon/src/browse/changes.rs`), condition for condition, and they are kept apart from the
// rest of the fixture daemon for that reason. Where the daemon would fill a field and the fixture
// carries nothing to fill it from, the reply is refused rather than sent unfilled; where a
// signature could have settled it instead, the factory that builds the change does.
import type { ChangeSide, ContentSource, Event, FileContent, RequestBody, SideRead, SideRef, SideVersion } from "../protocol";
import { refuseUnfillable } from "./banner";
import { text } from "./fixtures/builders";
import { EMPTY_TREE, listedCommits, type FixtureGit } from "./fixtures/changeFixtures";
import type { FixtureError } from "./fixtures/projectFiles";

/** How many lines a text holds: its newlines, and one more when the last line is unterminated. It
 * repeats `countLines` of `viewer/budgets.ts` on purpose: the fixture stands in for the daemon's
 * count and does not depend on a viewer render-budget helper. */
function lineCount(content: string): number {
  return content === "" ? 0 : content.split("\n").length - (content.endsWith("\n") ? 1 : 0);
}

/** A body's bytes with one character each, which `lineCount` then counts as the daemon counts the
 * bytes it read: a text body's newline characters are its newline bytes, and a binary one is
 * decoded so that an image counts the newlines in its own bytes, as the daemon's does. */
function bodyBytes(body: FileContent): string {
  return body.text ?? atob(body.data ?? "");
}
/**
 * A side of a fixture change as `read_project_change` reads it, with its body when it has one and
 * its line count where the daemon counts one (`apps/daemon/PROTOCOL.md`, "Changes and
 * comparisons"). Only a file side ever carries either: the daemon reads no content for a link or
 * a submodule. A carried body is counted from the body itself, whatever the patch holds;
 * otherwise only a side the daemon read an object for is counted, from `content`, the whole text
 * of that version. So `lines` stays null for a link or a submodule, for an absent or out-of-scope
 * side, for a side read from the worktree disk with no body, and for a side of a patch with no
 * hunk (`hunks` false) that carries no body, where no separator bar needs a count. Those are what
 * the real daemon leaves out, not gaps.
 *
 * Where the daemon would count and the fixture carries no `content` to count, the reply is
 * refused rather than sent with the field unfilled: a reply the daemon never sends is worse than
 * no reply, because it is believed. The gallery is where a UI state is looked at before it is
 * staged through a daemon, so a combination the app cannot produce reads there as the app's
 * defect. An unfilled count is a legal value of the reply, so nothing downstream — the type
 * check, the renderer, a scenario — can tell it from a real one.
 */
const readSide = (at: string, side: ChangeSide, body: FileContent | undefined, content?: string, hunks = true): SideRead => {
  if (side.state === "absent") return side;
  if (side.state !== "present") return { state: "out_of_scope" };
  if (side.kind !== "file") return { ...side, file: null, lines: null };
  if (body) return { ...side, file: body, lines: lineCount(bodyBytes(body)) };
  if (!hunks || side.source.kind === "live") return { ...side, file: null, lines: null };
  if (content === undefined) {
    refuseUnfillable(`${at} (${side.path}): the daemon counts this side's lines, and the fixture carries no whole text to count`);
  }
  return { ...side, file: null, lines: lineCount(content) };
};
/** The daemon's refusal to read a side that is not a regular file, which a request for a change's
 * bodies meets before it reads anything. */
const notAFile = (side: ChangeSide): FixtureError | undefined => {
  if (side.state !== "present" || side.kind === "file") return undefined;
  const params = { path: side.path, file_type: side.kind };
  return { code: "unsupported_file_type", params, message: `${side.path} is a ${side.kind}, which cannot be read here` };
};
/** Whether a change reaches across the project's boundary, which the daemon makes no patch for
 * and so reads no bodies of. */
const outsideTheProject = (change: { old: ChangeSide; new: ChangeSide }): boolean =>
  change.old.state === "out_of_scope" || change.new.state === "out_of_scope";
/** Whether two sources name the same version, field by field as the daemon compares them: a
 * serialization would turn a source written with its keys in another order into a refusal the
 * daemon never makes. */
const sameSource = (a: ContentSource, b: ContentSource): boolean => {
  if (a.kind === "live" && b.kind === "live") return a.root_id === b.root_id && a.version === b.version;
  if (a.kind === "index" && b.kind === "index") return a.worktree === b.worktree && a.blob === b.blob;
  if (a.kind === "commit" && b.kind === "commit") return a.commit === b.commit && a.branch === b.branch && a.blob === b.blob;
  return false;
};

/** Whether a side is still at the version a request names it at, which the daemon checks before it
 * reads any body: an absent side is named absent, and a present one by the very source its diff
 * reply reported. */
const sameVersion = (side: ChangeSide, named: SideVersion): boolean =>
  side.state === "present" ? named.state === "present" && sameSource(named.source, side.source) : named.state === "absent";
/** The daemon's refusal of a change a side of which has moved on since its patch was made. */
const sourceChanged = (entry: { old: ChangeSide; new: ChangeSide }): FixtureError => {
  // The new side's path, else the old one's, as the daemon names the change it refuses.
  const named = [entry.new, entry.old].find((side) => side.state === "present");
  const shown = named?.state === "present" ? named.path : "";
  return { code: "source_changed", params: { path: shown }, message: `${shown} changed while it was being read` };
};
/** Whether a fixture change's patch has a hunk, which a count of its sides is for. */
const hasHunk = (found: { patch?: string }) => /^@@ /m.test(found.patch ?? "");
const sameSide = (listed: ChangeSide, asked: SideRef) =>
  listed.state === asked.state && (listed.state !== "present" || (asked.state === "present" && asked.path === listed.path));

/**
 * The Git-mode replies over `git`, the fixture repository a scenario gives (`null` for a project in
 * no repository), answering with `fail` where the daemon answers with an error. The returned
 * function answers a request of this mode and nothing else: `undefined` says the request is not
 * one of them.
 */
export function createGitReplies(git: FixtureGit | null | undefined, fail: (error: FixtureError) => never) {
  /** Whether a comparison of a branch that is to go has been made, after which the branch list tells
   * of the branches gone: a comparison of any other branch, the default one's with its upstream
   * included, does not take them away. */
  let compared = false;
  /** The branches gone once the comparison of the pair that names them has been made (`goneAfter`). */
  const goneAfterPair = new Set<string>();
  const isGone = (name: string) => (compared && git?.gone?.includes(name)) === true || goneAfterPair.has(name);
  /** The daemon's refusal of a change whose patch has no lines to expand. */
  const noBodies = (): never => fail({ code: "invalid_change", params: {}, message: "no patch to expand" });
  /** The worktree a request is answered from: the one it names, else the project's own, as the
   * daemon resolves an omitted one. */
  const worktreeOf = (named: string | undefined) => named ?? git?.worktrees[0]?.id ?? "";
  const shortId = (commit: string) => commit.slice(0, 7);
  /** The commit a request names, which the daemon reads between it and the parent the request
   * names, whatever that is; the fixture holds only the parent the commit list gave. */
  const commitOf = (commit: string, parent: string | null | undefined) =>
    Object.values(git?.commits ?? {})
      .flatMap(listedCommits)
      .find((listed) => listed.commit === commit && listed.parent === (parent ?? null));
  /** The commit a remote-tracking branch is at: the upstream some branch has, else the default
   * branch the list gives. */
  const remoteTip = (name: string): string | undefined =>
    git?.branches?.find((b) => b.upstream?.name === name)?.upstream?.commit ?? (git?.defaultBranch?.name === name ? git.defaultBranch.commit : undefined);
  /** The commit a name is at in the branch list: a local branch, else the remote-tracking branch
   * some branch has as its upstream or the list gives as the default one. A pair's key spells a remote-tracking left side by its name
   * alone, as no local branch of the fixtures is named like one, and the empty tree by no name at all. */
  const tipOf = (name: string): string | undefined =>
    name === "" ? EMPTY_TREE : git?.branches?.find((b) => b.name === name)?.commit ?? remoteTip(name);
  /** The commit two branches fork at: their own, if they are at one commit, else the one `forks`
   * names, else `left`'s tip, as a branch made from it forks. `null` for two with no commit in common. */
  const forkOf = (key: string, left: string, right: string): string | null =>
    left === right ? left : git?.forks?.[key] === undefined ? left : git.forks[key];
  /** The pair `<left>..<right>` whose fork is `base` and whose right branch is at `tip`, the commits
   * a commit list request names a comparison by. */
  const pairAt = (base: string, tip: string): string | undefined =>
    [...Object.keys(git?.comparisons ?? {}), ...Object.keys(git?.commits ?? {})].find((pair) => {
      const [left, right] = pair.split("..").map(tipOf);
      return left !== undefined && right === tip && forkOf(pair, left, right) === base;
    });
  /** The whole bodies behind a change's patch, for expanding the lines it collapsed. */
  const readBodies = async (
    body: Extract<RequestBody, { type: "read_project_change_bodies" | "read_project_comparison_change_bodies" | "read_project_commit_change_bodies" }>,
  ): Promise<Event> => {
    /** Where the request reads: what a refusal names it by, the changes it can name, and, for a
     * worktree's, the section the change must be in (a comparison's and a commit's are all committed). */
    const target = (() => {
      switch (body.type) {
        case "read_project_change_bodies": {
          const worktree = worktreeOf(body.worktree);
          return { at: `${worktree} ${body.change.group} bodies`, changes: git?.changes[worktree], group: body.change.group };
        }
        case "read_project_comparison_change_bodies": {
          const pair = `${body.left.branch}..${body.right.branch}`;
          return { at: `${pair} bodies`, changes: git?.comparisons?.[pair], group: undefined };
        }
        case "read_project_commit_change_bodies":
          return { at: `commit ${shortId(body.commit)} bodies`, changes: commitOf(body.commit, body.parent)?.changes, group: undefined };
      }
    })();
    const { at, changes, group } = target;
    const found = Array.isArray(changes)
      ? changes.find(
        ({ entry }) =>
          entry.group !== "conflicted" &&
            (group === undefined || entry.group === group) &&
            sameSide(entry.old, body.change.old) &&
            sameSide(entry.new, body.change.new),
      )
      : undefined;
    if (!found || found.entry.group === "conflicted") return noBodies();
    // The two refusals the daemon makes from the request alone, before it resolves the change at
    // all: an untracked file has no patch whose lines could be collapsed, and neither has a change
    // with a side outside the project, since a patch would carry that side's content.
    if (found.entry.group === "untracked" || outsideTheProject(found.entry)) return noBodies();
    // The change is resolved by making its patch first, so a change whose patch the daemon refuses
    // to make answers a request for its bodies with that very refusal.
    if (found.error) return fail(found.error);
    const { delay, movedOn, error } = found.expand ?? {};
    if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
    // Then the version check, before any side is read: the daemon compares the side it finds with
    // the version the request names, and will not mix one side's revision with another's. A static
    // fixture cannot report one version and then be asked about another, so a change that is to
    // meet this declares the move itself.
    const stale = movedOn || !sameVersion(found.entry.old, body.old) || !sameVersion(found.entry.new, body.new);
    if (stale) return fail(sourceChanged(found.entry));
    // Only then is a side read, and the daemon reads no content for a link or a submodule: it
    // refuses the whole request rather than answering that side empty.
    const unsupported = notAFile(found.entry.old) ?? notAFile(found.entry.new);
    if (unsupported) return fail(unsupported);
    if (error) return fail(error);
    const whole = found.whole ?? {};
    /** The whole body of a side as this reply carries it. The fixture spells a side's content
     * as text (`whole`) or, where it is not text, as the body the diff reply carries for it
     * (`bodies`); the daemon reads one and the same version for both replies, so a side has
     * one content whichever way it is spelled. */
    const wholeBody = (which: "old" | "new"): FileContent | undefined => {
      const content = whole[which];
      return content === undefined ? found.bodies?.[which] : text(content);
    };
    /** A side as the bodies reply reads it. Unlike a diff reply, this one reads every present file
     * side whatever its source, so a side with no body here is a gap in the fixture rather than one
     * the daemon leaves empty. */
    const readWhole = (side: ChangeSide, file: FileContent | undefined): SideRead => {
      if (side.state === "present" && side.kind === "file" && file === undefined) {
        refuseUnfillable(`${at} (${side.path}): the daemon reads this side's whole body, and the fixture carries none`);
      }
      return readSide(at, side, file);
    };
    const sides = { old: readWhole(found.entry.old, wholeBody("old")), new: readWhole(found.entry.new, wholeBody("new")) };
    switch (body.type) {
      case "read_project_change_bodies":
        return { type: "project_change_bodies", project: body.project, worktree: body.worktree ?? null, group: body.change.group, ...sides };
      case "read_project_comparison_change_bodies":
        return { type: "project_comparison_change_bodies", project: body.project, left: body.left, right: body.right, base: body.base, ...sides };
      case "read_project_commit_change_bodies":
        return { type: "project_commit_change_bodies", project: body.project, commit: body.commit, parent: body.parent, ...sides };
    }
  };

  /** The reply to a Git-mode request, or `undefined` where the request is not one of them. */
  return (body: RequestBody): Event | Promise<Event> | undefined => {
    switch (body.type) {
      case "get_project_source": {
        // The project's folder is the `scope` below the root of the worktree holding it, as the daemon names it; a
        // project in no repository is the sample's own folder.
        const own = git?.worktrees[0];
        const scope = git?.scope ?? "";
        const root = scope && own ? `${own.root}/${scope}` : (own?.root ?? "/Users/dev/code/search-api");
        const main = git?.worktrees.find((worktree) => worktree.main) ?? own;
        return {
          type: "project_source",
          source: {
            project: body.project,
            root,
            resolved_root: root,
            root_id: "fixture",
            git:
              git && !git.gitError && own && main
                ? { repository: "repo", common_dir: `${main.root}/.git`, worktree: own.id, scope, worktrees: git.worktrees }
                : null,
            git_error: git?.gitError ?? null,
          },
        };
      }
      case "list_project_changes": {
        const worktree = worktreeOf(body.worktree);
        const changes = git?.changes[worktree];
        if (!changes) return fail({ code: "worktree_unavailable", params: { worktree }, message: `worktree ${worktree} is gone` });
        if (!Array.isArray(changes)) return fail(changes);
        return {
          type: "project_changes",
          project: body.project,
          worktree: body.worktree ?? null,
          head: git?.worktrees.find((w) => w.id === worktree)?.head ?? null,
          changes: changes.map((change) => change.entry),
          complete: !git?.cutShort,
        };
      }
      case "list_project_branches": {
        const listed = (git?.branches ?? []).filter((branch) => !isGone(branch.name));
        // A lookup past its budgets leaves the main branch without an upstream and the list without a default branch, and the list says so.
        if (git?.upstreamsCut) {
          return { type: "project_branches", project: body.project, branches: listed.map(({ upstream: _upstream, ...branch }) => branch), complete: true, upstreams_complete: false };
        }
        return { type: "project_branches", project: body.project, branches: listed, complete: !git?.branchesCut, ...(git?.defaultBranch ? { default_branch: git.defaultBranch } : {}) };
      }
      case "compare_project_branches": {
        const resolve = (name: string, remote: boolean) => {
          if (remote) {
            const commit = remoteTip(name);
            if (commit === undefined) return fail({ code: "unknown_branch", params: { branch: name }, message: `there is no remote-tracking branch ${name}` });
            return { branch: name, commit, remote: true };
          }
          const branch = git?.branches?.find((b) => b.name === name);
          if (!branch || git?.missing?.includes(name) || isGone(name)) return fail({ code: "unknown_branch", params: { branch: name }, message: `there is no local branch ${name}` });
          return { branch: name, commit: branch.commit };
        };
        if (git?.comparing === "pending") return new Promise<Event>(() => {});
        // Compared with the empty tree, `left` is ignored and the pair is keyed by an empty name.
        const left = body.left_empty ? { branch: "", commit: EMPTY_TREE, empty: true } : resolve(body.left, body.left_remote === true);
        const right = resolve(body.right, false);
        compared ||= [body.left, body.right].some((name) => git?.gone?.includes(name));
        const key = `${body.left_empty ? "" : body.left}..${body.right}`;
        for (const name of git?.goneAfter?.[key] ?? []) goneAfterPair.add(name);
        const fork = forkOf(key, left.commit, right.commit);
        if (fork === null) {
          const params = { left: body.left, right: body.right };
          return git?.shallow
            ? fail({ code: "no_merge_base_shallow", params, message: `${body.left} and ${body.right} share no commit in this shallow clone's history` })
            : fail({ code: "no_merge_base", params, message: `${body.left} and ${body.right} have no commit in common` });
        }
        // A right branch that is an ancestor of the left one forks at its own tip, which is no change.
        const changes = fork === right.commit ? [] : (git?.comparisons?.[key] ?? []);
        return { type: "project_comparison", project: body.project, left, right, base: fork, changes: changes.map((change) => change.entry), complete: !git?.cutShort };
      }
      case "read_project_comparison_change": {
        // Only the left side can be a remote-tracking branch or the empty tree.
        if (body.right.remote || body.right.empty) return fail({ code: "invalid_branch_name", params: { branch: body.right.branch }, message: `${body.right.branch} is not a valid branch name` });
        const changes = git?.comparisons?.[`${body.left.branch}..${body.right.branch}`] ?? [];
        const found = changes.find(({ entry }) => entry.group === "committed" && sameSide(entry.old, body.change.old) && sameSide(entry.new, body.change.new));
        if (!found || found.entry.group === "conflicted") return fail({ code: "file_not_found", params: { path: "" }, message: "no such change" });
        if (found.error) return fail(found.error);
        const whole = found.whole ?? {};
        const at = `${body.left.branch}..${body.right.branch}`;
        return {
          type: "project_comparison_change",
          project: body.project,
          left: body.left,
          right: body.right,
          base: body.base,
          old: readSide(at, found.entry.old, found.bodies?.old, whole.old, hasHunk(found)),
          new: readSide(at, found.entry.new, found.bodies?.new, whole.new, hasHunk(found)),
          patch: found.patch === undefined ? null : { size: found.patch.length, kind: "text", media_type: null, text: found.patch, data: null },
        };
      }
      case "read_project_change": {
        const worktree = worktreeOf(body.worktree);
        const changes = git?.changes[worktree];
        const found = Array.isArray(changes)
          ? changes.find(
            ({ entry }) =>
              entry.group === body.change.group && sameSide(entry.old, body.change.old) && sameSide(entry.new, body.change.new),
          )
          : undefined;
        if (!found || found.entry.group === "conflicted" || found.entry.group === "committed") {
          return fail({ code: "file_not_found", params: { path: "" }, message: "no such change" });
        }
        if (found.error) return fail(found.error);
        const whole = found.whole ?? {};
        const at = `${worktree} ${found.entry.group}`;
        return {
          type: "project_change",
          project: body.project,
          worktree: body.worktree ?? null,
          group: found.entry.group,
          head: null,
          old: readSide(at, found.entry.old, found.bodies?.old, whole.old, hasHunk(found)),
          new: readSide(at, found.entry.new, found.bodies?.new, whole.new, hasHunk(found)),
          patch: found.patch === undefined ? null : { size: found.patch.length, kind: "text", media_type: null, text: found.patch, data: null },
        };
      }
      case "list_project_commits": {
        const pair = pairAt(body.base, body.tip);
        const listed = pair === undefined ? undefined : git?.commits?.[pair];
        if (listed && "pending" in listed) return new Promise<Event>(() => {});
        if (!listed) {
          // A pair the fixture lists changes for has commits they are made of, which it must carry.
          if (pair !== undefined && (git?.comparisons?.[pair]?.length ?? 0) > 0) {
            refuseUnfillable(`${pair} commits: the comparison lists changes, and the fixture carries no commits to make them of`);
          }
          return { type: "project_commits", project: body.project, base: body.base, tip: body.tip, commits: [], complete: true, commits_complete: true };
        }
        if ("error" in listed) return fail(listed.error);
        return {
          type: "project_commits",
          project: body.project,
          base: body.base,
          tip: body.tip,
          commits: listed.commits.map(({ commit, parent, subject, changes }) => ({ commit, parent, subject, changes: changes.map(({ entry }) => entry) })),
          complete: !git?.cutShort,
          commits_complete: !listed.cut,
        };
      }
      case "read_project_commit_change": {
        const found = commitOf(body.commit, body.parent)?.changes.find(
          ({ entry }) => entry.group === "committed" && sameSide(entry.old, body.change.old) && sameSide(entry.new, body.change.new),
        );
        if (!found || found.entry.group === "conflicted") return fail({ code: "file_not_found", params: { path: "" }, message: "no such change" });
        if (found.error) return fail(found.error);
        const whole = found.whole ?? {};
        const at = `commit ${shortId(body.commit)}`;
        return {
          type: "project_commit_change",
          project: body.project,
          commit: body.commit,
          parent: body.parent,
          old: readSide(at, found.entry.old, found.bodies?.old, whole.old, hasHunk(found)),
          new: readSide(at, found.entry.new, found.bodies?.new, whole.new, hasHunk(found)),
          patch: found.patch === undefined ? null : { size: found.patch.length, kind: "text", media_type: null, text: found.patch, data: null },
        };
      }
      case "read_project_comparison_change_bodies":
        if (body.right.remote || body.right.empty) return fail({ code: "invalid_branch_name", params: { branch: body.right.branch }, message: `${body.right.branch} is not a valid branch name` });
        return readBodies(body);
      case "read_project_change_bodies":
      case "read_project_commit_change_bodies":
        return readBodies(body);
      default:
        return undefined;
    }
  };
}
