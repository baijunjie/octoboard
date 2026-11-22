// A daemon stand-in for `read_file_range`, shared by the tests of an image read in parts.
import type { ClientRequestBody, Event } from "../protocol";

/** A `read_file_range` request. */
export type RangeRequest = Extract<ClientRequestBody, { type: "read_file_range" }>;

/** The `file_range` reply of a daemon serving `bytes`: the range asked for, at most `cap` bytes of it (the real daemon
 * may answer fewer than asked), with the file's whole size and `version`. */
export function fileRangeReply(bytes: Uint8Array, body: RangeRequest, { cap = 1024 * 1024, version = "v1" }: { cap?: number; version?: string } = {}): Extract<Event, { type: "file_range" }> {
  const slice = bytes.subarray(body.offset, body.offset + Math.min(body.length, cap));
  return { type: "file_range", id: null, offset: body.offset, size: bytes.length, version, data: encodeBase64(slice) };
}

/** `bytes` as base64 text without line breaks, as the daemon sends it. */
export function encodeBase64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64");
}

/** The bytes of base64 text; throws on text that is not valid base64, such as padding in the middle. */
export function decodeBase64(data: string): Uint8Array {
  const bytes = Buffer.from(data, "base64");
  // Node's decoder skips what is not base64, so a text that does not read back as itself was not base64.
  if (bytes.toString("base64") !== data) throw new Error("not valid base64");
  return new Uint8Array(bytes);
}

/** `length` bytes that differ from one another's neighbours, so a dropped or repeated byte shows in a comparison. */
export function sampleBytes(length: number): Uint8Array {
  return Uint8Array.from({ length }, (_, index) => (index * 7 + 13) % 256);
}
