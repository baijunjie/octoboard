import { listLane } from "../../browse/budget";
import { useT } from "../../i18n/react";
import { useDaemon } from "../../store";
import type { HostPath } from "./useHostPathActions";

/** Resolves the live directory on the daemon's host; worktree ids are identities, not paths,
 * and a project's directory can sit below the repository root. */
export function useResolveAbsolutePath(): (host: HostPath) => Promise<string> {
  const t = useT();
  const { request } = useDaemon();
  return async ({ project, worktree, path }) => {
    const { source } = await listLane.send(() => request({ type: "get_project_source", project }));
    let root = source.resolved_root;
    if (worktree !== undefined) {
      const tree = source.git?.worktrees.find((entry) => entry.id === worktree);
      if (!tree) throw new Error(t("common.copyFailed"));
      root = source.git!.scope ? `${tree.root.replace(/\/$/, "")}/${source.git!.scope}` : tree.root;
    }
    return path ? `${root.replace(/\/$/, "")}/${path}` : root;
  };
}
