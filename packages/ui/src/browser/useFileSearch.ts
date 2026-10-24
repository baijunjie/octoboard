import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { ErrorPresentation } from "../errorPresentation";
import { useCurrentLanguage, useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendBrowseListRequest } from "./browseListRequests";

export type FileSearch =
  | { state: "loading" }
  | { state: "loaded"; paths: string[]; /** The `paths` Git ignores, as the daemon marks them. */ ignored: ReadonlySet<string>; complete: boolean }
  | { state: "error"; message: ErrorPresentation };

/** A debounced project-wide file-name search, leaving out the files Git ignores unless
 * `includeIgnored`; changing that repeats the search at once rather than after the debounce. A reply
 * only belongs to the query and the setting that requested it, even during the debounce before the
 * next request supersedes it in the daemon. */
export function useFileSearch(project: string, query: string, active: boolean, includeIgnored = false): { result: FileSearch; refresh: () => Promise<void> } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [held, setHeld] = useState<{ query: string; includeIgnored: boolean; result: FileSearch }>();
  const live = useRef({ alive: true, query, includeIgnored, sequence: 0, pending: 0 });
  useLayoutEffect(() => {
    live.current.query = query;
    live.current.includeIgnored = includeIgnored;
  });
  // The setting the last search sent was made with, to tell a toggle from typing. Only `send` and
  // an inactive effect write it, so an effect that runs twice (StrictMode) reads the same answer
  // both times; a toggle made with no search to repeat leaves the next keystroke debounced.
  const sentWith = useRef(includeIgnored);

  const send = (): Promise<void> => {
    if (!active) return Promise.resolve();
    const s = live.current;
    sentWith.current = includeIgnored;
    const sequence = ++s.sequence;
    s.pending += 1;
    const current = () => s.alive && s.query === query && s.includeIgnored === includeIgnored && s.sequence === sequence;
    const slot = `file-search:${project}`;
    setHeld((now) => now?.query === query && now.includeIgnored === includeIgnored && now.result.state === "loaded" ? now : { query, includeIgnored, result: { state: "loading" } });
    // Searches share the bounded list-request lane with Git, so switching modes cannot add
    // unbounded browse requests beside the directory listings and the viewer.
    return sendBrowseListRequest(slot, () => current() ? request({ type: "search_project_files", project, query, ...(includeIgnored && { include_ignored: true }), slot }) : undefined)
      .then((reply) => {
        if (!current() || reply?.type !== "project_file_matches" || reply.project !== project || reply.query !== query) return;
        setHeld({ query, includeIgnored, result: { state: "loaded", paths: reply.paths, ignored: new Set(reply.ignored), complete: reply.complete } });
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const failure = browseFailure(t, language, err, store.getState());
        if (failure.kind === "superseded" || failure.kind === "disconnected") return;
        setHeld({ query, includeIgnored, result: { state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.folderChanged") } });
      })
      .finally(() => { s.pending -= 1; });
  };

  useEffect(() => {
    if (!active) {
      sentWith.current = includeIgnored;
      return;
    }
    const toggled = sentWith.current !== includeIgnored;
    const timer = setTimeout(() => void send(), toggled ? 0 : 250);
    return () => clearTimeout(timer);
  }, [query, active, snapshotEpoch, includeIgnored]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => { s.alive = false; };
  }, []);

  return { result: held?.query === query && held.includeIgnored === includeIgnored ? held.result : { state: "loading" }, refresh: send };
}
