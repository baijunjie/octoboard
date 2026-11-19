import { expect, it } from "vitest";

import { EMPTY_MEMORY, indexLine, staleContents, withChange, withDeleted, withListing, withRead, withSaved } from "./memoryState";
import { deletedEvent, entryOf, savedEvent } from "../../fixtures/memoryBuilders";

const ids = (memory: { index: readonly { id: string }[] }) => memory.index.map((line) => line.id);

it("keeps the index most recently updated first as entries are saved, and holds their content", () => {
  let memory = withSaved(EMPTY_MEMORY, entryOf("a", { updated_at: 1 }));
  memory = withSaved(memory, entryOf("b", { updated_at: 3 }));
  memory = withSaved(memory, entryOf("a", { updated_at: 5, version: 2, content: "new" }));
  expect(ids(memory)).toEqual(["a", "b"]);
  expect(memory.contents.get("a")?.content).toBe("new");
  expect(memory.index[0]).toEqual(indexLine(entryOf("a", { updated_at: 5, version: 2, content: "new" })));
});

it("drops deleted entries and their content", () => {
  const memory = withDeleted(withSaved(withSaved(EMPTY_MEMORY, entryOf("a")), entryOf("b")), ["a"]);
  expect(ids(memory)).toEqual(["b"]);
  expect(memory.contents.has("a")).toBe(false);
});

it("takes a read's content unless something newer is held or the entry is gone", () => {
  const held = withSaved(EMPTY_MEMORY, entryOf("a", { version: 2, content: "two" }));
  expect(withRead(held, entryOf("a", { version: 1, content: "one" }))).toBe(held);
  expect(withRead(EMPTY_MEMORY, entryOf("a"))).toBe(EMPTY_MEMORY);
  const listed = withListing(EMPTY_MEMORY, [indexLine(entryOf("a", { version: 2 }))], []);
  expect(withRead(listed, entryOf("a", { version: 2, content: "two" })).contents.get("a")?.content).toBe("two");
});

it("puts the changes broadcast while a listing was out over it, without stepping back to an older save", () => {
  const listed = [indexLine(entryOf("a", { version: 3 })), indexLine(entryOf("gone"))];
  const memory = withListing(EMPTY_MEMORY, listed, [
    savedEvent(entryOf("a", { version: 2, summary: "older" })),
    savedEvent(entryOf("b", { version: 1 })),
    deletedEvent(["gone"]),
  ]);
  expect(ids(memory).sort()).toEqual(["a", "b"]);
  expect(memory.index.find((line) => line.id === "a")?.version).toBe(3);
});

it("keeps held content of entries a listing still has, and names the ones it found changed", () => {
  const before = withSaved(withSaved(EMPTY_MEMORY, entryOf("a")), entryOf("b"));
  const after = withListing(before, [indexLine(entryOf("a", { version: 2 })), indexLine(entryOf("b"))], []);
  expect(after.contents.get("a")?.version).toBe(1);
  expect(staleContents(after)).toEqual(["a"]);
  expect(withListing(before, [indexLine(entryOf("b"))], []).contents.has("a")).toBe(false);
});

it("applies a broadcast change by its kind", () => {
  const saved = withChange(EMPTY_MEMORY, savedEvent(entryOf("a")));
  expect(ids(saved)).toEqual(["a"]);
  expect(ids(withChange(saved, deletedEvent(["a"])))).toEqual([]);
});
