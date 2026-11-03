import { expect, it } from "vitest";

import { parseStored } from "./browserPreferences";

it("reads the Changes tab's layout", () => {
  const raw = JSON.stringify({ p: { mode: "git", layouts: { changes: "tree" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({ changes: "tree" });
});

it("drops a layout it does not know", () => {
  const raw = JSON.stringify({ p: { mode: "git", layouts: { changes: "grid" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({});
});
