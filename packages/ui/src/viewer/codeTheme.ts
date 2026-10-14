/** The viewer uses GitHub high-contrast token palettes on a shared reading surface. The light
 * fill separates content from the white dialog while retaining readable syntax colours. */
export const CODE_THEMES = {
  light: { name: "github-light-high-contrast", background: "#f3f4f6", foreground: "#0e1116" },
  dark: { name: "github-dark-high-contrast", background: "#0a0c10", foreground: "#f0f3f6" },
} as const;
