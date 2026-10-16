import { parseColor } from "react-aria-components";
import { expect, it } from "vitest";

import { LANGUAGES } from "./i18n/languages";
import { CONSOLE_SESSION_COLOURS, type ConsoleSessionColour } from "./protocol";
import { sessionColourHex, sessionColourVars } from "./sessionColours";

/** The colour names react-aria gives to more than one colour of the palette, by language and
 * appearance. Its Arabic strings name violet and fuchsia alike, and its Korean ones give one word to
 * green-cyan, cyan and cyan-blue, which no hue tuning separates. */
const ACCEPTED_NAME_COLLISIONS: Record<string, string[]> = {
  "ar light": ["violet/fuchsia"],
  "ar dark": ["violet/fuchsia"],
  "ko light": ["teal/cyan"],
  "ko dark": ["teal/sky"],
};

/** The pairs of colours that share a name in `language`, as `first/second` labels. */
function collisions(language: string, theme: "light" | "dark"): string[] {
  const byName = new Map<string, ConsoleSessionColour>();
  const pairs: string[] = [];
  for (const colour of CONSOLE_SESSION_COLOURS) {
    const name = parseColor(sessionColourHex(colour, theme)!).getColorName(language);
    const first = byName.get(name);
    if (first) pairs.push(`${first}/${colour}`);
    else byName.set(name, colour);
  }
  return pairs;
}

it("gives every colour a distinct value a swatch picker reads, in both appearances", () => {
  expect(CONSOLE_SESSION_COLOURS).toHaveLength(16);
  for (const theme of ["light", "dark"] as const) {
    const values = CONSOLE_SESSION_COLOURS.map((colour) => sessionColourHex(colour, theme)!);
    expect(new Set(values).size).toBe(values.length);
    for (const value of values) expect(parseColor(value).toString("hex").toLowerCase()).toBe(value);
  }
  expect(sessionColourVars("red")).toEqual({ "--session-colour-light": sessionColourHex("red", "light"), "--session-colour-dark": sessionColourHex("red", "dark") });
});

it.each(LANGUAGES)("names every colour apart in %s, bar the accepted collisions", (language) => {
  // react-aria names a swatch from its value in the window's language, and typeahead goes by that name.
  for (const theme of ["light", "dark"] as const) {
    expect(collisions(language, theme), `${language} ${theme}`).toEqual(ACCEPTED_NAME_COLLISIONS[`${language} ${theme}`] ?? []);
  }
});

it("gives nothing for a label outside the palette", () => {
  expect(sessionColourHex("mauve" as ConsoleSessionColour, "light")).toBeUndefined();
  expect(sessionColourHex("red-1" as ConsoleSessionColour, "dark")).toBeUndefined();
  expect(sessionColourHex("toString" as ConsoleSessionColour, "dark")).toBeUndefined();
  expect(sessionColourVars("mauve" as ConsoleSessionColour)).toBeUndefined();
});
