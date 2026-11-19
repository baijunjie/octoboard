import assert from "node:assert/strict";
import test from "node:test";
import { FALLBACK_LANGUAGE, LANGUAGE_NAMES, LANGUAGES } from "@octoboard/ui/languages";
import { locales } from "./locales.ts";

test("the docs offer every language of the application, in its order, with English as the unprefixed root", () => {
  assert.equal(LANGUAGES.length, 17);
  assert.deepEqual(Object.keys(locales), LANGUAGES.map((language) => (language === FALLBACK_LANGUAGE ? "root" : language)));
  assert.equal(FALLBACK_LANGUAGE, "en");
  assert.equal(locales.root?.lang, "en");
  assert.equal("en" in locales, false);
});

test("a locale's path segment and its lang are the language's tag, and its label the language's own name", () => {
  for (const language of LANGUAGES) {
    const key = language === FALLBACK_LANGUAGE ? "root" : language;
    assert.equal(locales[key]?.lang, language, language);
    assert.equal(locales[key]?.label, LANGUAGE_NAMES[language], language);
  }
  assert.equal(locales["zh-CN"]?.lang, "zh-CN");
  assert.equal(locales["zh-TW"]?.lang, "zh-TW");
  for (const retired of ["zh", "zh-Hans", "zh-Hant"]) assert.equal(retired in locales, false, retired);
});

test("Arabic reads right to left and every other language left to right", () => {
  assert.equal(locales.ar?.dir, "rtl");
  for (const [key, locale] of Object.entries(locales)) {
    if (key !== "ar") assert.equal(locale.dir, "ltr", key);
  }
});
