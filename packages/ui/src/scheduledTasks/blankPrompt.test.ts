import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "vitest";

import { isBlankPrompt } from "./blankPrompt";

/** A case of `apps/daemon/testdata/blank_prompt_cases.json`, which the daemon's `is_blank_once_stripped` runs too (`apps/daemon/src/session/term/mod.rs`). */
interface BlankCase {
  name: string;
  text: string;
  blank: boolean;
}

describe("the blank prompt rule shared with the daemon", () => {
  const { cases } = JSON.parse(
    readFileSync(new URL("../../../../apps/daemon/testdata/blank_prompt_cases.json", pathToFileURL(`${import.meta.dirname}/`)), "utf8"),
  ) as { cases: BlankCase[] };

  it.each(cases.map((testCase) => [testCase.name, testCase] as const))("%s", (_, testCase) => {
    expect(isBlankPrompt(testCase.text)).toBe(testCase.blank);
  });
});
