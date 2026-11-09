import { useEffect, useRef } from "react";

import { listLane } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import { useDaemon } from "../../store";

/** How long the file filter waits for the last keystroke before it searches. */
export const SEARCH_DEBOUNCE_MS = 250;

export type FileSearch =
  | { state: "loading" }
  | { state: "loaded"; paths: string[]; /** The `paths` Git ignores, as the daemon marks them. */ ignored: ReadonlySet<string>; complete: boolean }
  | { state: "error"; message: ErrorPresentation };

/** A debounced project-wide file-name search, leaving out the files Git ignores unless
 * `includeIgnored`; changing that repeats the search at once rather than after the debounce. A reply
 * only belongs to the query and the setting that requested it, even during the debounce before the
 * next request supersedes it in the daemon. A reconnect searches again at once. */
export function useFileSearch(project: string, query: string, active: boolean, includeIgnored = false): { result: FileSearch; refresh: () => Promise<void> } {
  const { request } = useDaemon();
  const latest = useLatestRequest<FileSearch>({
    scope: `${includeIgnored}\0${query}`,
    initial: { state: "loading" },
    project,
    onReconnect: () => void send(),
  });
  // The setting the last search sent was made with, to tell a toggle from typing. Only `send` and
  // an inactive effect write it, so an effect that runs twice (StrictMode) reads the same answer
  // both times; a toggle made with no search to repeat leaves the next keystroke debounced.
  const sentWith = useRef(includeIgnored);

  const send = (): Promise<void> => {
    if (!active) return Promise.resolve();
    sentWith.current = includeIgnored;
    const slot = `file-search:${project}`;
    // Searches share the bounded list-request lane with Git, so switching modes cannot add
    // unbounded browse requests beside the directory listings and the viewer.
    return latest.send({
      lane: listLane,
      slot,
      request: () => request({ type: "search_project_files", project, query, ...(includeIgnored && { include_ignored: true }), slot }),
      pending: (result) => (result.state === "loaded" ? result : { state: "loading" }),
      accept: (reply) =>
        reply.project === project && reply.query === query
          ? () => ({ state: "loaded", paths: reply.paths, ignored: new Set(reply.ignored), complete: reply.complete })
          : undefined,
      failed: (failure, _held, t) => ({ state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.folderChanged") }),
    });
  };

  useEffect(() => {
    if (!active) {
      sentWith.current = includeIgnored;
      return;
    }
    const toggled = sentWith.current !== includeIgnored;
    const timer = setTimeout(() => void send(), toggled ? 0 : SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, active, includeIgnored]);

  return { result: latest.value, refresh: send };
}
