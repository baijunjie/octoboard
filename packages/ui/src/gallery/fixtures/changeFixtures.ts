// The vocabulary a repository's change fixtures are written in, for the fixture daemon's Git-mode
// replies (`fixtureDaemon.ts`): what a fixture repository and one of its changes hold, a factory per
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
  /** How a request for the whole bodies is answered, where the change has bodies to read at all;
   * absent where it has none. */
  expand?: Expansion;
}

/** A fixture project's repository: its worktrees, the first holding the project, and each one's
 * changes by its id; a worktree with an `error` in place of changes fails to list. With `gitError`,
 * the repository cannot be read at all. `branches` are its local branches; `comparisons` the
 * changes between two of them, keyed `<left>..<right>` (none for a pair not given), and `missing`
 * branches the list has that a comparison no longer finds. `moved` is what the branch list says of
 * a branch once a comparison has been made: a branch moved to another commit since. */
export interface FixtureGit {
  worktrees: WorktreeInfo[];
  changes: Record<string, FixtureChange[] | FixtureError>;
  gitError?: string;
  branches?: BranchInfo[];
  comparisons?: Record<string, FixtureChange[]>;
  missing?: string[];
  moved?: Record<string, string>;
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

/** How a request for a change's whole bodies is answered: after `delay` milliseconds, or with
 * `error` in place of the bodies — the diff reply counted the versions all the same. */
export interface Expansion {
  delay?: number;
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
export function textChange(group: TwoSided, path: string, from: ContentSource, to: ContentSource, versions: Versions, expand: Expansion = {}): FixtureChange {
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
  expand: Expansion = {},
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
 * carries both bodies instead and counts each side's lines from its own. */
export function binaryChange(group: TwoSided, path: string, from: HashedSide, to: HashedSide, bodies: { old: FileContent; new: FileContent }): FixtureChange {
  return {
    entry: { group, old: sideFile(path, sourceOf(from)), new: sideFile(path, sourceOf(to)) },
    patch: [`diff --git a/${path} b/${path}`, `index ${hashOf(from)}..${hashOf(to)} 100644`, `Binary files a/${path} and b/${path} differ`, ""].join("\n"),
    bodies,
  };
}

/** A file that became a symbolic link: `git` writes the file's removal and the link's addition as
 * two sections, which the viewer joins into one diff with nothing collapsed, so there is nothing to
 * expand. Only the file side is counted, from `content`; a link's lines are never counted. */
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
      "--- /dev/null",
      `+++ b/${path}`,
      "@@ -0,0 +1 @@",
      // A link's target is one line and never ends in a newline, which `git` marks.
      `+${target}`,
      "\\ No newline at end of file",
      "",
    ].join("\n"),
    whole: { old: content },
  };
}

/** A file removed: `git` writes every line of it as a deletion. The new side does not exist, so
 * there are no bodies to read, and only the old side is counted, from `content`. */
export function deletion(group: TwoSided, path: string, from: ObjectSource, content: string): FixtureChange {
  return {
    entry: { group, old: sideFile(path, from), new: { state: "absent" } },
    patch: [`diff --git a/${path} b/${path}`, "deleted file mode 100644", `index ${from.blob}..0000000`, ...removalOf(path, content), ""].join("\n"),
    whole: { old: content },
  };
}

/** A file Git neither tracks nor ignores: there is nothing to compare it with, so the reply carries
 * its body in place of a patch and counts its lines from that body. */
export function untracked(path: string, source: ContentSource, content: string): FixtureChange {
  return { entry: { group: "untracked", old: { state: "absent" }, new: sideFile(path, source) }, bodies: { new: text(content) } };
}

/** A change one side of which lies outside the project's scope: the daemon delivers neither that
 * side nor a patch across the boundary, so the reply carries the in-scope side's body alone. Only a
 * staged or a committed change can have such a side; unstaged, the daemon refuses the read. */
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
 * opening it answers with `error`. */
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

/** A change of a long file whose diff can be expanded, with `error` where reading its bodies fails
 * instead of answering (the patch's own counts are taken from `source` either way); `source` is
 * written at `path` wherever it was generated for another. */
export function expandable(group: TwoSided, path: string, from: ObjectSource, to: ObjectSource, source: GeneratedSource, error?: FixtureError): FixtureChange {
  const patch = source.patch.replaceAll(source.path, path).replace(GENERATED_INDEX, indexOf(from, to));
  return textChange(group, path, from, to, { ...source, patch }, { delay: EXPAND_DELAY_MS, ...(error && { error }) });
}

/** A file renamed into or out of a path below itself, its content changed with it: one section, the
 * diff between its two versions under the rename's headers, which `git` writes above the `index`
 * line whenever it detected a rename. `similarity` is the percentage it put on the rename. */
export function nested(
  group: TwoSided,
  from: string,
  to: string,
  fromSource: ObjectSource,
  toSource: ObjectSource,
  source: GeneratedSource,
  similarity: number,
): FixtureChange {
  const patch = source.patch
    .replace(`diff --git a/${source.path} b/${source.path}`, `diff --git a/${from} b/${to}`)
    .replace(GENERATED_INDEX, [`similarity index ${similarity}%`, `rename from ${from}`, `rename to ${to}`, indexOf(fromSource, toSource)].join("\n"))
    .replace(`--- a/${source.path}`, `--- a/${from}`)
    .replace(`+++ b/${source.path}`, `+++ b/${to}`);
  return renamedTo(group, from, to, fromSource, toSource, { ...source, patch }, { delay: EXPAND_DELAY_MS });
}
