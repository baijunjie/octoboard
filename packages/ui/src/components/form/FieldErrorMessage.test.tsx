import { expect, it } from "vitest";

import { hasErrorMessage } from "./FieldErrorMessage";

it.each<[string, Parameters<typeof hasErrorMessage>[0], boolean]>([
  ["undefined", undefined, false],
  ["null", null, false],
  ["false", false, false],
  ["an empty string", "", false],
  ["a message", "Enter a name.", true],
  ["a message held as markup", ["Enter ", "a name."], true],
  ["a space", " ", true],
  ["zero", 0, true],
  ["true", true, true],
])("says whether a field is invalid by whether its message says anything: %s", (_, errorMessage, expected) => {
  expect(hasErrorMessage(errorMessage)).toBe(expected);
});
