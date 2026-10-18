import { expect, it } from "vitest";

import type { BranchInfo } from "../protocol";
import { sessionBranch } from "./sessionBranch";

const made = (name: string, session: string | undefined, responsible_since: number): BranchInfo => ({
  name,
  commit: "c",
  origin: session === undefined ? undefined : { responsible_session: session, responsible_title: "t", responsible_since },
});

it("takes the branch the session most recently became responsible for", () => {
  const branches = [made("old", "s", 1), made("new", "s", 5), made("other", "t", 9), made("bare", undefined, 0)];
  expect(sessionBranch(branches, "s")).toBe("new");
  expect(sessionBranch(branches, "nobody")).toBeUndefined();
});
