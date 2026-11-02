import { useCallback } from "react";

import type { ErrorPresentation } from "../errors/errorPresentation";
import { listAllProcesses } from "./processes";
import type { ProcessCount } from "../protocol";
import { useDaemon } from "../store";
import { usePolled, type ProcessPolling } from "./usePolled";

/** How many trees each project and console session has, asked for at the `polling` pace while the caller is mounted
 * and not while the document is hidden; `undefined` until the first answer. A later failure keeps the counts and is
 * `error`. */
export function useAllProcesses(polling: ProcessPolling): {
  counts: ProcessCount[] | undefined;
  error: ErrorPresentation | undefined;
} {
  const { request } = useDaemon();
  const list = useCallback(() => listAllProcesses(request), [request]);
  const { value, error } = usePolled("all", list, polling);
  return { counts: value, error };
}
