import { expect, it } from "vitest";

import { countLines, diffPlan, RENDER_BUDGETS, textPlan } from "./budgets";

const lines = (count: number) => "x\n".repeat(count);

it.each([
  ["", 0],
  ["a", 1],
  ["a\n", 1],
  ["a\nb", 2],
])("counts %j as %i lines", (text, count) => {
  expect(countLines(text)).toBe(count);
});

it.each([
  ["within both budgets", lines(RENDER_BUDGETS.highlightLines), "highlight"],
  ["past the line budget", lines(RENDER_BUDGETS.highlightLines + 1), "plain"],
  ["past the length budget", "x".repeat(RENDER_BUDGETS.highlightChars + 1), "plain"],
])("highlights text %s or shows it plain", (_, text, plan) => {
  expect(textPlan(text)).toBe(plan);
});

it.each([
  ["renders a patch within the budget", lines(RENDER_BUDGETS.diffLines), "render"],
  ["shows a longer patch as text", lines(RENDER_BUDGETS.diffLines + 1), "patch"],
  ["shows a wider patch as text", "x".repeat(RENDER_BUDGETS.diffChars + 1), "patch"],
])("%s", (_, patch, plan) => {
  expect(diffPlan(patch)).toBe(plan);
});
