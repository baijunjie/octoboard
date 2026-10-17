// What the fixture daemon answers a project pane's Git mode with, over a fixture repository
// (`fixtures/changeFixtures.ts`): the source, the change lists, a change's patch and the whole
// bodies behind it, the branch list and a comparison of one branch with another (what it has changed
// since it forked from it).
//
// These are the replies that have to track the real daemon's own
// (`apps/daemon/src/browse/changes.rs`), condition for condition, and they are kept apart from the
// rest of the fixture daemon for that reason. Where the daemon would fill a field and the fixture
// carries nothing to fill it from, the reply is refused rather than sent unfilled; where a
// signature could have settled it instead, the factory that builds the change does.
import type { ChangeSide, ContentSource, Event, FileContent, RequestBody, SideRead, SideRef, SideVersion } from "../protocol";
import { refuseUnfillable } from "./banner";
import { text } from "./fixtures/builders";
import type { FixtureGit } from "./fixtures/changeFixtures";
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
  /** Whether a comparison has been made, after which the branch list tells of the branches moved or gone. */
  let compared = false;
  /** The daemon's refusal of a change whose patch has no lines to expand. */
  const noBodies = (): never => fail({ code: "invalid_change", params: {}, message: "no patch to expand" });
  /** The worktree a request is answered from: the one it names, else the project's own, as the
   * daemon resolves an omitted one. */
  const worktreeOf = (named: string | undefined) => named ?? git?.worktrees[0]?.id ?? "";
  /** The whole bodies behind a change's patch, for expanding the lines it collapsed. */
  const readBodies = async (body: Extract<RequestBody, { type: "read_project_change_bodies" | "read_project_comparison_change_bodies" }>): Promise<Event> => {
    const asked = body.type === "read_project_change_bodies";
    const at = asked ? `${worktreeOf(body.worktree)} ${body.change.group} bodies` : `${body.left.branch}..${body.right.branch} bodies`;
    const changes = asked
      ? git?.changes[worktreeOf(body.worktree)]
      : git?.comparisons?.[`${body.left.branch}..${body.right.branch}`];
    const found = Array.isArray(changes)
      ? changes.find(
        ({ entry }) =>
          entry.group !== "conflicted" &&
            (!asked || entry.group === body.change.group) &&
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
    return asked
      ? { type: "project_change_bodies", project: body.project, worktree: body.worktree ?? null, group: body.change.group, ...sides }
      : { type: "project_comparison_change_bodies", project: body.project, left: body.left, right: body.right, base: body.base, ...sides };
  };

  /** The reply to a Git-mode request, or `undefined` where the request is not one of them. */
  return (body: RequestBody): Event | Promise<Event> | undefined => {
    switch (body.type) {
      case "get_project_source": {
        const own = git?.worktrees[0];
        return {
          type: "project_source",
          source: {
            project: body.project,
            root: "/Users/dev/code/search-api",
            resolved_root: "/Users/dev/code/search-api",
            root_id: "fixture",
            git:
              git && !git.gitError && own
                ? { repository: "repo", common_dir: "/Users/dev/code/search-api/.git", worktree: own.id, scope: "", worktrees: git.worktrees }
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
        const moved = compared ? (git?.moved ?? {}) : {};
        const gone = compared ? (git?.gone ?? []) : [];
        const branches = (git?.branches ?? [])
          .filter((branch) => !gone.includes(branch.name))
          .map((branch) => ({ ...branch, commit: moved[branch.name] ?? branch.commit }));
        return { type: "project_branches", project: body.project, branches, complete: true };
      }
      case "compare_project_branches": {
        const resolve = (name: string) => {
          const branch = git?.branches?.find((b) => b.name === name);
          if (!branch || git?.missing?.includes(name) || (compared && git?.gone?.includes(name))) return fail({ code: "unknown_branch", params: { branch: name }, message: `there is no local branch ${name}` });
          return { branch: name, commit: branch.commit };
        };
        const left = resolve(body.left);
        const right = resolve(body.right);
        compared = true;
        const key = `${body.left}..${body.right}`;
        const fork = left.commit === right.commit ? left.commit : git?.forks?.[key] === undefined ? left.commit : git.forks[key];
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
      case "read_project_change_bodies":
      case "read_project_comparison_change_bodies":
        return readBodies(body);
      default:
        return undefined;
    }
  };
}
