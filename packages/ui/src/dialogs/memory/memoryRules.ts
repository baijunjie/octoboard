import type { MemoryWriter } from "../../protocol";

// The rules of an entry of a memory, as the daemon holds it to them (`coordinator/memory.rs`): the
// form checks the same so that a refusal is told next to the field rather than in an alert.

/** The most entries one memory holds. */
export const MEMORY_MAX_ENTRIES = 200;
/** The most characters of an entry's summary, after trimming. */
export const MEMORY_MAX_SUMMARY_CHARS = 200;
/** The most bytes of UTF-8 an entry's content holds. */
export const MEMORY_MAX_CONTENT_BYTES = 16 * 1024;

/** Whether `summary` is one the daemon accepts: 1 to 200 characters after trimming, with no line break or other
 * control character. */
export function isValidMemorySummary(summary: string): boolean {
  const trimmed = summary.trim();
  const length = Array.from(trimmed).length;
  return length >= 1 && length <= MEMORY_MAX_SUMMARY_CHARS && !/\p{Cc}/u.test(trimmed);
}

/** Whether `content` has some text in it. */
export function hasMemoryContent(content: string): boolean {
  return content.trim() !== "";
}

/** How many bytes `content` takes in UTF-8, which is what the daemon limits. */
export function memoryContentBytes(content: string): number {
  return new TextEncoder().encode(content).length;
}

/** Who wrote an entry last: an agent (any session of any agent) or the user. */
export type ShownWriter = "agent" | "user";

/** `writer` as a `ShownWriter`, or `undefined` for a kind this client does not know (a newer daemon's), whose entry is
 * shown with no writer. A tagged union is not narrowed by the types against such a kind, so it is handled here, by
 * hand. */
export function shownWriter(writer: MemoryWriter): ShownWriter | undefined {
  const kind: string = writer.kind;
  return kind === "user" || kind === "agent" ? kind : undefined;
}
