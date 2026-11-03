/** The viewer uses GitHub high-contrast token palettes on their own editor backgrounds. The frame's
 * band (`FrameSurface`) separates the code from the dialog, so the light code stays on white,
 * the surface the diff's tints are chosen for. */
export const CODE_THEMES = {
  light: { name: "github-light-high-contrast", background: "#ffffff", foreground: "#0e1116" },
  dark: { name: "github-dark-high-contrast", background: "#0a0c10", foreground: "#f0f3f6" },
} as const;
