import { useEffect, useState } from "react";

import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { listProcesses } from "../processes";
import type { ProcessTree } from "../protocol";
import { useDaemon } from "../store";

/** The project processes (trees no session owns) removing the projects `projectIds` stops, read once from each
 * project's process list (one request per project) rather than from `inspect_project_removal`, which is asked
 * only while the Trash option is ticked and can be refused. Oldest first across the projects. `pending` holds
 * until every list has answered. A list that could not be read contributes no tree and is reported in `error`
 * (the first such failure), since what it holds is stopped all the same and must not read as "none". */
export function useProjectProcesses(projectIds: string[]): { trees: ProcessTree[]; pending: boolean; error: ErrorPresentation | undefined } {
  const { request } = useDaemon();
  const [answer, setAnswer] = useState<{ key: string; trees: ProcessTree[]; error?: ErrorPresentation }>();
  const key = JSON.stringify([...projectIds].sort());
  useEffect(() => {
    let active = true;
    void Promise.allSettled((JSON.parse(key) as string[]).map((id) => listProcesses(request, { project_id: id }))).then((lists) => {
      if (!active) return;
      const failed = lists.find((list): list is PromiseRejectedResult => list.status === "rejected");
      setAnswer({
        key,
        trees: lists.flatMap((list) => list.status === "fulfilled" ? list.value.filter((tree) => tree.session_id === undefined) : []).sort((a, b) => a.started_at - b.started_at),
        error: failed && errorPresentation(failed.reason),
      });
    });
    return () => { active = false; };
  }, [key, request]);
  const current = answer?.key === key ? answer : undefined;
  return { trees: current?.trees ?? [], pending: !current, error: current?.error };
}
