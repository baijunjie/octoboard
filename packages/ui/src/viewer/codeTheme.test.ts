import dark from "@shikijs/themes/github-dark-high-contrast";
import light from "@shikijs/themes/github-light-high-contrast";
import { expect, it } from "vitest";

import { CODE_THEMES } from "./codeTheme";

it.each([
  ["light", light],
  ["dark", dark],
] as const)("keeps the %s syntax palette on the reading surface", (appearance, theme) => {
  const colours = theme.colors ?? {};
  expect(CODE_THEMES[appearance]).toEqual({
    name: theme.name,
    background: appearance === "light" ? "#f3f4f6" : colours["editor.background"],
    foreground: colours["editor.foreground"],
  });
});

function luminance(hex: string): number {
  const channels = hex.slice(1).match(/../g)!
    .map((channel) => parseInt(channel, 16) / 255)
    .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4));
  return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
}

it("keeps light token text readable on the reading surface", () => {
  for (const token of light.tokenColors ?? []) {
    if (!token.settings.foreground) continue;
    const foreground = luminance(token.settings.foreground);
    const background = luminance(token.settings.background ?? CODE_THEMES.light.background);
    expect((Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05)).toBeGreaterThanOrEqual(4.5);
  }
});
