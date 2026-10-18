import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { ErrorPresentation } from "../errorPresentation";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendBrowseListRequest } from "./browseListRequests";

export type FileSearch =
  | { state: "loading" }
  | { state: "loaded"; paths: string[]; complete: boolean }
  | { state: "error"; message: ErrorPresentation };

/** A debounced project-wide file-name search. A reply only belongs to the query that requested
 * it, even during the debounce before the next request supersedes it in the daemon. */
export function useFileSearch(project: string, query: string, active: boolean): { result: FileSearch; refresh: () => Promise<void> } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [held, setHeld] = useState<{ query: string; result: FileSearch }>();
  const live = useRef({ alive: true, query, sequence: 0, pending: 0 });
  useLayoutEffect(() => {
    live.current.query = query;
  });

  const send = (): Promise<void> => {
    if (!active) return Promise.resolve();
    const s = live.current;
    const sequence = ++s.sequence;
    s.pending += 1;
    const current = () => s.alive && s.query === query && s.sequence === sequence;
    const slot = `file-search:${project}`;
    setHeld((now) => now?.query === query && now.result.state === "loaded" ? now : { query, result: { state: "loading" } });
    // Searches share the bounded list-request lane with Git, so switching modes cannot add
    // unbounded browse requests beside the directory listings and the viewer.
    return sendBrowseListRequest(slot, () => current() ? request({ type: "search_project_files", project, query, slot }) : undefined)
      .then((reply) => {
        if (!current() || reply?.type !== "project_file_matches" || reply.project !== project || reply.query !== query) return;
        setHeld({ query, result: { state: "loaded", paths: reply.paths, complete: reply.complete } });
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const failure = browseFailure(t, language, err, store.getState());
        if (failure.kind === "superseded" || failure.kind === "disconnected") return;
        setHeld({ query, result: { state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.folderChanged") } });
      })
      .finally(() => { s.pending -= 1; });
  };

  useEffect(() => {
    if (!active) return;
    const timer = setTimeout(() => void send(), 250);
    return () => clearTimeout(timer);
  }, [query, active, snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => { s.alive = false; };
  }, []);

  return { result: held?.query === query ? held.result : { state: "loading" }, refresh: send };
}
