import { expect, it } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { sessionToRestore } from "./lastSessionPerConsole";

const s1 = sessionOf("s1", "c1", "p1", "One", "idle");
const sessions = new Map([[s1.id, s1]]);

it.each([
  ["the session last selected in the console", new Map([["c1", "s1"]]), "c1", s1],
  ["nothing when none was selected", new Map(), "c1", undefined],
  ["nothing when it is gone", new Map([["c1", "gone"]]), "c1", undefined],
  ["nothing when it belongs to another console", new Map([["c2", "s1"]]), "c2", undefined],
])("brings back %s", (_name, last, consoleId, expected) => {
  expect(sessionToRestore(last, consoleId, sessions)).toBe(expected);
});
