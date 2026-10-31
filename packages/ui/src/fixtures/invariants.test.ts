import { describe, expect, it } from "vitest";

import { textChange } from "./changeFixtures";
import type { FixtureChange, FixtureGit, Versions } from "./changeFixtures";
import { objectIdCheck } from "./invariants";

describe("the object ids of a repository's fixtures", () => {
  const blob = (id: string) => ({ kind: "index", worktree: "wt-main", blob: id }) as const;
  const versions = (old: string, next: string): Versions => ({ old, new: next, patch: "" });
  const repository = (...changes: FixtureChange[]): FixtureGit => ({ worktrees: [], changes: { "wt-main": changes } });
  /** The conflicts without the checksum that tells two long contents apart. */
  const conflicts = (git: FixtureGit) => objectIdCheck(git).conflicts.map((message) => message.replace(/ \(\d+ chars, #\w+\)/g, ""));

  it("name one content each, and a content one id", () => {
    const git = repository(
      textChange("staged", "a.ts", blob("1111111"), blob("2222222"), versions("one\n", "two\n")),
      textChange("staged", "b.ts", blob("1111111"), blob("2222222"), versions("one\n", "two\n")),
    );

    expect(objectIdCheck(git)).toEqual({ conflicts: [], unchecked: [] });
  });

  it("are refused where one id names two contents", () => {
    const git = repository(
      textChange("staged", "a.ts", blob("1111111"), blob("2222222"), versions("one\n", "two\n")),
      textChange("staged", "b.ts", blob("1111111"), blob("3333333"), versions("other\n", "three\n")),
    );

    expect(conflicts(git)).toEqual(['1111111 names 2 contents: "one\\n", "other\\n"']);
  });

  it("are refused where one content is named by two ids", () => {
    const git = repository(
      textChange("staged", "a.ts", blob("1111111"), blob("2222222"), versions("one\n", "two\n")),
      textChange("staged", "b.ts", blob("4444444"), blob("2222222"), versions("one\n", "two\n")),
    );

    expect(conflicts(git)).toEqual(['"one\\n" is named by 1111111, 4444444']);
  });

  it("are read from a patch's index line as well, where its side is not a stored object", () => {
    const patch = "diff --git a/a.ts b/a.ts\nindex 1111111..5555555 100644\n";
    const git = repository(
      textChange("unstaged", "a.ts", blob("1111111"), { kind: "live", root_id: "fixture", version: "v1" }, { old: "one\n", new: "two\n", patch }),
      textChange("staged", "b.ts", blob("1111111"), blob("5555555"), versions("one\n", "three\n")),
    );

    expect(conflicts(git)).toEqual(['5555555 names 2 contents: "two\\n", "three\\n"']);
  });

  // A patch's later sections describe something other than the change's two sides, so only the
  // first is read; the id of a side the change carries no content for is reported as unchecked
  // rather than compared against nothing.
  it("report the id of a side whose content the change does not carry", () => {
    const patch = "diff --git a/a.ts b/a.ts\nindex 1111111..0000000\ndiff --git a/a.ts b/a.ts\nindex 0000000..6666666\n";
    const git = repository({ entry: { group: "staged", old: { state: "present", path: "a.ts", kind: "file", source: blob("1111111") }, new: { state: "present", path: "a.ts", kind: "symlink", source: blob("6666666") } }, patch, whole: { old: "one\n" } });

    expect(objectIdCheck(git)).toEqual({ conflicts: [], unchecked: ["6666666"] });
  });
});
