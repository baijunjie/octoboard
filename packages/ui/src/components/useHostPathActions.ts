import { DaemonRequestError } from "../daemon-client";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";

/** Where a file or folder lives on the daemon's host: in a project's directory, or in one of its
 * worktrees. `path` is a wire path below that directory. */
export interface HostPath {
  project: string;
  worktree?: string;
  path: string;
}

/** The codes of a file or folder that is not there any more. It is found again in the project's files, not by the
 * remedy of a missing path in general, which picks one with Browse in a form. */
const GONE = new Set(["file_not_found", "path_not_found"]);

/** What File manager and Default application do, on a row of the project pane and in the file
 * viewer's header alike: asks the daemon to reveal the live file or folder in the host's file
 * manager, or to open it with its default application, and raises an error alert when the host
 * refuses. A worktree's path is sent with the worktree's id. */
export function useHostPathActions(): (type: "open_project_path" | "reveal_project_path", host: HostPath) => void {
  const t = useT();
  const { request, toastError } = useDaemon();
  return (type, { project, worktree, path }) => {
    void request({ type, project, ...(worktree !== undefined && { worktree }), path }).catch((error: Error) => {
      const gone = error instanceof DaemonRequestError && GONE.has(error.code);
      toastError(error, undefined, t(type === "open_project_path" ? "common.openDefaultApplicationFailedTitle" : "common.revealFailedTitle"), gone ? "hostPathGone" : undefined);
    });
  };
}
