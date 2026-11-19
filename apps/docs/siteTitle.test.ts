import assert from "node:assert/strict";
import test from "node:test";
import { LANGUAGES } from "@octoboard/ui/languages";
import { docsTitles, docsWords } from "./siteTitle.ts";

test("every language has its word for \"Docs\"", () => {
  assert.deepEqual(Object.keys(docsWords).sort(), [...LANGUAGES].sort());
});

test("every language has a whole title naming the product", () => {
  assert.deepEqual(Object.keys(docsTitles).sort(), [...LANGUAGES].sort());
  for (const [language, title] of Object.entries(docsTitles)) assert.equal(title.split("{name}").length, 2, language);
  assert.equal(docsTitles.en, "{name} Docs");
});
