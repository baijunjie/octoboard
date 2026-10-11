import { expect, it } from "vitest";

import { hasHunks, hasTwoSides, joinedPatch, linesAfterLastHunk, withoutNoNewlineMarkers } from "./patch";

const REMOVED_FILE = "diff --git a/t b/t\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-x\n";
const ADDED_LINK = "diff --git a/t b/t\nnew file mode 120000\n@@ -0,0 +1 @@\n+y\n";
const NO_NEWLINE = "\\ No newline at end of file\n";
const same = (patch: string): [string, string, string] => [patch, patch, patch];

// A patch of two sections, one removing the old side and one adding the new, becomes one hunk of
// old against new whichever section git wrote first; any other shape is left as it is.
it.each<[string, string, string]>([
  ["a type change", REMOVED_FILE + ADDED_LINK, "diff --git a/t b/t\n@@ -1,1 +1,1 @@\n-x\n+y\n"],
  [
    "a removal at a path with an addition below it, the removal first",
    "diff --git a/f b/f\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-a\ndiff --git a/f/g b/f/g\nnew file mode 100644\n@@ -0,0 +1 @@\n+a\n",
    "diff --git a/f/g b/f/g\n@@ -1,1 +1,1 @@\n-a\n+a\n",
  ],
  [
    "a removal below a path with an addition at it, the addition first",
    "diff --git a/f b/f\nnew file mode 100644\n@@ -0,0 +1,2 @@\n+a\n+b\ndiff --git a/f/g b/f/g\ndeleted file mode 100644\n@@ -1,2 +0,0 @@\n-a\n-b\n",
    "diff --git a/f b/f\n@@ -1,2 +1,2 @@\n-a\n-b\n+a\n+b\n",
  ],
  [
    "a link written first, with several lines on the other side",
    ADDED_LINK + "diff --git a/t b/t\ndeleted file mode 100644\n@@ -1,2 +0,0 @@\n-a\n-b\n",
    "diff --git a/t b/t\n@@ -1,2 +1,1 @@\n-a\n-b\n+y\n",
  ],
  [
    "a file's no-newline marker kept and a link's dropped",
    `diff --git a/t b/t\ndeleted file mode 100644\n@@ -1,2 +0,0 @@\n-a\n-b\n${NO_NEWLINE}${ADDED_LINK}${NO_NEWLINE}`,
    `diff --git a/t b/t\n@@ -1,2 +1,1 @@\n-a\n-b\n${NO_NEWLINE}+y\n`,
  ],
  ["a side with no hunk", "diff --git a/t b/t\ndeleted file mode 100644\n" + ADDED_LINK, "diff --git a/t b/t\n@@ -0,0 +1,1 @@\n+y\n"],
  same("diff --git a/x b/x\n@@ -1 +1 @@\n-diff --git a\n+b\n"),
  same(REMOVED_FILE),
  same(REMOVED_FILE + REMOVED_FILE),
  same("diff --git a/t b/t\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-x\n@@ -5 +4,0 @@\n-z\n" + ADDED_LINK),
  same("diff --git a/t b/t\ndeleted file mode 100644\ndiff --git a/t b/t\nnew file mode 100644\n"),
])("joins %s", (_, patch, joined) => {
  expect(joinedPatch(patch)).toBe(joined);
});

it("offers the layout choice for a joined type change but not for an empty side", () => {
  const sides = (patch: string) => {
    const joined = joinedPatch(patch);
    return [hasHunks(joined), hasTwoSides(joined)];
  };
  expect(sides(REMOVED_FILE + ADDED_LINK)).toEqual([true, true]);
  expect(sides("diff --git a/t b/t\ndeleted file mode 100644\n" + ADDED_LINK)).toEqual([true, false]);
});

it("reports a joined patch's no-newline marker for the file's side only", () => {
  const file = `diff --git a/t b/t\ndeleted file mode 100644\n@@ -1 +0,0 @@\n-a\n${NO_NEWLINE}`;
  const link = `diff --git a/t b/t\nnew file mode 120000\n@@ -0,0 +1 @@\n+y\n${NO_NEWLINE}`;
  expect(withoutNoNewlineMarkers(joinedPatch(file + link))).toMatchObject({ old: true, new: false });
  expect(withoutNoNewlineMarkers(joinedPatch(link + file))).toMatchObject({ old: true, new: false });
});

it.each([
  ["after a removed line, the old side's", "-a\n\\ No newline at end of file\n+a\n", { old: true, new: false }],
  ["after an added line, the new side's", "-a\n+a\n\\ No newline at end of file\n", { old: false, new: true }],
  ["after a context line, both sides'", " a\n\\ No newline at end of file\n", { old: true, new: true }],
  ["in a symbolic link's patch, no side's", "new file mode 120000\n@@ -0,0 +1 @@\n+target\n\\ No newline at end of file\n", { old: false, new: false }],
])("takes git's no-newline marker out of a patch: %s", (_, hunk, marked) => {
  const result = withoutNoNewlineMarkers(`@@ -1 +1 @@\n${hunk}`);
  expect(result).toMatchObject(marked);
  expect(result.patch).not.toContain("No newline");
});

it.each([
  ["an added file", "@@ -0,0 +1,3 @@\n+a\n+b\n+c\n", true, false],
  ["a deleted file", "@@ -1,2 +0,0 @@\n-a\n-b\n", true, false],
  ["a modified file, even one only added to", "@@ -1,2 +1,3 @@\n a\n b\n+c\n", true, true],
  ["one-line sides, whose counts are left out", "@@ -1 +1 @@\n-a\n+b\n", true, true],
  ["an empty file added, with no hunk", "diff --git a/x b/x\nnew file mode 100644\nindex 0000000..e69de29\n", false, false],
])("tells the lines and sides of %s", (_, patch, lines, sides) => {
  expect([hasHunks(patch), hasTwoSides(patch)]).toEqual([lines, sides]);
});

// The unmodified lines after the last hunk are the same on both sides, so one side's count settles
// them; a hunk with no lines on a side writes its start as the line before it.
const patchOf = (hunks: string) => `diff --git a/t b/t\n${hunks}`;
it.each<[string, string, { old?: number | null; new?: number | null }, number | undefined]>([
  ["the old side counted", patchOf("@@ -3,2 +3,2 @@\n-a\n-b\n+c\n+d\n"), { old: 10, new: null }, 6],
  ["the new side counted", patchOf("@@ -3,2 +3,3 @@\n-a\n-b\n+c\n+d\n+e\n"), { old: null, new: 10 }, 5],
  ["both sides counted and agreeing", patchOf("@@ -3,2 +3,3 @@\n-a\n-b\n+c\n+d\n+e\n"), { old: 9, new: 10 }, 5],
  ["both sides counted and disagreeing", patchOf("@@ -3,2 +3,3 @@\n-a\n-b\n+c\n+d\n+e\n"), { old: 9, new: 12 }, undefined],
  ["no side counted", patchOf("@@ -1 +1 @@\n-a\n+b\n"), { old: null }, undefined],
  ["a hunk running to the end", patchOf("@@ -1,3 +1,3 @@\n-a\n+b\n c\n d\n"), { old: 3 }, 0],
  ["a hunk with no old lines", patchOf("@@ -0,0 +1,2 @@\n+a\n+b\n"), { old: 0, new: 2 }, 0],
  ["a hunk with no new lines", patchOf("@@ -4,2 +3,0 @@\n-a\n-b\n"), { new: 8 }, 5],
  ["a count smaller than the hunk reaches", patchOf("@@ -3,2 +3,2 @@\n-a\n-b\n+c\n+d\n"), { old: 2 }, undefined],
  ["only the last hunk", patchOf("@@ -1 +1 @@\n-a\n+b\n@@ -8,2 +8,2 @@\n-c\n-d\n+e\n+f\n"), { old: 12 }, 3],
  ["a patch with more than one section", REMOVED_FILE + ADDED_LINK, { old: 1, new: 1 }, undefined],
  ["a patch with no hunk", patchOf("rename from a\nrename to b\n"), { old: 1 }, undefined],
])("counts the lines after the last hunk: %s", (_, patch, totals, expected) => {
  expect(linesAfterLastHunk(patch, totals)).toBe(expected);
});
