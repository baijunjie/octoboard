import type React from "react";

import type { ConsoleSessionColour } from "./protocol";

/**
 * The console session colour palette: what each `ConsoleSessionColour` looks like, the one table
 * every drawing of it reads, so a colour is never kept in two places. A badge paints
 * `sessionColourVars`; the colour picker's swatches hold `sessionColourHex` for the appearance in
 * force, which is a real colour value, so react-aria names each swatch in the user's language
 * (a colour name is never written out here).
 *
 * Sixteen colours, one per family: fourteen hues around the colour wheel in OKLCH, roughly evenly
 * spaced with teal and blue nudged off it (see below), and brown and gray for the muted ones. Each is meant to read as its own name in both
 * appearances, so its lightness and chroma are its own rather than shared: yellow is bright, blue
 * and violet are deeper, in the spirit of Tailwind's 500 steps in the light appearance (yellow its
 * 400) and 400 steps in the dark one. That puts entries near HeroUI's own status colours (danger,
 * warning, success); accepted, since the colour is the user's to choose and a console session's
 * badge carries no status, which the row's own status glyph says.
 *
 * react-aria names a swatch from its value and typeahead goes by that name, so the names should differ
 * in each appearance and in every language the UI offers (`sessionColours.test.ts` checks all of
 * them): teal's hue leans towards green and blue's towards indigo for that, which is why they sit off
 * the even spacing. Two collisions are accepted, being in react-aria's own strings: Arabic names
 * violet and fuchsia alike, and Korean gives one word to green-cyan, cyan and cyan-blue (teal with
 * cyan in the light appearance, with sky in the dark).
 *
 * The colour is decorative: a dot beside the session's name, which is always visible or in the
 * dot's tooltip, so it is never the only carrier of anything and the 3:1 bar for a graphic (WCAG
 * 1.4.11) does not apply. Holding every colour to it would turn yellow olive, pink rose and orange
 * brown in the light appearance, and make the colours alike. The pale entries (yellow, lime, amber
 * on the light sidebar, gray on the dark one) stay visible instead through a hairline ring of the
 * foreground colour round the dot and each swatch (`.session-colour-ring` in `style.css`).
 *
 * The chroma written below is a cap: `oklchToHex` reduces it to what the sRGB gamut holds at the
 * lightness and hue, so no entry depends on the browser's gamut mapping.
 */

/** Which of the two appearances a colour is drawn for. */
export type ColourTheme = "light" | "dark";

/** What each colour is, in OKLCH: the hue angle, and the lightness and the most chroma it is given
 * in each appearance; the gamut may allow less. */
const COLOURS: Record<ConsoleSessionColour, { hue: number; light: [number, number]; dark: [number, number] }> = {
  red: { hue: 25, light: [0.637, 0.237], dark: [0.704, 0.191] },
  orange: { hue: 50, light: [0.705, 0.213], dark: [0.75, 0.183] },
  amber: { hue: 75, light: [0.769, 0.188], dark: [0.828, 0.189] },
  yellow: { hue: 95, light: [0.852, 0.199], dark: [0.88, 0.19] },
  lime: { hue: 130, light: [0.768, 0.233], dark: [0.841, 0.238] },
  green: { hue: 150, light: [0.723, 0.219], dark: [0.792, 0.209] },
  teal: { hue: 170, light: [0.704, 0.14], dark: [0.777, 0.152] },
  cyan: { hue: 213, light: [0.715, 0.143], dark: [0.789, 0.154] },
  sky: { hue: 235, light: [0.685, 0.169], dark: [0.746, 0.16] },
  blue: { hue: 266, light: [0.623, 0.214], dark: [0.707, 0.165] },
  indigo: { hue: 277, light: [0.585, 0.233], dark: [0.673, 0.182] },
  violet: { hue: 293, light: [0.606, 0.25], dark: [0.702, 0.183] },
  fuchsia: { hue: 322, light: [0.667, 0.295], dark: [0.74, 0.238] },
  pink: { hue: 352, light: [0.7, 0.22], dark: [0.75, 0.2] },
  brown: { hue: 55, light: [0.52, 0.085], dark: [0.65, 0.09] },
  gray: { hue: 262, light: [0.6, 0.02], dark: [0.7, 0.02] },
};

/** Linear-light sRGB channels of an OKLCH colour (Björn Ottosson's OKLab matrices). */
function linearRgb(lightness: number, chroma: number, hue: number): [number, number, number] {
  const a = chroma * Math.cos((hue * Math.PI) / 180);
  const b = chroma * Math.sin((hue * Math.PI) / 180);
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

function inGamut(channels: number[]): boolean {
  return channels.every((channel) => channel >= -1e-6 && channel <= 1 + 1e-6);
}

/** The chroma, at most `chroma`, the sRGB gamut holds at this lightness and hue. */
function fittingChroma(lightness: number, chroma: number, hue: number): number {
  if (inGamut(linearRgb(lightness, chroma, hue))) return chroma;
  let low = 0;
  let high = chroma;
  for (let step = 0; step < 24; step++) {
    const middle = (low + high) / 2;
    if (inGamut(linearRgb(lightness, middle, hue))) low = middle;
    else high = middle;
  }
  return low;
}

function encode(linear: number): number {
  const clamped = Math.min(1, Math.max(0, linear));
  return clamped <= 0.0031308 ? 12.92 * clamped : 1.055 * clamped ** (1 / 2.4) - 0.055;
}

/** An OKLCH colour as an sRGB `#rrggbb`, with its chroma reduced to what the gamut holds. */
function oklchToHex(lightness: number, chroma: number, hue: number): string {
  const channels = linearRgb(lightness, fittingChroma(lightness, chroma, hue), hue);
  return `#${channels.map((channel) => Math.round(encode(channel) * 255).toString(16).padStart(2, "0")).join("")}`;
}

/** A palette entry's colour for one appearance, as an sRGB `#rrggbb`: a value `parseColor` reads, so
 * a swatch picker can hold it, and the one value both the swatches and the badges draw from.
 * `undefined` for a label outside the palette, such as one a newer daemon assigned. */
export function sessionColourHex(colour: ConsoleSessionColour, theme: ColourTheme): string | undefined {
  if (!Object.hasOwn(COLOURS, colour)) return undefined;
  const entry = COLOURS[colour];
  const [lightness, chroma] = entry[theme];
  return oklchToHex(lightness, chroma, entry.hue);
}

/** A palette entry's colour in both appearances, as the two custom properties the
 * `.session-colour-dot` rule in `style.css` paints with, the second one in the dark appearance;
 * `undefined` for a label outside the palette. */
export function sessionColourVars(colour: ConsoleSessionColour): React.CSSProperties | undefined {
  const light = sessionColourHex(colour, "light");
  const dark = sessionColourHex(colour, "dark");
  if (!light || !dark) return undefined;
  return { "--session-colour-light": light, "--session-colour-dark": dark } as React.CSSProperties;
}
