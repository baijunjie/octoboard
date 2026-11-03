import { expect, it } from "vitest";

import { parseMarkdownStyle } from "./markdownStyle";

it.each([
  ["github", "github"],
  ["latex", "latex"],
  [null, "tailwind"],
  ["", "tailwind"],
  ["sepia", "tailwind"],
])("reads the stored %j as %s", (raw, expected) => {
  expect(parseMarkdownStyle(raw)).toBe(expected);
});
