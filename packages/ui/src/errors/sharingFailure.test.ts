import { expect, it } from "vitest";

import { parseSharingItems, sharingFixKey, sharingReasonKey } from "./sharingFailure";

const entry = { item: "skills/", left: "unchanged", detail: "denied" };

it.each(["permission_denied", "busy", "not_found", "no_space", "conflict", "other"])("reads an entry's cause %s as itself", (cause) => {
  expect(parseSharingItems([{ ...entry, cause }])?.[0]?.cause).toBe(cause);
});

it.each([
  ["default_moved, which only a change as a whole fails with", "default_moved"],
  ["one the client does not know", "from_a_newer_daemon"],
  ["no cause", undefined],
  ["a cause that is not a string", 7],
  ["a null cause", null],
])("reads an entry with %s as other", (_name, cause) => {
  const items = parseSharingItems([{ ...entry, ...(cause === undefined ? {} : { cause }) }]);
  expect(items).toHaveLength(1);
  expect(items?.[0]?.cause).toBe("other");
});

it("names no reason line for other, and one for each cause a reader can act on", () => {
  expect(sharingReasonKey("other")).toBeUndefined();
  for (const cause of ["permission_denied", "busy", "not_found", "no_space", "conflict", "default_moved"] as const) {
    expect(sharingReasonKey(cause)).toBe(`daemon.sharing.reason.${cause}`);
  }
});

it("names a fix for the causes that have one, and none for not_found and default_moved", () => {
  for (const cause of ["permission_denied", "busy", "no_space", "conflict"] as const) {
    expect(sharingFixKey(cause)).toBe(`daemon.sharing.fix.${cause}`);
  }
  expect(sharingFixKey("not_found")).toBeUndefined();
  expect(sharingFixKey("default_moved")).toBeUndefined();
  expect(sharingFixKey("other")).toBe("daemon.sharing.fix.other");
  expect(sharingFixKey("other", true)).toBe("daemon.sharing.fix.otherBelow");
});
