/** The viewer uses GitHub high-contrast token palettes. Light code keeps its white editor
 * background; dark code sits on a lighter reading surface to stand apart from the dimmed window. */
export const CODE_THEMES = {
  light: { name: "github-light-high-contrast", background: "#ffffff", foreground: "#0e1116" },
  dark: { name: "github-dark-high-contrast", background: "#1e1f22", foreground: "#f0f3f6", numberForeground: "#9ea7b3" },
} as const;
