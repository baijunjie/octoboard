// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi, type MockInstance } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { CodedError } from "../../errors/codedError";
import { openPathTarget } from "./openPathTarget";

let report: MockInstance<typeof errorAlerts.report>;
beforeEach(() => { report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {}); });
afterEach(() => { report.mockRestore(); });

const file = (over: object) => ({ size: 1, kind: "text", media_type: null, text: "x", data: null, ...over });
const readReply = (over: object = {}) => ({ type: "project_file", id: null, project: "p", worktree: null, path: "a.ts", source: { kind: "live", version: "1" }, file: file(over) });

function click(target: Parameters<typeof openPathTarget>[0]["target"], answer: () => Promise<unknown>, viewFile: Parameters<typeof openPathTarget>[0]["viewFile"] = vi.fn()) {
  const request = vi.fn(async (body: { type: string }) => (body.type === "read_project_file" ? answer() : { type: "ack", id: null })) as never;
  return { done: openPathTarget({ target, project: "p", request, viewFile }), request: request as unknown as ReturnType<typeof vi.fn>, viewFile };
}
const target = { path: "/r/a.ts", kind: "file" as const, line: 4, view: { worktree: "w", path: "a.ts" } };

it("shows a file of the project in the viewer unread, at its line, and never opens it on the host", async () => {
  const { done, request, viewFile } = click(target, async () => readReply());
  await done;
  expect(request).not.toHaveBeenCalled();
  expect(viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "a.ts", line: 4 });
});

it("shows a file outside every project directory from its path, without reading or opening it", async () => {
  const { done, request, viewFile } = click({ path: "/tmp/b.png", kind: "file", line: 2 }, async () => readReply());
  await done;
  expect(request).not.toHaveBeenCalled();
  expect(viewFile).toHaveBeenCalledExactlyOnceWith({ link: "/tmp/b.png", project: "p", line: 2 });
});

it("marks a file the system could launch for the viewer, whichever kind of path names it", async () => {
  const project = click({ ...target, viewerOnly: true }, async () => readReply());
  await project.done;
  expect(project.request).not.toHaveBeenCalled();
  expect(project.viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "a.ts", line: 4, launchable: true });
  const temporary = click({ path: "/tmp/run.sh", kind: "file", viewerOnly: true }, async () => readReply());
  await temporary.done;
  expect(temporary.viewFile).toHaveBeenCalledExactlyOnceWith({ link: "/tmp/run.sh", project: "p", line: undefined, launchable: true });
});

it("does nothing for a file when there is no viewer to show it", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null })) as never;
  await openPathTarget({ target, project: "p", request, viewFile: undefined });
  expect(request).not.toHaveBeenCalled();
});

it("opens a folder with no view on the host, and says when that fails", async () => {
  const plain = click({ path: "/tmp/dir", kind: "directory" }, async () => readReply());
  await plain.done;
  expect(plain.request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: "p", path: "/tmp/dir" });
  const request = vi.fn(async () => { throw new Error("refused"); }) as never;
  await openPathTarget({ target: { path: "/tmp/dir", kind: "directory" }, project: undefined, request, viewFile: undefined });
  expect(report).toHaveBeenCalledOnce();
});

it("hands a rejection that is not an Error to the alert as it came, titled for the failed open", async () => {
  const request = vi.fn(async () => { throw "native refused"; }) as never;
  await openPathTarget({ target: { path: "/tmp/dir", kind: "directory" }, project: undefined, request, viewFile: undefined });
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.lastCall![0]).toMatchObject({ error: "native refused", title: "Could not open the path", recovery: undefined });
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
  const failed = click(folder, () => Promise.reject(new CodedError("gone", "file_not_found", {})));
  await failed.done;
  expect(failed.viewFile).not.toHaveBeenCalled();
  expect(report).not.toHaveBeenCalled();
  expect(failed.request).toHaveBeenLastCalledWith({ type: "open_path_link", project: "p", path: "/r/docs" });
});

it("opens a folder on the host without reading its entry file when asked to", async () => {
  const request = vi.fn(async (body: { type: string }) => (body.type === "read_project_file" ? readReply() : { type: "ack", id: null })) as never;
  const viewFile = vi.fn();
  await openPathTarget({ target: folder, onHost: true, project: "p", request, viewFile });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: "p", path: "/r/docs" });
  expect(viewFile).not.toHaveBeenCalled();
});

it("where nothing opens on the host, shows a folder's entry file unread whatever the click asked, and leaves a folder without one alone", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null })) as never;
  const viewFile = vi.fn();
  await openPathTarget({ target: folder, onHost: true, hostActions: false, project: "p", request, viewFile });
  expect(viewFile).toHaveBeenCalledExactlyOnceWith({ project: "p", worktree: "w", path: "docs/README.md" });
  await openPathTarget({ target: { path: "/r/empty", kind: "directory" }, hostActions: false, project: "p", request, viewFile });
  expect(request).not.toHaveBeenCalled();
  expect(viewFile).toHaveBeenCalledOnce();
});

it("reports a folder gone since it was probed, with the remedy of finding it in the project's files only for one in the project", async () => {
  const gone = (root?: string) => () => Promise.reject(new CodedError("gone", "file_not_found", { path: "x", ...(root && { root }) }));
  const onHost = (answer: () => Promise<unknown>) => vi.fn((body: { type: string }) => (body.type === "open_path_link" ? answer() : Promise.resolve({ type: "ack", id: null }))) as never;
  await openPathTarget({ target: { path: "/tmp/x", kind: "directory", line: undefined }, project: "p", request: onHost(gone("temporary")), viewFile: vi.fn() });
  expect(report.mock.lastCall![0].recovery).toBe("none");
  await openPathTarget({ target: { path: "docs", kind: "directory", line: undefined }, project: "p", request: onHost(gone("project")), viewFile: vi.fn() });
  expect(report.mock.lastCall![0].recovery).toBe("hostPathGone");
});
