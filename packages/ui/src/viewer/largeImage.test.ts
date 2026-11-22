import { describe, expect, it, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import type { RangeFile } from "../protocol";
import { looseRequestSender } from "../requests";
import { daemonError } from "../test/daemonError";
import { decodeBase64, encodeBase64, fileRangeReply, sampleBytes, type RangeRequest } from "../test/fileRange";
import { BROWSE_SLOTS } from "../browse/budget";
import { MAX_IMAGE_BYTES, readLargeImage } from "./largeImage";

const FILE: RangeFile = { kind: "project", project: "p", path: "photo.png" };

/** The refusal a whole read of an image past the file budget is answered with. */
const refusal = (params: Record<string, string> = { limit: "file_bytes", max: "4194304", size: "9000000" }) => daemonError("limit_exceeded", params);

/** A sender serving `bytes` in ranges, recording every request it was sent. */
function serving(bytes: Uint8Array, options: { cap?: number; version?: string } = {}) {
  const sent: RangeRequest[] = [];
  const request = vi.fn(async (body: { type: string }) => {
    const range = body as RangeRequest;
    sent.push(range);
    return fileRangeReply(bytes, range, options);
  });
  return { sent, request, sender: looseRequestSender(request as never) };
}

const read = (over: Partial<Parameters<typeof readLargeImage>[0]> & { sender: Parameters<typeof readLargeImage>[0]["request"] }) =>
  readLargeImage({ refusal: refusal(), file: FILE, name: "photo.png", wanted: () => true, request: over.sender, ...over });

describe("readLargeImage", () => {
  it.each([
    { name: "a whole file in one part", length: 10, cap: 1024 },
    { name: "parts that are a multiple of three bytes", length: 100, cap: 9 },
    { name: "parts that leave one byte over", length: 100, cap: 7 },
    { name: "parts that leave two bytes over", length: 101, cap: 5 },
    { name: "parts of four bytes", length: 100, cap: 4 },
    { name: "a file whose last part is a single byte", length: 22, cap: 7 },
  ])("joins $name into the base64 of the whole file", async ({ length, cap }) => {
    const bytes = sampleBytes(length);
    const { sender } = serving(bytes, { cap });
    const { file, version } = await read({ sender });
    expect(file).toMatchObject({ kind: "binary", media_type: "image/png", size: length, text: null });
    // The text is the base64 of the file as a whole, not parts' texts with padding in between.
    expect(file.data).toBe(encodeBase64(bytes));
    expect(decodeBase64(file.data!)).toEqual(bytes);
    expect(version).toBe("v1");
  });

  it("reads a file larger than one part's size in parts and joins them", async () => {
    const bytes = sampleBytes(2.5 * 1024 * 1024 + 1);
    const { sender, sent } = serving(bytes);
    const { file } = await read({ sender });
    expect(sent.length).toBeGreaterThan(1);
    expect(file.size).toBe(bytes.length);
    // Compared as text: an element-by-element comparison of megabytes is slow.
    expect(file.data === encodeBase64(bytes)).toBe(true);
  });

  it("asks one part at a time in the viewer slot, from offset 0, pinning the first reply's version for the rest", async () => {
    const { sender, sent } = serving(sampleBytes(100), { cap: 12, version: "v7" });
    await read({ sender });
    expect(sent.length).toBeGreaterThan(2);
    expect(sent[0]).toMatchObject({ type: "read_file_range", file: FILE, offset: 0, slot: BROWSE_SLOTS.viewer });
    expect(sent[0]).not.toHaveProperty("version");
    for (const body of sent.slice(1)) expect(body).toMatchObject({ file: FILE, version: "v7", slot: BROWSE_SLOTS.viewer });
    // Each part asks where the bytes kept so far end: never past the file, never back to the start.
    const offsets = sent.map((body) => body.offset);
    expect(offsets).toEqual([...offsets].sort((a, b) => a - b));
    expect(new Set(offsets).size).toBe(offsets.length);
    expect(offsets.every((offset) => offset < 100)).toBe(true);
  });

  it("never has two parts out at once", async () => {
    const bytes = sampleBytes(60);
    let out = 0;
    let most = 0;
    const sender = looseRequestSender((async (body: RangeRequest) => {
      out += 1;
      most = Math.max(most, out);
      await Promise.resolve();
      out -= 1;
      return fileRangeReply(bytes, body, { cap: 6 });
    }) as never);
    await read({ sender });
    expect(most).toBe(1);
  });

  it("takes the version of the first reply as the result's", async () => {
    const bytes = sampleBytes(30);
    let n = 0;
    const sender = looseRequestSender((async (body: RangeRequest) => fileRangeReply(bytes, body, { cap: 9, version: `v${++n}` })) as never);
    expect((await read({ sender })).version).toBe("v1");
  });

  it("reads a path link's file the same way", async () => {
    const { sender, sent } = serving(sampleBytes(20), { cap: 8 });
    const link: RangeFile = { kind: "path_link", project: "p", path: "/tmp/x.png" };
    const { file } = await read({ sender, file: link, name: "/tmp/x.png" });
    expect(sent.every((body) => body.file === link)).toBe(true);
    expect(decodeBase64(file.data!)).toEqual(sampleBytes(20));
  });

  it("reads the whole file when the refusal gave no size", async () => {
    const bytes = sampleBytes(50);
    const { sender } = serving(bytes, { cap: 7 });
    const { file } = await read({ sender, refusal: refusal({ limit: "file_bytes", max: "4194304" }) });
    expect(decodeBase64(file.data!)).toEqual(bytes);
    expect(file.size).toBe(50);
  });

  it("reads a file the refusal put at exactly the most", async () => {
    const bytes = sampleBytes(10);
    const { sender } = serving(bytes);
    const { file } = await read({ sender, refusal: refusal({ limit: "file_bytes", max: "4194304", size: String(MAX_IMAGE_BYTES) }) });
    expect(decodeBase64(file.data!)).toEqual(bytes);
  });

  it.each([
    ["png", "image/png"],
    ["PNG", "image/png"],
    ["jpg", "image/jpeg"],
    ["jpeg", "image/jpeg"],
    ["JpEg", "image/jpeg"],
    ["gif", "image/gif"],
    ["webp", "image/webp"],
    ["bmp", "image/bmp"],
    ["ico", "image/x-icon"],
    ["avif", "image/avif"],
  ])("types a .%s file as %s", async (extension, mediaType) => {
    const { sender } = serving(sampleBytes(5));
    const { file } = await read({ sender, name: `dir.d/pic.${extension}` });
    expect(file.media_type).toBe(mediaType);
  });

  describe("rejects with the refusal itself and sends nothing", () => {
    it.each([
      { name: "a rejection that is not a coded error", error: new Error("boom"), image: "photo.png" },
      { name: "another code", error: daemonError("file_not_found", { path: "photo.png", root: "project" }), image: "photo.png" },
      { name: "another limit", error: daemonError("limit_exceeded", { limit: "reply_bytes", max: "1", size: "2" }), image: "photo.png" },
      { name: "a lost connection", error: new CodedError("closed", "connection_closed", {}), image: "photo.png" },
      { name: "an SVG", error: refusal(), image: "vector.svg" },
      { name: "an extensionless name", error: refusal(), image: "photo" },
      { name: "a name that only ends in an image's extension", error: refusal(), image: "png" },
      { name: "another file type", error: refusal(), image: "data.bin" },
      { name: "a file past the most", error: refusal({ limit: "file_bytes", max: "4194304", size: String(MAX_IMAGE_BYTES + 1) }), image: "photo.png" },
    ])("for $name", async ({ error, image }) => {
      const { sender, request } = serving(sampleBytes(10));
      await expect(read({ sender, refusal: error, name: image })).rejects.toBe(error);
      expect(request).not.toHaveBeenCalled();
    });
  });

  it("rejects with the refusal's code, message and params but the first part's size when that says the file is past the most", async () => {
    const sender = looseRequestSender((async (body: RangeRequest) => ({ ...fileRangeReply(sampleBytes(10), body), size: MAX_IMAGE_BYTES + 1 })) as never);
    const error = refusal();
    const rejected = await read({ sender, refusal: error }).catch((caught: unknown) => caught);
    expect(rejected).toBeInstanceOf(CodedError);
    expect(rejected).toMatchObject({ code: error.code, message: error.message, params: { ...error.params, size: String(MAX_IMAGE_BYTES + 1) } });
  });

  it("words the first part's size over the most when the refusal gave none", async () => {
    const sender = looseRequestSender((async (body: RangeRequest) => ({ ...fileRangeReply(sampleBytes(10), body), size: MAX_IMAGE_BYTES + 5 })) as never);
    const rejected = await read({ sender, refusal: refusal({ limit: "file_bytes", max: "4194304" }) }).catch((caught: unknown) => caught);
    expect(rejected).toMatchObject({ code: "limit_exceeded", params: { limit: "file_bytes", max: "4194304", size: String(MAX_IMAGE_BYTES + 5) } });
  });

  it("rejects with the refusal when a part comes back empty before the size is reached", async () => {
    const bytes = sampleBytes(40);
    let answered = 0;
    const sender = looseRequestSender((async (body: RangeRequest) => {
      answered += 1;
      return answered === 1 ? fileRangeReply(bytes, body, { cap: 12 }) : { ...fileRangeReply(bytes, body), data: "" };
    }) as never);
    const error = refusal();
    await expect(read({ sender, refusal: error })).rejects.toBe(error);
    expect(answered).toBe(2);
  });

  it("asks whether the read is still wanted before each part, and sends no part once it is not", async () => {
    const { sender, sent } = serving(sampleBytes(100), { cap: 12 });
    let asked = 0;
    const error = refusal();
    await expect(read({ sender, refusal: error, wanted: () => ++asked <= 2 })).rejects.toBe(error);
    expect(asked).toBe(3);
    expect(sent).toHaveLength(2);
  });

  it("sends no part at all when it is not wanted from the start", async () => {
    const { sender, request } = serving(sampleBytes(10));
    const error = refusal();
    await expect(read({ sender, refusal: error, wanted: () => false })).rejects.toBe(error);
    expect(request).not.toHaveBeenCalled();
  });

  it.each(["source_changed", "request_superseded"])("passes a part's own %s rejection on as it is, not as the refusal", async (code) => {
    const bytes = sampleBytes(40);
    const failure = daemonError(code, { path: "photo.png" });
    let answered = 0;
    const sender = looseRequestSender((async (body: RangeRequest) => {
      if (++answered === 2) throw failure;
      return fileRangeReply(bytes, body, { cap: 12 });
    }) as never);
    await expect(read({ sender })).rejects.toBe(failure);
  });

  it("passes on a closed connection of the first part as it is", async () => {
    const closed = new CodedError("closed", "connection_closed", {});
    const sender = looseRequestSender((async () => {
      throw closed;
    }) as never);
    await expect(read({ sender })).rejects.toBe(closed);
  });
});
