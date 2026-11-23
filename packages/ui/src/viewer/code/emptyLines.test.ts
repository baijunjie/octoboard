import { getSharedHighlighter, renderFileWithHighlighter } from "@pierre/diffs";
import { expect, it } from "vitest";

import { CODE_THEMES } from "./codeTheme";

const THEMES = { light: CODE_THEMES.light.name, dark: CODE_THEMES.dark.name };

type ElementContent = ReturnType<typeof renderFileWithHighlighter>["code"][number];

function textOf(node: ElementContent): string {
  if (node.type === "text") return node.value;
  return node.type === "element" ? node.children.map(textOf).join("") : "";
}

// A line holding no text is left out of a copied selection, and a Markdown fence draws it with no height. Shiki draws an
// empty line of plain text as an empty token, which the renderer's patch (`patches/@pierre__diffs@1.5.2.patch`) gives
// the newline a highlighted language's empty line has.
it.each([
  ["plain text", "notes.txt"],
  ["a highlighted language", "notes.ts"],
])("draws an empty line of %s holding a newline", async (_, name) => {
  const highlighter = await getSharedHighlighter({ themes: [THEMES.light, THEMES.dark], langs: name.endsWith(".ts") ? ["typescript"] : [] });
  const { code } = renderFileWithHighlighter({ name, contents: "beta\n\ndelta" }, highlighter, { theme: THEMES, useTokenTransformer: false, tokenizeMaxLineLength: 1000 });
  expect(code.map(textOf)).toEqual(["beta", "\n", "delta"]);
});
