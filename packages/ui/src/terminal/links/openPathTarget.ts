import { BROWSE_SLOTS } from "../../browse/budget";
import { CodedError } from "../../errors/codedError";
import type { RequestSender } from "../../requests";
import type { ViewedFile } from "../../viewer/viewedFile";
import { pathLinkRecovery } from "../../errors/errorRecovery";
import { raiseError } from "../../components/alerts/raiseError";
import { t } from "../../i18n/language";
import { bodyFromFileContent } from "../../viewer/content";
import type { PathLinkTarget } from "./pathLinks";

/**
 * What a click on a path link in the terminal does. A file the daemon marks `viewerOnly` (every file of the project or
 * its worktrees) goes to the viewer unread, which reads it and says what it cannot show; it is never opened on the host
 * from here, and with no viewer to show it nothing happens. Any other path with a `view` (a folder's README or index
 * file, shown without a line) is read first and shown in the viewer with what was read when it is text or an image;
 * one that is not, or is past the read cap, or a folder whose entry file cannot be read, is opened on the host
 * instead, and a folder at once when `onHost` asks. A path with no `view`, such as one in a temporary folder, is opened
 * on the host. A failed read or open is reported; a file or folder gone since the link was drawn gets the remedy of
 * finding it in the project's files, when it was in the project rather than a temporary folder.
 */
export async function openPathTarget({
  target,
  onHost = false,
  project,
  request,
  viewFile,
}: {
  target: PathLinkTarget;
  /** The click asked for the path to be opened on the host; only a folder is moved by it. */
  onHost?: boolean;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: RequestSender;
  /** Shows a file in the viewer; absent where there is none. */
  viewFile: ((file: ViewedFile) => void) | undefined;
}): Promise<void> {
  const report = (error: unknown) => raiseError(error, { title: t("terminal.openPathFailedTitle"), recovery: pathLinkRecovery(error) });
  const openOnHost = () =>
    request({ type: "open_path_link", project, path: target.path }).then(() => undefined, report);
  const { view } = target;
  const directory = target.kind === "directory";
  const line = directory ? undefined : target.line;
  // A viewer-only file (one of the project, or one the system could launch) is never opened on the host: with no
  // viewer to show it, nothing happens.
  if (target.viewerOnly) {
    if (view && project) viewFile?.({ project, worktree: view.worktree, path: view.path, line });
    return;
  }
  if (!view || !project || !viewFile || (onHost && directory)) return openOnHost();
  const file: ViewedFile = { project, worktree: view.worktree, path: view.path, line };
  try {
    const reply = await request({
      type: "read_project_file",
      project,
      ...(view.worktree !== undefined && { worktree: view.worktree }),
      path: view.path,
      slot: BROWSE_SLOTS.viewer,
    });
    // The viewer's own rule: text and the images it draws; any other binary is no use to it.
    if (bodyFromFileContent(view.path, reply.file).kind !== "binary") viewFile({ ...file, read: reply });
    else await openOnHost();
  } catch (error) {
    if (directory || (error instanceof CodedError && error.code === "limit_exceeded")) return openOnHost();
    report(error);
  }
}
