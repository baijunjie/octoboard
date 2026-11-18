import { expect, it } from "vitest";

import { translatorFor } from "../i18n/catalog";
import { pullNotice } from "./pullNotice";

const t = translatorFor("en");

it.each([
  [{ kind: "up_to_date" }, "The current branch is already up to date."],
  [{ kind: "fast_forwarded" }, "Pulled the current branch."],
  [{ kind: "merged" }, "Pulled the current branch."],
] as const)("words a pull that ended %j as %s", (outcome, notice) => {
  expect(pullNotice(t, outcome)).toBe(notice);
});
