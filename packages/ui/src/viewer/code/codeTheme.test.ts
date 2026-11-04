import dark from "@shikijs/themes/github-dark-high-contrast";
import light from "@shikijs/themes/github-light-high-contrast";
import { expect, it } from "vitest";

import { CODE_THEMES } from "./codeTheme";

it.each([
  ["light", light],
  ["dark", dark],
] as const)("keeps the %s syntax palette on the reading surface", (appearance, theme) => {
  const colours = theme.colors ?? {};
  expect(CODE_THEMES[appearance]).toMatchObject({
    name: theme.name,
    background: appearance === "dark" ? "#1e1f22" : colours["editor.background"],
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

function contrast(foreground: string, background: string): number {
  const ink = luminance(foreground);
  const paper = luminance(background);
  return (Math.max(ink, paper) + 0.05) / (Math.min(ink, paper) + 0.05);
}

it("keeps dark syntax readable on the reading surface and changed lines", () => {
  // Browser-resolved Lab mixes from @pierre/diffs: 80% reading surface and 20% the palette's
  // gitDecoration added/deleted foreground. Explicit token backgrounds do not read this surface.
  for (const background of [CODE_THEMES.dark.background, "#283e2c", "#472f2f"]) {
    for (const token of dark.tokenColors ?? []) {
      if (!token.settings.foreground || token.settings.background) continue;
      expect(contrast(token.settings.foreground, background)).toBeGreaterThanOrEqual(4.5);
    }
  }
});

it("keeps dark line numbers and collapse labels readable on their own fills", () => {
  expect(CODE_THEMES.dark.numberForeground).toBe(dark.colors?.["editorLineNumber.foreground"]);
  // The separator mixes 15% white into the reading surface. Changed gutters instead mix 15%
  // of their Git colour into it, and draw their numbers in that same Git colour.
  for (const background of [CODE_THEMES.dark.background, "#3a3b3e"]) {
    expect(contrast(CODE_THEMES.dark.numberForeground, background)).toBeGreaterThanOrEqual(4.5);
  }
  expect(contrast(dark.colors!["gitDecoration.addedResourceForeground"] as string, "#26362a")).toBeGreaterThanOrEqual(4.5);
  expect(contrast(dark.colors!["gitDecoration.deletedResourceForeground"] as string, "#3d2b2c")).toBeGreaterThanOrEqual(4.5);
});
