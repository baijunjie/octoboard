import { listLane } from "../../browse/budget";
import { join } from "../../format/hostPath";
import { useT } from "../../i18n/react";
import { useDaemon } from "../../store";
import { useHostPaths } from "../hooks/useHostPaths";
import type { HostPath } from "./useHostPathActions";

/** Resolves the live directory on the daemon's host, written the way the host writes a path; worktree ids are
 * identities, not paths, and a project's directory can sit below the repository root. */
export function useResolveAbsolutePath(): (host: HostPath) => Promise<string> {
  const t = useT();
  const { request } = useDaemon();
  const { style } = useHostPaths();
  return async ({ project, worktree, path }) => {
    const { source } = await listLane.send(() => request({ type: "get_project_source", project }));
    let root = source.resolved_root;
    if (worktree !== undefined) {
      const tree = source.git?.worktrees.find((entry) => entry.id === worktree);
      if (!tree) throw new Error(t("common.copyFailed"));
      root = source.git!.scope ? join(tree.root, source.git!.scope, style) : tree.root;
    }
    return path ? join(root, path, style) : root;
  };
}
