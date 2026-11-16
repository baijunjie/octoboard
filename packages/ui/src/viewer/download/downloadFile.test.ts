import { expect, it, vi } from "vitest";

import type { RemoteFilesCapability } from "../../platform";
import type { RequestSender } from "../../requests";
import { BROWSE_SLOTS } from "../../browse/budget";
import { downloadFile, DownloadCancelled } from "./downloadFile";

const FILE = { kind: "project", project: "p", path: "a.bin" } as const;

/** A daemon holding `bytes`, answering `read_file_range` in parts of at most `part` bytes at `version`. */
function daemon(bytes: Uint8Array, { part = 4, launchable = false, versions }: { part?: number; launchable?: boolean; versions?: string[] } = {}) {
  const asked: { offset: number; length: number; version?: string }[] = [];
  const request = vi.fn((body: { offset: number; length: number; version?: string }) => {
    asked.push({ offset: body.offset, length: body.length, version: body.version });
    const version = versions?.[asked.length - 1] ?? "v1";
    if (body.version !== undefined && body.version !== version) return Promise.reject(Object.assign(new Error("changed"), { code: "source_changed" }));
    const slice = bytes.slice(body.offset, body.offset + Math.min(body.length, part));
    return Promise.resolve({ type: "file_range", id: null, offset: body.offset, size: bytes.length, version, data: btoa(String.fromCharCode(...slice)), ...(launchable && { launchable }) });
  });
  return { request: request as unknown as RequestSender, asked };
}

function shell() {
  const stored: number[][] = [];
  const files = {
    begin: vi.fn(() => Promise.resolve("copy")),
    write: vi.fn((_id: string, part: Uint8Array) => {
      stored.push([...part]);
      return Promise.resolve();
    }),
    open: vi.fn(() => Promise.resolve()),
    discard: vi.fn(() => Promise.resolve()),
  } satisfies RemoteFilesCapability;
  return { files, stored };
}

const run = (d: ReturnType<typeof daemon>, files: RemoteFilesCapability, signal = new AbortController().signal, onProgress = vi.fn()) =>
  downloadFile({ file: FILE, name: "a.bin", request: d.request, files, onProgress, signal });

it("learns the size from a request for no bytes, then copies the parts in order, each pinned to that version, and opens the copy", async () => {
  const bytes = Uint8Array.from({ length: 10 }, (_, i) => i);
  const d = daemon(bytes);
  const { files, stored } = shell();
  const onProgress = vi.fn();
  await run(d, files, undefined, onProgress);
  expect(stored.flat()).toEqual([...bytes]);
  expect(d.asked).toEqual([
    { offset: 0, length: 0, version: undefined },
    { offset: 0, length: 1024 * 1024, version: "v1" },
    { offset: 4, length: 1024 * 1024, version: "v1" },
    { offset: 8, length: 1024 * 1024, version: "v1" },
  ]);
  expect(files.begin).toHaveBeenCalledWith({ name: "a.bin", size: 10, launchable: false });
  expect(onProgress.mock.calls).toEqual([[4, 10], [8, 10], [10, 10]]);
  expect(files.open).toHaveBeenCalledWith("copy");
  expect(files.discard).not.toHaveBeenCalled();
});

it("opens an empty file without a part to write", async () => {
  const { files } = shell();
  await run(daemon(new Uint8Array()), files);
  expect(files.write).not.toHaveBeenCalled();
  expect(files.open).toHaveBeenCalled();
});

it("tells the shell the daemon's word that the file is launchable, and ends the copy at the shell's refusal, before a byte is asked for", async () => {
  const { files } = shell();
  files.begin.mockRejectedValueOnce(Object.assign(new Error(""), { kind: "launchable" }));
  const d = daemon(new Uint8Array([1]), { launchable: true });
  await expect(run(d, files)).rejects.toMatchObject({ kind: "launchable" });
  expect(files.begin).toHaveBeenCalledWith({ name: "a.bin", size: 1, launchable: true });
  expect(d.asked).toEqual([{ offset: 0, length: 0, version: undefined }]);
  expect(files.write).not.toHaveBeenCalled();
  expect(files.discard).not.toHaveBeenCalled();
});

it("leaves it to the shell to refuse what its name says: the shell's refusal ends the copy with nothing to discard", async () => {
  const { files } = shell();
  files.begin.mockRejectedValueOnce(Object.assign(new Error(""), { kind: "launchable" }));
  await expect(run(daemon(new Uint8Array([1])), files)).rejects.toMatchObject({ kind: "launchable" });
  expect(files.write).not.toHaveBeenCalled();
  expect(files.discard).not.toHaveBeenCalled();
});

it("discards the copy when a part fails because the file changed", async () => {
  const { files } = shell();
  await expect(run(daemon(new Uint8Array(10), { versions: ["v1", "v2"] }), files)).rejects.toMatchObject({ code: "source_changed" });
  expect(files.discard).toHaveBeenCalledWith("copy");
  expect(files.open).not.toHaveBeenCalled();
});

it("discards the copy when a write fails", async () => {
  const { files } = shell();
  files.write.mockRejectedValueOnce(Object.assign(new Error("full"), { kind: "storage" }));
  await expect(run(daemon(new Uint8Array(10)), files)).rejects.toMatchObject({ kind: "storage" });
  expect(files.discard).toHaveBeenCalledWith("copy");
});

it("is cancelled between parts, discarding what arrived", async () => {
  const { files } = shell();
  const abort = new AbortController();
  files.write.mockImplementationOnce(() => {
    abort.abort();
    return Promise.resolve();
  });
  await expect(run(daemon(new Uint8Array(10)), files, abort.signal)).rejects.toBeInstanceOf(DownloadCancelled);
  expect(files.discard).toHaveBeenCalledWith("copy");
  expect(files.open).not.toHaveBeenCalled();
});

it("stores nothing when it is cancelled before the first part arrives", async () => {
  const { files } = shell();
  const abort = new AbortController();
  abort.abort();
  await expect(run(daemon(new Uint8Array(10)), files, abort.signal)).rejects.toBeInstanceOf(DownloadCancelled);
  expect(files.begin).not.toHaveBeenCalled();
});

it("asks for parts of at most a mebibyte, in a slot of its own, for the file it was given", async () => {
  const d = daemon(Uint8Array.from({ length: 10 }, (_, i) => i));
  const { files } = shell();
  await downloadFile({ file: { kind: "path_link", project: "p", path: "/tmp/a.bin" }, name: "a.bin", request: d.request, files, onProgress: vi.fn(), signal: new AbortController().signal });
  const calls = (d.request as unknown as ReturnType<typeof vi.fn>).mock.calls.map(([body]) => body as { type: string; file: unknown; length: number; slot: string });
  expect(calls.length).toBeGreaterThan(1);
  for (const body of calls) {
    expect(body.type).toBe("read_file_range");
    expect(body.file).toEqual({ kind: "path_link", project: "p", path: "/tmp/a.bin" });
    expect(body.length).toBeLessThanOrEqual(1024 * 1024);
    expect(body.slot).toBe(BROWSE_SLOTS.download);
  }
  expect(BROWSE_SLOTS.download).not.toBe(BROWSE_SLOTS.viewer);
});

it("discards a copy that arrived whole when the shell cannot open it, and says why", async () => {
  const { files } = shell();
  files.open.mockRejectedValueOnce(Object.assign(new Error("no application"), { kind: "open" }));
  await expect(run(daemon(new Uint8Array(6)), files)).rejects.toMatchObject({ kind: "open" });
  expect(files.discard).toHaveBeenCalledExactlyOnceWith("copy");
});

it("discards the copy when it is cancelled after the last part is written, and does not open it", async () => {
  const { files } = shell();
  const abort = new AbortController();
  files.write.mockImplementation(() => {
    abort.abort();
    return Promise.resolve();
  });
  await expect(run(daemon(new Uint8Array(3)), files, abort.signal)).rejects.toBeInstanceOf(DownloadCancelled);
  expect(files.open).not.toHaveBeenCalled();
  expect(files.discard).toHaveBeenCalledWith("copy");
});

it("is cancelled while a part is on its way, without storing it", async () => {
  const { files } = shell();
  const abort = new AbortController();
  const request = vi.fn(async () => {
    abort.abort();
    return { type: "file_range", id: null, offset: 0, size: 3, version: "v1", data: btoa("abc") };
  }) as unknown as RequestSender;
  await expect(downloadFile({ file: FILE, name: "a.bin", request, files, onProgress: vi.fn(), signal: abort.signal })).rejects.toBeInstanceOf(DownloadCancelled);
  expect(files.begin).not.toHaveBeenCalled();
  expect(files.write).not.toHaveBeenCalled();
});
