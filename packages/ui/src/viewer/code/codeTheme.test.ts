import dark from "@shikijs/themes/github-dark-high-contrast";
import light from "@shikijs/themes/github-light-high-contrast";
import { describe, expect, it } from "vitest";

import { CHANGED_WORD_INK, CODE_THEMES, DIFF_TINTS } from "./codeTheme";

const THEMES = { light, dark } as const;
const APPEARANCES = ["light", "dark"] as const;
const SIDES = ["addition", "deletion"] as const;

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

function contrast(foreground: string, background: string): number {
  const ink = luminance(foreground);
  const paper = luminance(background);
  return (Math.max(ink, paper) + 0.05) / (Math.min(ink, paper) + 0.05);
}

// The sRGB channel-wise mix the stylesheet asks the browser for: `color-mix(in srgb, colour ink%, mixer)`.
function mixed(appearance: (typeof APPEARANCES)[number], colour: string): string {
  const ink = CHANGED_WORD_INK[appearance];
  const mixer = appearance === "light" ? 0 : 255;
  const channels = colour.slice(1).match(/../g)!
    .map((channel) => Math.round(parseInt(channel, 16) * ink + mixer * (1 - ink)));
  return `#${channels.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

// Tokens with a background of their own do not sit on the diff fills.
function tokenColours(appearance: (typeof APPEARANCES)[number]): string[] {
  return (THEMES[appearance].tokenColors ?? [])
    .filter((token) => token.settings.foreground && !token.settings.background)
    .map((token) => token.settings.foreground as string);
}

it("keeps light token text readable on the reading surface", () => {
  for (const token of light.tokenColors ?? []) {
    if (!token.settings.foreground) continue;
    const foreground = luminance(token.settings.foreground);
    const background = luminance(token.settings.background ?? CODE_THEMES.light.background);
    expect((Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05)).toBeGreaterThanOrEqual(4.5);
  }
});

it("keeps the dark line number colour the palette's and readable on the reading surface", () => {
  expect(CODE_THEMES.dark.numberForeground).toBe(dark.colors?.["editorLineNumber.foreground"]);
  expect(contrast(CODE_THEMES.dark.numberForeground, CODE_THEMES.dark.background)).toBeGreaterThanOrEqual(4.5);
});

describe.each(APPEARANCES)("%s changed lines", (appearance) => {
  const surface = CODE_THEMES[appearance].background;
  const foreground = CODE_THEMES[appearance].foreground;
  const tints = DIFF_TINTS[appearance];

  it("keeps every syntax token readable on the reading surface and both line fills", () => {
    const tokens = tokenColours(appearance);
    expect(tokens.length).toBeGreaterThan(0);
    for (const background of [surface, tints.addition.line, tints.deletion.line]) {
      for (const token of tokens) {
        expect(contrast(token, background), `${token} on ${background}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps every syntax token and the code foreground readable on both changed-word fills once mixed", () => {
    const tokens = [...tokenColours(appearance), foreground];
    for (const side of SIDES) {
      for (const token of tokens) {
        expect(contrast(mixed(appearance, token), tints[side].word), `${token} on ${tints[side].word}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it.each(SIDES)("keeps the code foreground readable on the %s line number fill", (side) => {
    expect(contrast(foreground, tints[side].number)).toBeGreaterThanOrEqual(4.5);
  });

  it.each(SIDES)("draws the %s line number cell and changed words deeper than the line", (side) => {
    const line = luminance(tints[side].line);
    for (const deeper of [tints[side].number, tints[side].word]) {
      if (appearance === "light") expect(luminance(deeper)).toBeLessThan(line);
      else expect(luminance(deeper)).toBeGreaterThan(line);
    }
  });

  it("keeps the collapsed-lines separator text readable on its bar and controls", () => {
    const { separator } = tints;
    expect(contrast(separator.text, separator.bar)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(separator.text, separator.control)).toBeGreaterThanOrEqual(3);
    expect(contrast(separator.onControlActive, separator.controlActive)).toBeGreaterThanOrEqual(4.5);
  });
});

it("draws the dark separator text in the palette's line number colour", () => {
  expect(DIFF_TINTS.dark.separator.text).toBe(CODE_THEMES.dark.numberForeground);
});
