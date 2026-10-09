import { expect, it } from "vitest";

import { comparisonNotices } from "./comparisonNotices";
import type { BranchList } from "./useBranchList";

const comparison = { left: { branch: "main", commit: "c1" }, right: { branch: "topic", commit: "c2" }, receivedAt: 10 };
const listed = (topic: string | undefined, { sentAt = 20, complete = true } = {}): BranchList => ({
  state: "loaded",
  branches: [{ name: "main", commit: "c1" }, ...(topic ? [{ name: "topic", commit: topic }] : [])],
  complete,
  sentAt,
});

it.each([
  ["a list asked for before the comparison says nothing", listed("c9", { sentAt: 5 }), []],
  ["a branch at the compared commit says nothing", listed("c2"), []],
  ["a branch at another commit has moved", listed("c9"), [{ kind: "moved", branch: "topic" }]],
  ["a branch missing from a complete list is gone", listed(undefined), [{ kind: "deleted", branch: "topic", commit: "c2" }]],
  ["a list cut short never says a branch is gone", listed(undefined, { complete: false }), []],
  ["no list says nothing", { state: "loading" } as BranchList, []],
  ["a branch compared with itself is named once", listed("c9"), [{ kind: "moved", branch: "topic" }], "topic"],
])("%s", (_, branches, expected, left?: string) => {
  const compared = left ? { ...comparison, left: { branch: left, commit: "c2" } } : comparison;
  expect(comparisonNotices(branches, compared)).toEqual(expected);
});
