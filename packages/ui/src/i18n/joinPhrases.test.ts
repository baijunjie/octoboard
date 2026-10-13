import { expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "./isolate";
import { joinPhrases } from "./joinPhrases";

it.each([
  ["落后 1 个提交", `Git 错误：${isolate("原文Git error")}`, `落后 1 个提交和 Git 错误：${isolate("原文Git error")}`],
  [isolate("Website"), "分支", `${isolate("Website")} 和分支`],
  ["分支", isolate("main"), `分支和 ${isolate("main")}`],
  ["分支", "2 个提交", "分支和 2 个提交"],
  ["领先 2 个提交", "落后 1 个提交", "领先 2 个提交和落后 1 个提交"],
])("spaces Chinese joining words next to Latin text or numbers (%s, %s)", (first, second, expected) => {
  // Some engines' narrow Chinese lists use 和; others use 、. Exercise the joining word
  // independently of the test runner's ICU data, while keeping each element's text untouched.
  const parts = vi.spyOn(Intl.ListFormat.prototype, "formatToParts").mockReturnValue([
    { type: "element", value: first },
    { type: "literal", value: "和" },
    { type: "element", value: second },
  ]);
  onTestFinished(() => parts.mockRestore());
  expect(joinPhrases("zh-Hans", [first, second])).toBe(expected);
});

it("keeps punctuation separators and English lists in the runtime's own format", () => {
  const phrases = ["分支 main", "落后 1 个提交", "Git 错误：fetch failed"];
  const parts = vi.spyOn(Intl.ListFormat.prototype, "formatToParts").mockReturnValue([
    { type: "element", value: phrases[0] },
    { type: "literal", value: "、" },
    { type: "element", value: phrases[1] },
    { type: "literal", value: "、" },
    { type: "element", value: phrases[2] },
  ]);
  onTestFinished(() => parts.mockRestore());
  expect(joinPhrases("zh-Hans", phrases)).toBe("分支 main、落后 1 个提交、Git 错误：fetch failed");
  expect(joinPhrases("en", ["Branch main", "1 commit behind", "Git error: fetch failed"])).toBe("Branch main, 1 commit behind, Git error: fetch failed");
});
