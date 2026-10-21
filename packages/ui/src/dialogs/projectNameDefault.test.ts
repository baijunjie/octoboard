import { expect, it } from "vitest";

import { projectNameDefault } from "./projectNameDefault";

it.each([
  [" /work/site/ ", "site"],
  ["/work/site/../other/./", "other"],
  ["~/site/..", "dev"],
  ["~", "dev"],
  ["~//..", "/"],
  ["/../../", "/"],
  ["relative/site", undefined],
  ["", undefined],
])("shows the daemon's normalized directory name for %s", (path, expected) => {
  expect(projectNameDefault("local", path, "", "/home/dev")).toBe(expected);
});

it.each([
  ["https://example.com/owner/site.git", "site"],
  ["git@example.com:owner/site.git/", "site"],
  ["  https://example.com/owner/site/  ", "site"],
  ["host:site.git", "site"],
  ["https://example.com/owner/site.GIT", "site.GIT"],
  ["", undefined],
  [".git", undefined],
])("uses the clone directory's name from remote %s", (remote, expected) => {
  expect(projectNameDefault("git", "/unrelated/parent", remote, null)).toBe(expected);
});

it("does not guess an unknown host home or a batch's project names", () => {
  expect(projectNameDefault("local", "~/..", "", null)).toBeUndefined();
  expect(projectNameDefault("parent", "/work", "", "/home/dev")).toBeUndefined();
});
