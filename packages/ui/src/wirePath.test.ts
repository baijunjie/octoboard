import { expect, it } from "vitest";

import { displayWirePath, wireBaseName, wireRelativeOrAbsolute } from "./wirePath";

it.each([
  ["src/main.ts", "src/main.ts"],
  ["100%25 done.txt", "100% done.txt"],
  ["%FFreport.md", "�report.md"],
  ["日本語/%E2%9C%93.md", "日本語/✓.md"],
])("shows the wire path %s as %s", (wire, shown) => {
  expect(displayWirePath(wire)).toBe(shown);
});

it("names a path by its last component", () => {
  expect(wireBaseName("docs/100%25.md")).toBe("100%.md");
});

it.each([
  ["/work/app/.worktrees/topic", ["/work/app"], ".worktrees/topic"],
  ["/work/app/.worktrees/topic/", ["/work/app/"], ".worktrees/topic"],
  ["/work/app", ["/work/app/"], "."],
  ["/work/app-2", ["/work/app"], "/work/app-2"],
  ["/work/other/topic", ["/work/app"], "/work/other/topic"],
  ["/work/app/100%25", ["/work/app"], "100%"],
  ["/real/app/topic", ["/link/app", "/real/app"], "topic"],
  ["/anything", ["/"], "anything"],
  ["/anything", [], "/anything"],
])("copies %s below %j as %s", (path, roots, copied) => {
  expect(wireRelativeOrAbsolute(path, roots)).toBe(copied);
});
