import dark from "@shikijs/themes/github-dark-high-contrast";
import light from "@shikijs/themes/github-light-high-contrast";
import { expect, it } from "vitest";

import { CODE_THEMES } from "./codeTheme";

it.each([
  ["light", light],
  ["dark", dark],
] as const)("fills %s code with its theme's own colours", (appearance, theme) => {
  const colours = theme.colors ?? {};
  expect(CODE_THEMES[appearance]).toEqual({
    name: theme.name,
    background: colours["editor.background"],
    foreground: colours["editor.foreground"],
  });
});
