import { describe, expect, it } from "vitest";

import { textChange } from "./changeFixtures";

describe("a change fixture's factories", () => {
  // What keeps a fixture from being written half-filled: a kind's factory takes exactly what the
  // daemon's reply for it carries, so leaving part of it out is a type error. The omission below is
  // the demonstration — `@ts-expect-error` fails the type check the day that call becomes valid,
  // and the throw says nothing half-filled is built meanwhile.
  it("cannot write a text change without the versions its reply counts", () => {
    const blob = { kind: "index", worktree: "wt-gap", blob: "1111111" } as const;
    // @ts-expect-error a text change takes both whole versions and the patch between them
    expect(() => textChange("staged", "src/gap.ts", blob, blob)).toThrow();
  });
});
