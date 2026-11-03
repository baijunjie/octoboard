import { expect, it } from "vitest";

import { forgetSessionFollowsOtherThan, noteSessionSwitch, sessionFollow } from "./sessionFollow";

it("keeps one record per project for one selection of a session", () => {
  noteSessionSwitch();
  const follow = sessionFollow("p", "s");
  follow.settled = true;
  expect(sessionFollow("p", "s")).toBe(follow);
  expect(sessionFollow("q", "s")).not.toBe(follow);
});

it("starts afresh for another session or a later selection of the same one", () => {
  noteSessionSwitch();
  const first = sessionFollow("p", "s");
  first.settled = true;
  const other = sessionFollow("p", "t");
  expect(other).not.toBe(first);
  expect(other.settled).toBe(false);
  noteSessionSwitch();
  const again = sessionFollow("p", "t");
  expect(again).not.toBe(other);
  expect(again.switchId).toBeGreaterThan(other.switchId);
  expect(again.since).toBeGreaterThan(other.since);
  expect(again.sourceSince).toBe(again.since);
});

it("forgets the records of projects that are gone", () => {
  noteSessionSwitch();
  const kept = sessionFollow("kept", "s");
  const gone = sessionFollow("gone", "s");
  forgetSessionFollowsOtherThan(new Set(["kept"]));
  expect(sessionFollow("kept", "s")).toBe(kept);
  expect(sessionFollow("gone", "s")).not.toBe(gone);
});
