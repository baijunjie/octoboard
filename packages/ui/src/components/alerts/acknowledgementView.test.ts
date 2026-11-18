// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "../../errors/codedError";
import { acknowledgementViewOf } from "./acknowledgementView";
import { translatorFor } from "../../i18n/catalog";

const t = translatorFor("en");

it("words an actionable failure for the failure modal, titled as asked, with its error kept and its classification actionable", () => {
  const error = new CodedError("Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed: src/a.ts\nsrc/b.ts", "pull_merge_conflict", { detail: "src/a.ts\nsrc/b.ts" });
  const view = acknowledgementViewOf(t, error, "Could not pull the branch");
  expect(view?.title).toBe("Could not pull the branch");
  expect(view?.error).toBe(error);
  expect(view?.classification).toEqual({ kind: "actionable", recovery: "mergeConflict" });
});

it.each([
  ["a failure of no known kind", new Error("boom")],
  ["a code the application does not know", new CodedError("future", "future_code", {})],
])("gives no failure modal for %s, which is classified unexpected and belongs in an error alert", (_case, error) => {
  expect(acknowledgementViewOf(t, error, "Could not pull the branch")).toBeUndefined();
});
