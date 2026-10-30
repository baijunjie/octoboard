// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { openPathTarget } from "./openPathTarget";

const file = (over: object) => ({ size: 1, kind: "text", media_type: null, text: "x", data: null, ...over });
const readReply = (over: object = {}) => ({ type: "project_file", id: null, project: "p", worktree: null, path: "a.ts", source: { kind: "live", version: "1" }, file: file(over) });

function click(target: Parameters<typeof openPathTarget>[0]["target"], answer: () => Promise<unknown>, viewFile: Parameters<typeof openPathTarget>[0]["viewFile"] = vi.fn()) {
  const request = vi.fn(async (body: { type: string }) => (body.type === "read_project_file" ? answer() : { type: "ack", id: null })) as never;
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

it("opens a file past the read cap on the host, and reports any other failed read, a gone file with the remedy of finding it again", async () => {
  const big = click(target, () => Promise.reject(new DaemonRequestError("too large", "limit_exceeded", {})));
  await big.done;
  expect(big.viewFile).not.toHaveBeenCalled();
  expect(big.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/a.ts" });
  const failed = click(target, () => Promise.reject(new DaemonRequestError("gone", "file_not_found", {})));
  await failed.done;
  expect(failed.toastError).toHaveBeenCalledOnce();
  expect(failed.toastError.mock.calls[0][3]).toBe("hostPathGone");
  expect(failed.request).toHaveBeenCalledTimes(1);
  const refused = click(target, () => Promise.reject(new DaemonRequestError("denied", "permission_denied", { path: "a.ts", detail: "x" })));
  await refused.done;
  expect(refused.toastError.mock.calls[0][3]).toBeUndefined();
});

it("reports a path outside every root (outside_scope) without the remedy of the browse views, whichever step of the click found it", async () => {
  const outside = () => Promise.reject(new DaemonRequestError("outside", "outside_scope", { path: "/r/a.ts" }));
  const toastError = vi.fn();
  await openPathTarget({ target: { path: "/r/a.ts" }, project: "p", request: outside as never, viewFile: undefined, toastError });
  expect(toastError.mock.calls[0][3]).toBe("none");
  const read = click(target, outside);
  await read.done;
  expect(read.toastError.mock.calls[0][3]).toBe("none");
});

it("shows a file the system could launch in the viewer unread, and never opens it on the host", async () => {
  const launchable = click({ ...target, viewerOnly: true }, async () => readReply());
  await launchable.done;
  expect(launchable.request).not.toHaveBeenCalled();
  expect(launchable.viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "a.ts", line: 4 });
});

it("does nothing for a file the system could launch when there is no viewer to show it", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null })) as never;
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

const folder = { path: "/r/docs", kind: "directory" as const, line: 4, view: { worktree: "w", path: "docs/README.md" } };

it("shows a folder's entry file in the viewer without a line", async () => {
  const { done, request, viewFile } = click(folder, async () => readReply());
  await done;
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "read_project_file", project: "p", worktree: "w", path: "docs/README.md", slot: "viewer" });
  expect(viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "docs/README.md", line: undefined, read: readReply() });
});

it("opens the folder itself on the host when its entry file cannot be shown or read", async () => {
  const binary = click(folder, async () => readReply({ kind: "binary", media_type: null, text: null, data: "AA" }));
  await binary.done;
  expect(binary.viewFile).not.toHaveBeenCalled();
  expect(binary.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/docs" });
  const failed = click(folder, () => Promise.reject(new DaemonRequestError("gone", "file_not_found", {})));
  await failed.done;
  expect(failed.viewFile).not.toHaveBeenCalled();
  expect(failed.toastError).not.toHaveBeenCalled();
  expect(failed.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/docs" });
});

it("opens a folder on the host without reading its entry file when asked to, but not a file", async () => {
  const request = vi.fn(async (body: { type: string }) => (body.type === "read_project_file" ? readReply() : { type: "ack", id: null })) as never;
  const viewFile = vi.fn();
  await openPathTarget({ target: folder, onHost: true, project: "p", request, viewFile, toastError: vi.fn() });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: "p", path: "/r/docs" });
  expect(viewFile).not.toHaveBeenCalled();
  await openPathTarget({ target: { ...target, kind: "file" }, onHost: true, project: "p", request, viewFile, toastError: vi.fn() });
  expect(viewFile).toHaveBeenCalledOnce();
});

it("reports a path gone since it was probed, with the remedy of finding it in the project's files only for one in the project", async () => {
  const gone = (root?: string) => () => Promise.reject(new DaemonRequestError("gone", "file_not_found", { path: "x", ...(root && { root }) }));
  const onHost = (answer: () => Promise<unknown>) => vi.fn((body: { type: string }) => (body.type === "open_path_link" ? answer() : Promise.resolve({ type: "ack", id: null }))) as never;
  const temporary = vi.fn();
  await openPathTarget({ target: { path: "/tmp/x.log", line: undefined }, project: "p", request: onHost(gone("temporary")), viewFile: vi.fn(), toastError: temporary });
  expect(temporary.mock.calls[0][3]).toBe("none");
  const folder = vi.fn();
  await openPathTarget({ target: { path: "docs", kind: "directory", line: undefined }, project: "p", request: onHost(gone("project")), viewFile: vi.fn(), toastError: folder });
  expect(folder.mock.calls[0][3]).toBe("hostPathGone");
  const read = vi.fn();
  await openPathTarget({ target, project: "p", request: vi.fn(gone()) as never, viewFile: vi.fn(), toastError: read });
  expect(read.mock.calls[0][3]).toBe("hostPathGone");
});
