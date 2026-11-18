import { expect, it } from "vitest";

import { branchItemOf, projectOf, sessionOf, worktreeItemOf } from "../fixtures/builders";
import { branchTargets } from "./branchInspection";

const project = projectOf("p", "c", "Site");
const session = sessionOf("s", "c", "p", "Work", "idle");

it("names each item back as it was shown, with a tip of null where it has none", () => {
  const branch = branchItemOf(project, session, "feat/a");
  const worktree = { ...worktreeItemOf(project, session, "feat/b", "/work/b", "locked"), tip: "abc123" };
  expect(branchTargets([branch, worktree])).toEqual([
    { kind: "branch", repository: branch.repository, branch: "feat/a", path: null, state: "unmerged", uncommitted: false, tip: null },
    { kind: "worktree", repository: worktree.repository, branch: "feat/b", path: "/work/b", state: "locked", uncommitted: false, tip: "abc123" },
  ]);
});

it("leaves out an item of a kind or in a state this build does not know, so a request never names one", () => {
  const known = branchItemOf(project, session, "feat/a");
  const futureKind = { ...branchItemOf(project, session, "feat/b"), kind: "from_a_newer_daemon" };
  const futureState = { ...branchItemOf(project, session, "feat/c"), state: "from_a_newer_daemon" };
  expect(branchTargets([futureKind, known, futureState]).map((target) => target.branch)).toEqual(["feat/a"]);
});
