import { expect, it } from "vitest";

import { comparisonNews } from "./comparisonNews";
import type { BranchList } from "./useBranchList";

const comparison = { left: { branch: "main", commit: "c1" }, right: { branch: "topic", commit: "c2" }, receivedAt: 10 };
const listed = (topic: string | undefined, { sentAt = 20, complete = true, main = "c1" } = {}): BranchList => ({
  state: "loaded",
  branches: [...(main ? [{ name: "main", commit: main }] : []), ...(topic ? [{ name: "topic", commit: topic }] : [])],
  complete,
  sentAt,
});

it.each([
  ["a list asked for before the comparison says nothing", listed("c9", { sentAt: 5 }), { moved: false, gone: [] }],
  ["branches at the compared commits say nothing", listed("c2"), { moved: false, gone: [] }],
  ["the branch compared at another commit has moved", listed("c9"), { moved: true, gone: [] }],
  ["the base branch at another commit has moved", listed("c2", { main: "c8" }), { moved: true, gone: [] }],
  ["a branch missing from a complete list is gone", listed(undefined), { moved: false, gone: ["topic"] }],
  ["a list cut short never says a branch is gone", listed(undefined, { complete: false }), { moved: false, gone: [] }],
  ["a moved branch is not compared again while another is gone", listed("c9", { main: "" }), { moved: false, gone: ["main"] }],
  ["no list says nothing", { state: "loading" } as BranchList, { moved: false, gone: [] }],
])("%s", (_, branches, expected) => {
  expect(comparisonNews(branches, comparison)).toMatchObject(expected);
});
