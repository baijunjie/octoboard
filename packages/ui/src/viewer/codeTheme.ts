/**
 * The highlighting theme the viewer draws code in for each appearance, and that theme's own
 * background and text colours. Every frame of code — highlighted, a diff, or plain text — is filled
 * with the background, so a region of code is one panel in both appearances: the library paints
 * only behind the lines it draws, so a short file would otherwise leave a strip as tall as itself,
 * and plain text would sit on a different colour. The colours are the themes' `editor.background`
 * and `editor.foreground` (`codeTheme.test.ts` checks them against the themes), against which every
 * token colour was measured at 4.5:1 or more.
 */
export const CODE_THEMES = {
  light: { name: "github-light-high-contrast", background: "#ffffff", foreground: "#0e1116" },
  dark: { name: "github-dark-high-contrast", background: "#0a0c10", foreground: "#f0f3f6" },
} as const;
