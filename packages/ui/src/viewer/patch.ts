// Reading and rewriting the unified patch text a change is drawn from. Pure text work: nothing here
// names a rendering library or a daemon type.

/** A patch header line that says the file is a symbolic link (mode 120000). */
const LINK_MODE = /^((new|deleted) file mode|old mode|new mode) 120000$|^index \S+ 120000$/m;

/** `patch` without git's "\\ No newline at end of file" lines, and which side they marked: the
 * old one after a removed line, the new one after an added line, both after a context line. Each
 * hunk stays whole without them: the marker is no line of either side. A symbolic link's patch
 * marks nothing: a link's target never ends in a newline, so saying so would say nothing. */
export function withoutNoNewlineMarkers(patch: string): { patch: string; old: boolean; new: boolean } {
  const lines = patch.split("\n");
  const kept: string[] = [];
  const marked = { old: false, new: false };
  for (const line of lines) {
    if (line.startsWith("\\ ")) {
      const previous = kept[kept.length - 1] ?? "";
      if (previous.startsWith("-")) marked.old = true;
      else if (previous.startsWith("+")) marked.new = true;
      else if (previous.startsWith(" ")) marked.old = marked.new = true;
      continue;
    }
    kept.push(line);
  }
  if (!marked.old && !marked.new) return { patch, ...marked };
  const link = LINK_MODE.test(patch);
  return { patch: kept.join("\n"), old: marked.old && !link, new: marked.new && !link };
}

/** Where each hunk of a patch starts and how many lines it holds, on the old side and the new. */
function hunks(patch: string): { oldStart: number; oldCount: number; newStart: number; newCount: number }[] {
  return [...patch.matchAll(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/gm)].map(([, oldStart, oldCount, newStart, newCount]) => ({
    oldStart: Number(oldStart),
    oldCount: Number(oldCount ?? 1),
    newStart: Number(newStart),
    newCount: Number(newCount ?? 1),
  }));
}

/** The line counts of each hunk of a patch, old side then new. */
function hunkCounts(patch: string): [number, number][] {
  return hunks(patch).map(({ oldCount, newCount }) => [oldCount, newCount]);
}

/** Whether a patch has any line to draw: one with no hunk — an empty file added or removed, a mode
 * change — has none, and the viewer says so rather than draw an empty diff. */
export function hasHunks(patch: string): boolean {
  return hunkCounts(patch).length > 0;
}

/** Whether a patch says nothing changed at all: it has no hunk, no binary line (`git` writes a
 * file it treats as binary, whatever its content, as one `Binary files … differ` line, which says
 * the content differs) and no mode change. What is left is empty or a rename's header. */
export function saysNothingChanged(patch: string): boolean {
  return !hasHunks(patch) && !/^(Binary files .* differ|GIT binary patch|old mode |new mode )/m.test(patch);
}

/** Whether a patch has two sides to set side by side: some hunk has lines on both. One where every
 * hunk adds to nothing or removes everything — an added or a deleted file — reads the same unified
 * and split, so the viewer offers no choice between them. */
export function hasTwoSides(patch: string): boolean {
  return hunkCounts(patch).some(([old, next]) => old > 0 && next > 0);
}

/** A patch split into its sections, one per `diff --git` header, each a patch of its own. No line
 * of a hunk can start that way: each starts with a space, `+`, `-` or `\`. */
function patchSections(patch: string): string[] {
  const starts = [...patch.matchAll(/^diff --git /gm)].map((match) => match.index);
  if (starts.length < 2) return [patch];
  return starts.map((start, i) => patch.slice(start, starts[i + 1]));
}

/**
 * A change's patch as one patch. `git` writes a type change (a file become a symbolic link, or the
 * reverse), including one whose two paths nest, as two sections: one removing the old side and one
 * adding the new, in the order of their paths rather than of their roles. (A rename into or out of
 * a path below itself that keeps its kind is one section, and passes through untouched.) Drawn apart
 * they would lack the layout choice every other change has, so they are joined into one hunk of
 * the old side's lines against the new side's. The hunk holds both sides whole, so nothing in it
 * is collapsed, and no line diff is computed here: the removed lines are followed by the added
 * ones, as for a file replaced wholesale. A patch of any other shape — not exactly one removal
 * and one addition, each with at most one hunk — is returned as it is.
 */
export function joinedPatch(patch: string): string {
  const sections = patchSections(patch);
  if (sections.length !== 2) return patch;
  const removalAt = sections.findIndex((section) => /^deleted file mode /m.test(section));
  const additionAt = sections.findIndex((section) => /^new file mode /m.test(section));
  if (removalAt === -1 || additionAt === -1 || removalAt === additionAt) return patch;
  const sides = sections.map((section) => {
    const at = section.search(/^@@ /m);
    if (at === -1) return { count: [0, 0], lines: "" };
    const hunk = section.slice(at);
    const counts = hunkCounts(hunk);
    if (counts.length !== 1) return undefined;
    // A symbolic link's target never ends in a newline, so its marker would say nothing.
    const body = LINK_MODE.test(section) ? hunk.replace(/^\\ .*\n?/gm, "") : hunk;
    return { count: counts[0], lines: body.slice(body.indexOf("\n") + 1) };
  });
  const [removal, addition] = [sides[removalAt], sides[additionAt]];
  if (!removal || !addition) return patch;
  const [old, next] = [removal.count[0], addition.count[1]];
  if (old + next === 0) return patch;
  const header = sections[additionAt].slice(0, sections[additionAt].indexOf("\n") + 1);
  return `${header}@@ -${old === 0 ? 0 : 1},${old} +${next === 0 ? 0 : 1},${next} @@\n${removal.lines}${addition.lines}`;
}

/**
 * How many unmodified lines follow a patch's last hunk, given how many lines each side holds where
 * that is known. The lines after the last hunk are the same on both sides, so one side's count
 * settles it; `undefined` when no count is known, when the two counts disagree, or for a patch of
 * more than one section (a type change), which has no single end.
 */
export function linesAfterLastHunk(patch: string, totals: { old?: number | null; new?: number | null }): number | undefined {
  if (patchSections(patch).length !== 1) return undefined;
  const last = hunks(patch).at(-1);
  if (!last) return undefined;
  // A hunk with no lines on a side writes its start as the line before it, or 0.
  const consumed = (start: number, count: number) => (count > 0 ? start + count - 1 : start);
  const after = [
    totals.old == null ? undefined : totals.old - consumed(last.oldStart, last.oldCount),
    totals.new == null ? undefined : totals.new - consumed(last.newStart, last.newCount),
  ].filter((count) => count !== undefined);
  if (after.length === 0 || after.some((count) => count < 0 || count !== after[0])) return undefined;
  return after[0];
}
