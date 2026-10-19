// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { openPathTarget } from "./openPathTarget";

const file = (over: object) => ({ size: 1, kind: "text", media_type: null, text: "x", data: null, ...over });
const readReply = (over: object = {}) => ({ type: "project_file", project: "p", worktree: null, path: "a.ts", source: { kind: "live", version: "1" }, file: file(over) });

function click(target: Parameters<typeof openPathTarget>[0]["target"], answer: () => Promise<unknown>, viewFile: Parameters<typeof openPathTarget>[0]["viewFile"] = vi.fn()) {
  const request = vi.fn(async (body: { type: string }) => (body.type === "read_project_file" ? answer() : { type: "ack" })) as never;
  const toastError = vi.fn();
  return { done: openPathTarget({ target, project: "p", request, viewFile, toastError }), request: request as unknown as ReturnType<typeof vi.fn>, viewFile, toastError };
}
const target = { path: "/r/a.ts", line: 4, view: { worktree: "w", path: "a.ts" } };

it("reads a file with a view first and shows it in the viewer, with what was read, at its line", async () => {
  const { done, request, viewFile } = click(target, async () => readReply());
  await done;
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "read_project_file", project: "p", worktree: "w", path: "a.ts", slot: "viewer" });
  expect(viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "a.ts", line: 4, read: readReply() });
});

it("shows an image too, but opens other binary on the host", async () => {
  const image = click(target, async () => readReply({ kind: "binary", media_type: "image/png", text: null, data: "AA" }));
  await image.done;
  expect(image.viewFile).toHaveBeenCalledOnce();
  const binary = click(target, async () => readReply({ kind: "binary", media_type: null, text: null, data: "AA" }));
  await binary.done;
  expect(binary.viewFile).not.toHaveBeenCalled();
  expect(binary.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/a.ts" });
});

it("opens a file past the read cap on the host, and reports any other failed read", async () => {
  const big = click(target, () => Promise.reject(new DaemonRequestError("too large", "limit_exceeded", {})));
  await big.done;
  expect(big.viewFile).not.toHaveBeenCalled();
  expect(big.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/a.ts" });
  const failed = click(target, () => Promise.reject(new DaemonRequestError("gone", "file_not_found", {})));
  await failed.done;
  expect(failed.toastError).toHaveBeenCalledOnce();
  expect(failed.request).toHaveBeenCalledTimes(1);
});

it("shows a file the system could launch in the viewer unread, and never opens it on the host", async () => {
  const launchable = click({ ...target, viewerOnly: true }, async () => readReply());
  await launchable.done;
  expect(launchable.request).not.toHaveBeenCalled();
  expect(launchable.viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "a.ts", line: 4 });
});

it("does nothing for a file the system could launch when there is no viewer to show it", async () => {
  const request = vi.fn(async () => ({ type: "ack" })) as never;
  await openPathTarget({ target: { ...target, viewerOnly: true }, project: "p", request, viewFile: undefined, toastError: vi.fn() });
  expect(request).not.toHaveBeenCalled();
});

it("opens a path with no view on the host, and says when that fails", async () => {
  const plain = click({ path: "/tmp/b.png" }, async () => readReply());
  await plain.done;
  expect(plain.request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: "p", path: "/tmp/b.png" });
  const request = vi.fn(async () => { throw new Error("refused"); }) as never;
  const toastError = vi.fn();
  await openPathTarget({ target: { path: "/tmp/b.png" }, project: undefined, request, viewFile: undefined, toastError });
  expect(toastError).toHaveBeenCalledOnce();
});
