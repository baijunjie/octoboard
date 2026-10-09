import { expect, it } from "vitest";

import { displayWirePath, wireBaseName } from "./wirePath";

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
