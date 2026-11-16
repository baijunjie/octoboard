import { BROWSE_SLOTS } from "../../browse/budget";
import type { RequestSender } from "../../requests";
import type { ViewedFile } from "../../viewer/viewedFile";
import { pathLinkRecovery } from "../../errors/errorRecovery";
import { raiseError } from "../../components/alerts/raiseError";
import { t } from "../../i18n/language";
import { bodyFromFileContent } from "../../viewer/content";
import type { PathLinkTarget } from "./pathLinks";

/**
 * What a click on a path link in the terminal does. A file always goes to the viewer, wherever it lies: a file of the
 * project or its worktrees from its place there, any other (in a temporary folder) from its path, which the viewer
 * reads itself and says what it cannot show. A file is never opened on the host from here, and with no viewer to show
 * it nothing happens. A folder with a `view` (its README or index file, shown without a line) is read first and shown
 * in the viewer with what was read when it is text or an image; one that is not, or is past the read cap, or whose
 * entry file cannot be read, is opened in the file manager instead, and a folder at once when `onHost` asks. A folder
 * with no `view` is opened in the file manager. Where `hostActions` is false (a remote window, whose host's file manager
 * is not on the user's screen) nothing is opened on the host: a folder's entry file goes to the viewer unread, which
 * says what it cannot show, and a folder with none does nothing. A failed read or open is reported; a folder gone
 * since the link was drawn gets the remedy of finding it in the project's files, when it was in the project rather
 * than a temporary folder.
 */
export async function openPathTarget({
  target,
  onHost = false,
  hostActions = true,
  project,
  request,
  viewFile,
}: {
  target: PathLinkTarget;
  /** The click asked for the path to be opened on the host; only a folder is moved by it. */
  onHost?: boolean;
  /** Whether a folder may be opened in the host's file manager; false in a remote window. */
  hostActions?: boolean;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: RequestSender;
  /** Shows a file in the viewer; absent where there is none. */
  viewFile: ((file: ViewedFile) => void) | undefined;
}): Promise<void> {
  const report = (error: unknown) => raiseError(error, { title: t("terminal.openPathFailedTitle"), recovery: pathLinkRecovery(error) });
  const openOnHost = () =>
    hostActions ? request({ type: "open_path_link", project, path: target.path }).then(() => undefined, report) : undefined;
  const { view } = target;
  const launchable = target.viewerOnly ? { launchable: true as const } : {};
  if (target.kind !== "directory") {
    if (!viewFile) return;
    const line = target.line;
    if (view && project) viewFile({ project, worktree: view.worktree, path: view.path, line, ...launchable });
    else viewFile({ link: target.path, project, line, ...launchable });
    return;
  }
  if (!view || !project) return openOnHost();
  const file: ViewedFile = { project, worktree: view.worktree, path: view.path };
  if (!hostActions) {
    viewFile?.(file);
    return;
  }
  if (!viewFile || onHost) return openOnHost();
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
  } catch {
    return openOnHost();
  }
}
