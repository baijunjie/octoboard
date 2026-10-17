// The vocabulary a repository's change fixtures are written in, for the fixture daemon's Git-mode
// replies (`fixtureGit.ts`): what a fixture repository and one of its changes hold, a factory per
// kind of change, and the generator the long files a diff collapses lines of are written with.
//
// A sample change is always built by the factory for its kind, never written out as a literal — a
// test staging a deliberately broken fixture being the one exception. Each factory takes exactly
// what the real daemon's reply for that kind carries and derives the rest, so a change that could
// not be answered faithfully cannot be written at all. That is the half of the net the types can
// hold; the fixture daemon refuses at run time what no signature can settle.
import type { BranchInfo, ChangeEntry, ChangeSide, ConflictKind, ContentSource, FileContent, WorktreeInfo } from "../../protocol";
import type { FixtureError } from "./projectFiles";
import { text } from "./builders";

/** One change of a fixture worktree: the entry its list carries, and what reading it answers — its
 * patch, its sides' bodies and the whole text each side holds, or an error. */
export interface FixtureChange {
  entry: ChangeEntry;
  patch?: string;
  /** The body the reply carries for a side, where no patch shows its content. */
  bodies?: { old?: FileContent; new?: FileContent };
  /** The error reading the change answers with, in place of a patch. */
  error?: FixtureError;
  /** The whole text of each side: what the diff reply counts the side's lines from, and what a
   * request for the bodies answers with. One source for both, so the two cannot disagree. */
  whole?: { old?: string; new?: string };
  /** How a request for the whole bodies is answered where it is not answered straight away, as
   * `Expansion` below sets out. Whether the change has bodies to read at all is not from here —
   * that follows from the change's own sides, as it does in the daemon. */
  expand?: Expansion;
}

/** A fixture project's repository: its worktrees, the first holding the project, and each one's
 * changes by its id; a worktree with an `error` in place of changes fails to list. With `gitError`,
 * the repository cannot be read at all. `branches` are its local branches; `comparisons` the
 * changes `<right>` made since it forked from `<left>`, keyed `<left>..<right>` (none for a pair not
 * given), whose old sides are read from the fork's commit; `forks` names that commit for a pair
 * (`null` for two branches with no commit in common, which a comparison refuses, as a shallow
 * clone's refusal when `shallow` is set), and a pair it does
 * not name forks at `<left>`'s own tip, as a branch made from it does. `missing` are branches the
 * list has that a comparison no longer finds. `moved` is what the branch list says of a branch once
 * a comparison has been made: a branch moved to another commit since. `gone` are branches deleted
 * once a comparison has been made: from then on neither the list nor a new comparison finds them,
 * where `missing` ones are refused from the start. */
export interface FixtureGit {
  worktrees: WorktreeInfo[];
  changes: Record<string, FixtureChange[] | FixtureError>;
  gitError?: string;
  branches?: BranchInfo[];
  comparisons?: Record<string, FixtureChange[]>;
  forks?: Record<string, string | null>;
  shallow?: boolean;
  missing?: string[];
  moved?: Record<string, string>;
  gone?: string[];
  /** The daemon stopped listing before it reached the end, so every change list and comparison
   * is answered `complete: false` and the pane says under the list that it is not whole. */
  cutShort?: boolean;
}

/** Which section of a change list a change with two sides belongs to; `committed` is a branch
 * comparison's. An untracked change has no old side, so it has a factory of its own instead. */
export type TwoSided = "staged" | "unstaged" | "committed";

/** What both sides of a change hold and the patch between them, which a factory derives the reply's
 * line counts and bodies from. */
export interface Versions {
  old: string;
  new: string;
  patch: string;
}

/** A `Versions` `editedSource` generated, which knows the path its patch names, so a factory can
 * write the same change at another path. */
export interface GeneratedSource extends Versions {
  path: string;
}

/** How a request for a change's whole bodies is answered, after `delay` milliseconds. The daemon
 * can refuse it at either of two moments, which the fixture keeps apart: `movedOn` says the side
 * the request names is not the one the daemon finds any more, which it answers `source_changed`
 * for before it reads anything at all; `error` is what reading a side's body then meets. The diff
 * reply counted the versions whichever way this goes. */
export interface Expansion {
  delay?: number;
  movedOn?: boolean;
  error?: FixtureError;
}

/** A side the daemon read a stored object of, which is the only kind of side whose id a patch's
 * `index` line can be written from: a side read from the worktree disk has no id in these fixtures,
 * though `git` hashes one for it. */
type ObjectSource = Extract<ContentSource, { blob: string }>;

const sideFile = (path: string, source: ContentSource): ChangeSide => ({ state: "present", path, kind: "file", source });
/** A hunk's range over the first `count` lines of a file, as `git` writes it: a single line alone. */
const rangeOf = (count: number) => (count === 1 ? "1" : `1,${count}`);
/** A text's lines, without the empty one its terminating newline leaves after it. */
const linesOf = (content: string) => content.replace(/\n$/, "").split("\n");

/** The addition of a whole file as `git` writes it, from `--- /dev/null` down, the mirror of
 * `removalOf`: nothing at all for an empty file, which has no line to add, and the marker after the
 * last line where the file does not end in a newline. */
function additionOf(path: string, content: string): string[] {
  if (content === "") return [];
  const lines = linesOf(content);
  return [
    "--- /dev/null",
    `+++ b/${path}`,
    `@@ -0,0 +${rangeOf(lines.length)} @@`,
    ...lines.map((line) => `+${line}`),
    ...(content.endsWith("\n") ? [] : ["\\ No newline at end of file"]),
  ];
}

/** The removal of a whole file as `git` writes it, from `--- a/<path>` down: nothing at all for an
 * empty file, which has no line to remove, and the marker after the last line where the file does
 * not end in a newline. */
function removalOf(path: string, content: string): string[] {
  if (content === "") return [];
  const lines = linesOf(content);
  return [
    `--- a/${path}`,
    "+++ /dev/null",
    `@@ -${rangeOf(lines.length)} +0,0 @@`,
    ...lines.map((line) => `-${line}`),
    ...(content.endsWith("\n") ? [] : ["\\ No newline at end of file"]),
  ];
}

/** A change of a text file: the patch shows its content, so the reply counts both sides from
 * `versions` and a request for the bodies answers with them. */
export function textChange(group: TwoSided, path: string, from: ContentSource, to: ContentSource, versions: Versions, expand?: Expansion): FixtureChange {
  return renamedTo(group, path, path, from, to, versions, expand);
}

/** A file renamed, its content changed with it: as a text change, with a path on each side. */
export function renamedTo(
  group: TwoSided,
  from: string,
  to: string,
  fromSource: ContentSource,
  toSource: ContentSource,
  versions: Versions,
  expand?: Expansion,
): FixtureChange {
  return {
    entry: { group, old: sideFile(from, fromSource), new: sideFile(to, toSource) },
    patch: versions.patch,
    whole: { old: versions.old, new: versions.new },
    expand,
  };
}

/** What a request for the bodies of a change with no diff answers: the one content both its sides
 * hold, after `delay`, or the error reading it meets instead — which is the only thing left to say
 * the change altered nothing, since its patch shows nothing either way. A patch with no hunk counts
 * neither side, so the content is for that request alone. */
export type UnchangedRead = { content: string; delay?: number; error?: never } | { error: FixtureError; content?: never; delay?: never };

const answering = (read: UnchangedRead) =>
  "error" in read ? { expand: { error: read.error } } : { whole: { old: read.content, new: read.content }, expand: { delay: read.delay } };

/** A change whose two sides hold the same content at the same path, which `git` compares as an
 * empty patch: there is no diff to draw, and the content is read as a diff's collapsed lines are. */
export function unchanged(group: TwoSided, path: string, from: ContentSource, to: ContentSource, read: UnchangedRead): FixtureChange {
  return { entry: { group, old: sideFile(path, from), new: sideFile(path, to) }, patch: "", ...answering(read) };
}

/** A rename into or out of a path below itself whose content came through unchanged. The daemon
 * diffs such a pair between the two versions it names, and `git` writes nothing at all for two
 * operands of equal content, so the patch is empty rather than a rename's headers. */
export function nestedUnchanged(
  group: TwoSided,
  from: string,
  to: string,
  fromSource: ContentSource,
  toSource: ContentSource,
  read: UnchangedRead,
): FixtureChange {
  return { entry: { group, old: sideFile(from, fromSource), new: sideFile(to, toSource) }, patch: "", ...answering(read) };
}

/** A plain `git mv`: rename detection writes the rename's headers and no hunk, so there is no diff
 * here either. */
export function renamedUnchanged(
  group: TwoSided,
  from: string,
  to: string,
  fromSource: ContentSource,
  toSource: ContentSource,
  read: UnchangedRead,
): FixtureChange {
  return {
    entry: { group, old: sideFile(from, fromSource), new: sideFile(to, toSource) },
    patch: [`diff --git a/${from} b/${to}`, "similarity index 100%", `rename from ${from}`, `rename to ${to}`, ""].join("\n"),
    ...answering(read),
  };
}

/** A side of a change whose patch names the id `git` hashed it to: a stored object is its own id,
 * and a side read from the worktree disk has one `git` hashed for it which these fixtures do not
 * otherwise hold, so it is given with the source (`hashedOnDisk`). */
type HashedSide = ObjectSource | { source: ContentSource; hashed: string };
export const hashedOnDisk = (source: ContentSource, hashed: string) => ({ source, hashed });
const sourceOf = (side: HashedSide): ContentSource => ("source" in side ? side.source : side);
const hashOf = (side: HashedSide): string => ("source" in side ? side.hashed : side.blob);

/** A change `git` treats as binary: its patch is the one line saying the files differ, so the reply
 * carries both bodies instead and counts each side's lines from its own. A request for the whole
 * bodies answers with those same bodies: the daemon reads every present file side there whatever
 * the change's kind, and these are the whole of each side. */
export function binaryChange(group: TwoSided, path: string, from: HashedSide, to: HashedSide, bodies: { old: FileContent; new: FileContent }): FixtureChange {
  return {
    entry: { group, old: sideFile(path, sourceOf(from)), new: sideFile(path, sourceOf(to)) },
    patch: [`diff --git a/${path} b/${path}`, `index ${hashOf(from)}..${hashOf(to)} 100644`, `Binary files a/${path} and b/${path} differ`, ""].join("\n"),
    bodies,
  };
}

/** A file that became a symbolic link: `git` writes the file's removal and the link's addition as
 * two sections, which the viewer joins into one diff with nothing collapsed, so there is nothing to
 * expand. Only the file side is counted, from `content`; a link's lines are never counted. A request
 * for the whole bodies is refused outright, the link having no body to read. */
export function typeChange(group: TwoSided, path: string, from: ObjectSource, to: ObjectSource, content: string, target: string): FixtureChange {
  return {
    entry: { group, old: sideFile(path, from), new: { state: "present", path, kind: "symlink", source: to } },
    patch: [
      `diff --git a/${path} b/${path}`,
      "deleted file mode 100644",
      `index ${from.blob}..0000000`,
      ...removalOf(path, content),
      `diff --git a/${path} b/${path}`,
      "new file mode 120000",
      `index 0000000..${to.blob}`,
      // A link's target is one line and never ends in a newline, which `additionOf` marks.
      ...additionOf(path, target),
      "",
    ].join("\n"),
    whole: { old: content },
  };
}

/** A file removed: `git` writes every line of it as a deletion. Only the old side exists, so it
 * alone is counted, from `content`, and a request for the whole bodies answers with that content
 * and an absent new side — the daemon reads the side that is there and leaves the other absent. */
export function deletion(group: TwoSided, path: string, from: ObjectSource, content: string): FixtureChange {
  return {
    entry: { group, old: sideFile(path, from), new: { state: "absent" } },
    patch: [`diff --git a/${path} b/${path}`, "deleted file mode 100644", `index ${from.blob}..0000000`, ...removalOf(path, content), ""].join("\n"),
    whole: { old: content },
  };
}

/** A file added: `git` writes every line of it as an addition. Only the new side exists, so it alone is
 * counted, from `content`, and a request for the whole bodies answers with that content and an
 * absent old side, as for a removal. Not an untracked file, which Git has no patch for. */
export function addition(group: TwoSided, path: string, to: ObjectSource, content: string): FixtureChange {
  return {
    entry: { group, old: { state: "absent" }, new: sideFile(path, to) },
    patch: [`diff --git a/${path} b/${path}`, "new file mode 100644", `index 0000000..${to.blob}`, ...additionOf(path, content), ""].join("\n"),
    whole: { new: content },
  };
}

/** A file Git neither tracks nor ignores: there is nothing to compare it with, so the reply carries
 * its body in place of a patch and counts its lines from that body. Having no patch, it has no
 * collapsed lines either, so a request for its whole bodies is refused. */
export function untracked(path: string, source: ContentSource, content: string): FixtureChange {
  return { entry: { group: "untracked", old: { state: "absent" }, new: sideFile(path, source) }, bodies: { new: text(content) } };
}

/** A symbolic link, or a repository of its own inside the project, that Git neither tracks nor
 * ignores. Like an untracked file it has nothing to compare with, so the reply carries no patch;
 * unlike one it carries no body either, the daemon reading no content for a link or a nested
 * repository, and counts none of its lines. A request for its whole bodies is refused as an untracked
 * file's is. */
export function untrackedNotFile(path: string, kind: "symlink" | "submodule", source: ContentSource): FixtureChange {
  return { entry: { group: "untracked", old: { state: "absent" }, new: { state: "present", path, kind, source } } };
}

/** A change one side of which lies outside the project's scope: the daemon delivers neither that
 * side nor a patch across the boundary, so the reply carries the in-scope side's body alone. Only a
 * staged or a committed change can have such a side; unstaged, the daemon refuses the read. With no
 * patch, a request for the whole bodies is refused as well. */
export function fromOutsideTheProject(
  group: "staged" | "committed",
  repositoryPath: string,
  path: string,
  source: ContentSource,
  content: string,
): FixtureChange {
  return {
    entry: { group, old: { state: "out_of_scope", repository_path: repositoryPath }, new: sideFile(path, source) },
    bodies: { new: text(content) },
  };
}

/** A path in conflict, which is no two-sided change: it is read as the file on disk. */
export function conflicted(path: string, conflict: ConflictKind): FixtureChange {
  return { entry: { group: "conflicted", path, conflict } };
}

/** A change the daemon refuses to read at all — its patch past a limit, say: the list has it, and
 * opening it answers with `error`. Where `error` is what making the patch raised — a patch past the
 * budget, a change `git` could not read — a request for the whole bodies answers with it too, those
 * being resolved by making the patch first. */
export function refused(group: TwoSided, path: string, from: ContentSource, to: ContentSource, error: FixtureError): FixtureChange {
  return { entry: { group, old: sideFile(path, from), new: sideFile(path, to) }, error };
}

/** The `index` line `editedSource` writes, which the factories that place a generated source at a
 * path replace with the one naming the two sides they are given: a generator knows no object id. */
const GENERATED_INDEX = "index 0000000..0000000 100644";
const indexOf = (from: ObjectSource, to: ObjectSource) => `index ${from.blob}..${to.blob} 100644`;

/** A source file of `count` numbered lines before and after the lines `edits` names (1-based) are
 * rewritten, and the unified patch between them as `git diff` writes it with three lines of context:
 * edits at most seven lines apart share a hunk, and the lines between hunks are what the viewer
 * collapses. */
export function editedSource(path: string, count: number, edits: number[], tail = ""): GeneratedSource {
  const before = Array.from({ length: count }, (_, i) => `  stages.push(stage${i + 1}(context)); // step ${i + 1}${tail}`);
  const after = before.map((line, i) => (edits.includes(i + 1) ? `  stages.push(await stage${i + 1}(context)); // step ${i + 1}, awaited${tail}` : line));
  const groups: number[][] = [];
  for (const edit of edits) {
    const last = groups[groups.length - 1];
    if (last && edit - last[last.length - 1] <= 7) last.push(edit);
    else groups.push([edit]);
  }
  const patch = [`diff --git a/${path} b/${path}`, GENERATED_INDEX, `--- a/${path}`, `+++ b/${path}`];
  for (const group of groups) {
    const first = Math.max(group[0] - 3, 1);
    const last = Math.min(group[group.length - 1] + 3, count);
    patch.push(`@@ -${first},${last - first + 1} +${first},${last - first + 1} @@`);
    for (let n = first; n <= last; n++) {
      if (edits.includes(n)) patch.push(`-${before[n - 1]}`, `+${after[n - 1]}`);
      else patch.push(` ${before[n - 1]}`);
    }
  }
  return { path, old: `${before.join("\n")}\n`, new: `${after.join("\n")}\n`, patch: `${patch.join("\n")}\n` };
}

/** How long reading the bodies of an expandable change takes, long enough to see it loading. */
export const EXPAND_DELAY_MS = 600;

/** A change of a long file whose diff can be expanded, with `expand` where reading its bodies ends
 * in a refusal instead of an answer (the patch's own counts are taken from `source` either way);
 * `source` is written at `path` wherever it was generated for another. */
export function expandable(
  group: TwoSided,
  path: string,
  from: ObjectSource,
  to: ObjectSource,
  source: GeneratedSource,
  expand: Omit<Expansion, "delay"> = {},
): FixtureChange {
  const patch = source.patch.replaceAll(source.path, path).replace(GENERATED_INDEX, indexOf(from, to));
  return textChange(group, path, from, to, { ...source, patch }, { delay: EXPAND_DELAY_MS, ...expand });
}

/** A file renamed into or out of a path below itself, its content changed with it: one section, the
 * diff between the two versions the daemon names. It carries no rename header, since `git` detects
 * no rename between two operands it was handed. */
export function nested(
  group: TwoSided,
  from: string,
  to: string,
  fromSource: ObjectSource,
  toSource: ObjectSource,
  source: GeneratedSource,
): FixtureChange {
  const patch = source.patch
    .replace(`diff --git a/${source.path} b/${source.path}`, `diff --git a/${from} b/${to}`)
    .replace(GENERATED_INDEX, indexOf(fromSource, toSource))
    .replace(`--- a/${source.path}`, `--- a/${from}`)
    .replace(`+++ b/${source.path}`, `+++ b/${to}`);
  return renamedTo(group, from, to, fromSource, toSource, { ...source, patch }, { delay: EXPAND_DELAY_MS });
}
