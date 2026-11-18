import { expect, it } from "vitest";

import { knownValues } from "./knownValue";

type Colour = "red" | "green";
const isColour = knownValues<Colour>({ red: true, green: true });

it.each(["red", "green"])("says %s, a listed value, is known", (value) => {
  expect(isColour(value)).toBe(true);
});

it.each(["blue", "", "Red", "toString", "__proto__", "constructor", "hasOwnProperty"])("says %j, which is not listed, is not known", (value) => {
  expect(isColour(value)).toBe(false);
});

it("says neither null nor undefined is known", () => {
  expect(isColour(null)).toBe(false);
  expect(isColour(undefined)).toBe(false);
});

it("narrows a received value to the listed union where it is known", () => {
  const raw: string = "green";
  const known: Colour | undefined = isColour(raw) ? raw : undefined;
  expect(known).toBe("green");
});
