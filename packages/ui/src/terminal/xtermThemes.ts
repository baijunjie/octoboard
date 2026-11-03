import type { ITheme } from "@xterm/xterm";

/**
 * The two xterm colour themes the terminal switches between, one per resolved UI theme. xterm's
 * default ANSI colours assume a dark background, so an agent printing a plain ANSI colour (most
 * TUI output) would be unreadable on a light one. Each palette therefore gives all 16 ANSI colours
 * and the cursor pair a value checked for contrast against its own `background`, not borrowed from
 * the other mode.
 *
 * `dark` (background `#1e1f22`, foreground `#e4e4e6`) takes its ANSI colours from the "One Dark"
 * terminal scheme. `light` is a parallel palette in the style of "One Light": a light "paper"
 * background with dark, saturated foregrounds.
 *
 * ANSI black/white cannot mirror a well-known scheme unmodified: printed verbatim as foreground
 * text they would be close to invisible against a same-toned background (black-on-near-black,
 * white-on-near-white), so both sit a step away from their theme's background rather than at the
 * textbook value.
 */
export const XTERM_THEMES: Record<"light" | "dark", ITheme> = {
  dark: {
    background: "#1e1f22",
    foreground: "#e4e4e6",
    cursor: "#e4e4e6",
    cursorAccent: "#1e1f22",
    selectionBackground: "#3a3b40",
    black: "#45474d",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#d19a66",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#c9ccd1",
    brightBlack: "#767b87",
    brightRed: "#ef7983",
    brightGreen: "#a9d68c",
    brightYellow: "#e0ad7d",
    brightBlue: "#7ec1f5",
    brightMagenta: "#d892e8",
    brightCyan: "#6fcdda",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#fafafa",
    foreground: "#383a42",
    cursor: "#383a42",
    cursorAccent: "#fafafa",
    selectionBackground: "#d4d6db",
    black: "#383a42",
    red: "#d6265a",
    green: "#1d7a2e",
    yellow: "#986801",
    blue: "#2765c4",
    magenta: "#a325ac",
    cyan: "#0b7285",
    white: "#6e7076",
    brightBlack: "#8a8d93",
    brightRed: "#e4335f",
    brightGreen: "#2a9d3f",
    brightYellow: "#b6861a",
    brightBlue: "#3d7fe0",
    brightMagenta: "#c13ecb",
    brightCyan: "#1596ab",
    brightWhite: "#1b1c1f",
  },
};
