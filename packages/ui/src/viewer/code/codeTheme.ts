/** The viewer uses GitHub high-contrast token palettes. Light code keeps its white editor
 * background; dark code sits on a lighter reading surface to stand apart from the dimmed window. */
export const CODE_THEMES = {
  light: { name: "github-light-high-contrast", background: "#ffffff", foreground: "#0e1116" },
  dark: { name: "github-dark-high-contrast", background: "#1e1f22", foreground: "#f0f3f6", numberForeground: "#9ea7b3" },
} as const;

/** One side of a diff: the fill of a changed line, the deeper one of its line number's cell, and the
 * one of a changed word within the line. */
interface ChangeTints {
  line: string;
  number: string;
  word: string;
}

/** The fills of a bar of collapsed lines: the bar, an expanding control on it at rest, the control
 * hovered or focused, the glyph on that deeper fill, and the bar's text and resting glyphs. */
interface SeparatorTints {
  bar: string;
  control: string;
  controlActive: string;
  onControlActive: string;
  text: string;
}

/**
 * A diff's fills, which are GitHub's in each appearance: a pale line, and a clearly deeper fill
 * for its line number's cell and for a changed word, on blue bars of collapsed lines. The dark
 * ones are GitHub's translucent fills composited on the dark reading surface, so every one is a
 * plain colour a ratio can be taken from. They stand in place of the application's own tint and
 * accent tokens BY THE USER'S CHOICE, who asked for GitHub's look exactly — see
 * docs/conventions/ui-standard.md's "One definition per semantic colour" section for the rule
 * they are an exception to. The light deletion line is a shade paler than GitHub's,
 * which would take the palette's comments under 4.5:1.
 */
export const DIFF_TINTS: Record<"light" | "dark", { addition: ChangeTints; deletion: ChangeTints; separator: SeparatorTints }> = {
  light: {
    addition: { line: "#dafbe1", number: "#aceebb", word: "#aceebb" },
    deletion: { line: "#ffefed", number: "#ffcecb", word: "#ffcecb" },
    separator: { bar: "#ddf4ff", control: "#bbdfff", controlActive: "#0969da", onControlActive: "#ffffff", text: "#59636e" },
  },
  dark: {
    addition: { line: "#203227", number: "#284d30", word: "#245330" },
    deletion: { line: "#3f2628", number: "#5f2e2e", word: "#753332" },
    separator: { bar: "#212a38", control: "#284a7a", controlActive: "#1f6feb", onControlActive: "#ffffff", text: CODE_THEMES.dark.numberForeground },
  },
};

/** How much of a token's own colour its text keeps on a changed word's fill, the rest being black
 * in the light appearance and white in the dark one: the fill is deep enough to take the palette's
 * greyest tokens under 4.5:1 as they are. */
export const CHANGED_WORD_INK = { light: 0.8, dark: 0.75 } as const;
