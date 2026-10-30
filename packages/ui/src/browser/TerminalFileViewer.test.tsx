// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import type { ViewerSubject } from "../viewer/content";
import { ViewerHost } from "../viewer/viewerTestSupport";
import { TerminalFileViewer, type ViewedFile } from "./TerminalFileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The viewer is the subject of other tests; this one looks at what the pane hands it.
type Location = { project?: string; anchor?: string; onOpenFile?: (path: string, fragment?: string) => void };
const shown = vi.hoisted(() => ({ props: undefined as undefined | { subject: ViewerSubject; line?: number; location?: Location } }));
vi.mock("../viewer/FileViewer", () => ({
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

const reply = (file: ViewedFile) => ({
  type: "project_file",
  id: null,
  project: file.project,
  worktree: file.worktree ?? null,
  path: file.path,
  source: { kind: "live", version: "1" },
  file: { size: 1, kind: "text", media_type: null, text: "x", data: null },
});

function mount(file: ViewedFile) {
  const request = vi.fn(async (body: { path: string; project: string; worktree?: string }) => reply({ ...file, path: body.path }) as never);
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(
    <ViewerHost request={request as never}>
      <TerminalFileViewer file={file} onClose={() => {}} onOpenFile={() => {}} />
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
      <TerminalFileViewer file={{ project: "p", path: "other.md", fragment: "usage" }} onClose={() => {}} onOpenFile={onOpenFile} />
    </ViewerHost>,
  ));
  await act(async () => {});
  expect(shown.props?.location?.anchor).toBe("usage");
  shown.props?.location?.onOpenFile?.("next.md", "install");
  expect(onOpenFile).toHaveBeenCalledExactlyOnceWith({ project: "p", path: "next.md", fragment: "install" });
});
