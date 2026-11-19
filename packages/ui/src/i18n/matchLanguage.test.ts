import { expect, it } from "vitest";

import { matchLanguage } from "./matchLanguage";

it.each([
  [["en-GB"], "en"],
  [["in"], "id"],
  [["nl", "fr-CA"], "fr"],
  [["nl"], "en"],
  [["zh-TW"], "zh-TW"],
  [["zh_HK"], "zh-TW"],
  [["zh-MO"], "zh-TW"],
  [["zh-Hant"], "zh-TW"],
  [["zh-Hant-HK"], "zh-TW"],
  [["zh-CN"], "zh-CN"],
  [["zh-SG"], "zh-CN"],
  [["zh-Hans"], "zh-CN"],
  [["zh-Hans-CN"], "zh-CN"],
  [["zh-Hans-TW"], "zh-CN"],
  [["zh"], "zh-CN"],
  [["zh-Hant-CN"], "zh-TW"],
  [["zh-Latn"], "en"],
  [["pt-PT"], "pt"],
  [["pt"], "pt"],
])("maps %j to %s", (preferred, expected) => {
  expect(matchLanguage(preferred)).toBe(expected);
});
