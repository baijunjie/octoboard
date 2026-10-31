import { describe, expect, it } from "vitest";

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

describe("a remote-tracking base", () => {
  const remote = { left: { branch: "origin/main", commit: "c1", remote: true }, right: { branch: "main", commit: "c2" }, receivedAt: 10 };
  const tracking = (upstream: string | undefined, { complete = true, main = "c2" } = {}): BranchList => ({
    state: "loaded",
    branches: [{ name: "main", commit: main, upstream: upstream ? { name: "origin/main", commit: upstream } : undefined }],
    complete,
    sentAt: 20,
  });

  it.each([
    ["the upstream at the compared commit says nothing", tracking("c1"), { moved: false, gone: [], tips: ["c1", "c2"] }],
    ["the upstream at another commit has moved", tracking("c9"), { moved: true, gone: [], tips: ["c9", "c2"] }],
    ["an upstream no branch has any longer is not gone, the branch gets another base", tracking(undefined), { moved: false, gone: [], tips: ["", "c2"] }],
    ["the default branch is read like an upstream", { ...tracking(undefined), defaultBranch: { name: "origin/main", commit: "c9" } } as BranchList, { moved: true, gone: [], tips: ["c9", "c2"] }],
  ])("%s", (_, branches, expected) => {
    expect(comparisonNews(branches, remote)).toMatchObject(expected);
  });
});

it("says nothing of a comparison from the empty tree beyond its branch moving", () => {
  const nothing = { left: { branch: "", commit: "t0", empty: true }, right: { branch: "main", commit: "c2" }, receivedAt: 10 };
  expect(comparisonNews(listed("c2", { main: "c2" }), { ...nothing, right: { branch: "topic", commit: "c2" } })).toMatchObject({ moved: false, gone: [], tips: ["c2"] });
  expect(comparisonNews(listed("c9"), { ...nothing, right: { branch: "topic", commit: "c2" } })).toMatchObject({ moved: true, gone: [], tips: ["c9"] });
});
