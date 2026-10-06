import { expect, it } from "vitest";

import { matchLanguage } from "./matchLanguage";

it.each([
  [["en-GB"], "en"],
  [["in"], "id"],
  [["nl", "fr-CA"], "fr"],
  [["nl"], "en"],
  [["zh-TW"], "zh-Hant"],
  [["zh_HK"], "zh-Hant"],
  [["zh-CN"], "zh-Hans"],
  [["zh"], "zh-Hans"],
  [["zh-Hant-CN"], "zh-Hant"],
  [["zh-Latn"], "en"],
  [["pt-PT"], "pt-BR"],
])("maps %j to %s", (preferred, expected) => {
  expect(matchLanguage(preferred)).toBe(expected);
});
