// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { ViewerSubject } from "../../viewer/content";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import type { ViewedFile, ViewedProjectFile } from "../../viewer/viewedFile";
import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { LinkedFileViewer } from "./LinkedFileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The viewer is the subject of other tests; this one looks at what the pane hands it.
type Location = { project?: string; anchor?: string; onOpenFile?: (path: string, fragment?: string) => void };
const shown = vi.hoisted(() => ({ props: undefined as undefined | { subject: ViewerSubject; line?: number; location?: Location } }));
vi.mock("../../viewer/FileViewer", () => ({
  FileViewer: (props: { subject: ViewerSubject; line?: number; location?: Location }) => {
    shown.props = props;
    return <div data-viewer="" />;
  },
}));

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  shown.props = undefined;
});

const reply = (file: ViewedProjectFile) => ({
  type: "project_file",
  id: null,
  project: file.project,
  worktree: file.worktree ?? null,
  path: file.path,
  source: { kind: "live", version: "1" },
  file: { size: 1, kind: "text", media_type: null, text: "x", data: null },
});

function mount(file: ViewedProjectFile) {
  const request = vi.fn(async (body: { path: string; project: string; worktree?: string }) => reply({ ...file, path: body.path }) as never);
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(
    <ViewerHost request={request as never}>
      <LinkedFileViewer file={file} onClose={() => {}} onOpenFile={() => {}} />
    </ViewerHost>,
  ));
  return request;
}

it("reads a worktree's file from that worktree and gives the viewer its line and place on the host", async () => {
  const request = mount({ project: "p", worktree: "w", path: "src/a.ts", line: 7 });
  await act(async () => {});
  expect(request).toHaveBeenCalledWith(expect.objectContaining({ type: "read_project_file", project: "p", worktree: "w", path: "src/a.ts" }));
  expect(shown.props?.line).toBe(7);
  expect(shown.props?.subject.host).toEqual({ project: "p", worktree: "w", path: "src/a.ts" });
  expect(shown.props?.subject.content.state).toBe("file");
  // A worktree's documents do not follow their links into the project's own directory.
  expect(shown.props?.location).toBeUndefined();
});

it("reads a file of the project's own directory with no worktree and lets its documents link on", async () => {
  const request = mount({ project: "p", path: "a.md" });
  await act(async () => {});
  expect(request.mock.calls[0][0]).not.toHaveProperty("worktree");
  expect(shown.props?.location).toMatchObject({ project: "p" });
});

it("starts from what was read before the click and does not read it again", async () => {
  const read = reply({ project: "p", path: "a.ts" });
  const request = mount({ project: "p", path: "a.ts", read: read as never });
  expect(shown.props?.subject.content.state).toBe("file");
  await act(async () => {});
  expect(request).not.toHaveBeenCalled();
  expect(shown.props?.subject.content.state).toBe("file");
});

it("hands a followed link's heading to the document, and asks for the file with it", async () => {
  const onOpenFile = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(
    <ViewerHost request={(async (body: { path: string }) => reply({ project: "p", path: body.path })) as never}>
      <LinkedFileViewer file={{ project: "p", path: "other.md", fragment: "usage" }} onClose={() => {}} onOpenFile={onOpenFile} />
    </ViewerHost>,
  ));
  await act(async () => {});
  expect(shown.props?.location?.anchor).toBe("usage");
  shown.props?.location?.onOpenFile?.("next.md", "install");
  expect(onOpenFile).toHaveBeenCalledExactlyOnceWith({ project: "p", path: "next.md", fragment: "install" });
});

const linkReply = (path: string, version = "1", over: object = {}) => ({
  type: "path_link_file",
  id: null,
  path,
  version,
  file: { size: 1, kind: "text", media_type: null, text: "x", data: null, ...over },
});

type Handler = (body: { type: string; path: string; project?: string; slot?: string }) => Promise<unknown>;

function mountAny(file: ViewedFile, handler: Handler) {
  const request = vi.fn(handler);
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  const render = (next: ViewedFile) => act(() => root!.render(
    <ViewerHost request={request as never}>
      <LinkedFileViewer file={next} onClose={() => {}} onOpenFile={() => {}} />
    </ViewerHost>,
  ));
  render(file);
  return { request, render };
}

it("reads a file outside every project directory through the daemon's path link rules, never opening it on the host", async () => {
  const { request } = mountAny({ link: "/tmp/out/a.txt", project: "p", line: 5 }, async (body) => linkReply(body.path));
  await act(async () => {});
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "read_path_link", project: "p", path: "/tmp/out/a.txt", slot: "viewer" });
  const subject = shown.props!.subject;
  expect(subject.content).toMatchObject({ state: "file", body: { kind: "text", text: "x" } });
  // The path stays as the terminal wrote it, and says it is the daemon host's; it is no file of a project.
  expect(subject.path).toBe("/tmp/out/a.txt");
  expect(subject.linkHost).toEqual({ project: "p", path: "/tmp/out/a.txt" });
  expect(subject.host).toBeUndefined();
  expect(shown.props!.line).toBe(5);
  // No project around it: a relative link in it stays its text.
  expect(shown.props!.location).toBeUndefined();
});

it("sends no project for a session that has none", async () => {
  const { request } = mountAny({ link: "/tmp/out/a.txt" }, async (body) => linkReply(body.path));
  await act(async () => {});
  expect(request.mock.calls[0][0]).not.toHaveProperty("project");
  expect(shown.props!.subject.linkHost).toEqual({ project: undefined, path: "/tmp/out/a.txt" });
});

it("hands the daemon's word that the system could launch the file on to the viewer, for a linked file and a project one", async () => {
  mountAny({ link: "/tmp/run.sh", launchable: true }, async (body) => linkReply(body.path));
  await act(async () => {});
  expect(shown.props!.subject.launchable).toBe(true);
  act(() => root!.unmount());
  mountAny({ project: "p", path: "tool", launchable: true }, async (body) => reply({ project: "p", path: body.path }) as never);
  await act(async () => {});
  expect(shown.props!.subject.launchable).toBe(true);
  act(() => root!.unmount());
  mountAny({ link: "/tmp/a.txt" }, async (body) => linkReply(body.path));
  await act(async () => {});
  expect(shown.props!.subject.launchable).toBeUndefined();
});

it("says in the viewer why a linked file cannot be read, and raises no error alert", async () => {
  const { request } = mountAny({ link: "/tmp/gone.txt" }, async () => { throw new CodedError("gone", "file_not_found", { path: "/tmp/gone.txt", root: "temporary" }); });
  await act(async () => {});
  expect(request).toHaveBeenCalledOnce();
  expect(shown.props!.subject.content.state).toBe("error");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it.each([
  { name: "a linked file", file: { link: "/tmp/big.bin" } as ViewedFile, handler: (params: Record<string, string>): Handler => async () => { throw new CodedError("too large", "limit_exceeded", params); } },
  { name: "a project file", file: { project: "p", path: "big.bin" } as ViewedFile, handler: (params: Record<string, string>): Handler => async () => { throw new CodedError("too large", "limit_exceeded", params); } },
])("shows $name past what a read delivers as a stand-in with its size, not as an error", async ({ file, handler }) => {
  mountAny(file, handler({ limit: "file_bytes", max: "4194304", size: "9000000" }));
  await act(async () => {});
  expect(shown.props!.subject.content).toEqual({ state: "file", body: { kind: "tooLarge", size: 9000000 } });
  act(() => root!.unmount());
  mountAny(file, handler({ limit: "file_bytes", max: "4194304" }));
  await act(async () => {});
  expect(shown.props!.subject.content).toEqual({ state: "file", body: { kind: "tooLarge", size: null } });
});

it("shows another limit's failure as an error, since only a file past the file budget has a stand-in", async () => {
  mountAny({ link: "/tmp/x" }, async () => { throw new CodedError("busy", "limit_exceeded", { limit: "pending_requests", max: "16" }); });
  await act(async () => {});
  expect(shown.props!.subject.content.state).toBe("error");
});

it("takes no answer for a file the viewer has moved on from", async () => {
  const answers = new Map<string, (reply: unknown) => void>();
  const { render } = mountAny({ link: "/tmp/first.txt" }, (body) => new Promise((resolve) => answers.set(body.path, resolve)));
  render({ link: "/tmp/second.txt" });
  await act(async () => answers.get("/tmp/second.txt")!(linkReply("/tmp/second.txt", "1", { text: "second" })));
  await act(async () => answers.get("/tmp/first.txt")!(linkReply("/tmp/first.txt", "1", { text: "first" })));
  expect(shown.props!.subject.path).toBe("/tmp/second.txt");
  expect(shown.props!.subject.content).toMatchObject({ state: "file", body: { text: "second" } });
});

it("shows a file it is asked for again, at another line, without reading anew when the version is the same", async () => {
  const { request, render } = mountAny({ link: "/tmp/a.txt", line: 1 }, async (body) => linkReply(body.path));
  await act(async () => {});
  render({ link: "/tmp/a.txt", line: 9 });
  await act(async () => {});
  expect(shown.props!.line).toBe(9);
  expect(shown.props!.subject.content.state).toBe("file");
  expect(request.mock.calls.every(([body]) => body.type === "read_path_link")).toBe(true);
});

it("shows a file whose reply is in hand before the render that names it", async () => {
  const request = mount({ project: "p", path: "src/a.ts" });
  await act(async () => {});
  expect(request).toHaveBeenCalledOnce();
  expect(shown.props?.subject.content.state).toBe("file");
});

it("shows a file outside every project from its path, read through the daemon", async () => {
  const request = vi.fn(async () => ({ type: "path_link_file", id: null, path: "/tmp/b.txt", version: "1", file: { size: 1, kind: "text", media_type: null, text: "x", data: null } }) as never);
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(
    <ViewerHost request={request as never}>
      <LinkedFileViewer file={{ link: "/tmp/b.txt", project: "p" }} onClose={() => {}} onOpenFile={() => {}} />
    </ViewerHost>,
  ));
  await act(async () => {});
  expect(request).toHaveBeenCalledWith(expect.objectContaining({ type: "read_path_link", project: "p", path: "/tmp/b.txt" }));
  expect(shown.props?.subject.linkHost).toEqual({ project: "p", path: "/tmp/b.txt" });
  expect(shown.props?.subject.content.state).toBe("file");
});
