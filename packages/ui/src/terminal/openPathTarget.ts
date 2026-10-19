import { DaemonRequestError } from "../daemon-client";
import type { Event, RequestBody } from "../protocol";
import type { ViewedFile } from "../browser/TerminalFileViewer";
import { bodyFromFileContent } from "../viewer/content";
import type { PathLinkTarget } from "./pathLinks";

/**
 * What a click on a path link in the terminal does. A file in the project or one of its worktrees
 * is read first and shown in the file viewer, with what was read, when it is text or an image, so
 * the viewer opens with its content; one that is not, or is past the read cap, is opened on the
 * host like any other path. A file the system could launch (`viewerOnly`) goes to the viewer
 * unread, which says what it cannot show, and is never opened on the host. A read that fails is
 * reported.
 */
export async function openPathTarget({
  target,
  project,
  request,
  viewFile,
  toastError,
}: {
  target: PathLinkTarget;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: (body: RequestBody) => Promise<Event>;
  /** Shows a file in the viewer; absent where there is none. */
  viewFile: ((file: ViewedFile) => void) | undefined;
  toastError: (error: Error | string) => void;
}): Promise<void> {
  const report = (error: unknown) => toastError(error instanceof Error ? error : String(error));
  const openOnHost = () =>
    request({ type: "open_path_link", project, path: target.path }).then(() => undefined, report);
  const { view } = target;
  // A file the system could launch is for the viewer alone: with none to show it, nothing happens.
  if (target.viewerOnly) {
    if (view && project) viewFile?.({ project, worktree: view.worktree, path: view.path, line: target.line });
    return;
  }
  if (!view || !project || !viewFile) return openOnHost();
  const file: ViewedFile = { project, worktree: view.worktree, path: view.path, line: target.line };
  try {
    const reply = await request({
      type: "read_project_file",
      project,
      ...(view.worktree !== undefined && { worktree: view.worktree }),
      path: view.path,
      slot: "viewer",
    });
    // The viewer's own rule: text and the images it draws; any other binary is no use to it.
    if (reply.type === "project_file" && bodyFromFileContent(view.path, reply.file).kind !== "binary") viewFile({ ...file, read: reply });
    else await openOnHost();
  } catch (error) {
    if (error instanceof DaemonRequestError && error.code === "limit_exceeded") return openOnHost();
    report(error);
  }
}
