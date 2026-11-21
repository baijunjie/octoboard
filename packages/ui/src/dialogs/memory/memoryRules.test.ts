import { expect, it } from "vitest";

import {
  hasMemoryContent,
  isValidMemorySummary,
  MEMORY_MAX_CONTENT_BYTES,
  MEMORY_MAX_SUMMARY_CHARS,
  MEMORY_MAX_ENTRIES,
  memoryContentBytes,
  shownWriter,
  writerAgent,
} from "./memoryRules";

it("takes a summary of one line of 1 to 200 characters", () => {
  expect(isValidMemorySummary(" ok ")).toBe(true);
  expect(isValidMemorySummary("x".repeat(200))).toBe(true);
  for (const summary of ["", "  ", "x".repeat(201), "two\nlines", "tab\there"]) expect(isValidMemorySummary(summary), JSON.stringify(summary)).toBe(false);
});

it("counts content in UTF-8 bytes and calls only whitespace blank", () => {
  expect(memoryContentBytes("あ")).toBe(3);
  expect(hasMemoryContent("  \n ")).toBe(false);
  expect(hasMemoryContent(" a ")).toBe(true);
});

it("reads a writer the client knows, and nothing for a newer daemon's", () => {
  expect(shownWriter({ kind: "user" })).toBe("user");
  expect(shownWriter({ kind: "agent" })).toBe("agent");
  expect(shownWriter({ kind: "somebody" } as never)).toBeUndefined();
});

it("reads the agent an agent's entry names, and none for the user or an agent's entry that names none", () => {
  expect(writerAgent({ kind: "agent", agent: "codex" })).toBe("codex");
  expect(writerAgent({ kind: "agent" })).toBeUndefined();
  expect(writerAgent({ kind: "user" })).toBeUndefined();
});

it("holds the limits the daemon holds an entry to ", () => {
  expect([MEMORY_MAX_ENTRIES, MEMORY_MAX_SUMMARY_CHARS, MEMORY_MAX_CONTENT_BYTES]).toEqual([200, 200, 16384]);
});

it("counts a summary's characters by code point, so a letter outside the BMP counts once", () => {
  expect(isValidMemorySummary("𠀀".repeat(200))).toBe(true);
  expect(isValidMemorySummary("𠀀".repeat(201))).toBe(false);
});

it("refuses a summary holding any control character", () => {
  for (const summary of ["bell\u0007", "nul\u0000", "line\rbreak", "del\u007f"]) expect(isValidMemorySummary(summary), JSON.stringify(summary)).toBe(false);
});
