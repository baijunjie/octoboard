import { useEffect, useRef } from "react";

import { BROWSE_SLOTS } from "../../../browse/budget";
import { useLatestRequest } from "../../../components/hooks/useLatestRequest";
import { CodedError } from "../../../errors/codedError";
import { CODE } from "../../../errors/codes";
import type { ErrorPresentation } from "../../../errors/errorPresentation";
import type { ContentMatch, ContentSearchStop } from "../../../protocol";
import { useDaemon, type ContentSearchEvent } from "../../../store";
import { knownValues } from "../../../values/knownValue";
import { SEARCH_DEBOUNCE_MS } from "../useFileSearch";

/** What a content search looks for, besides its query. */
export interface ContentSearchOptions {
  matchCase: boolean;
  wholeWord: boolean;
  regex: boolean;
  /** Comma-separated globs a file must match one of, `!` leaving out what a glob matches; empty for every file. */
  fileMask: string;
  /** Whether the files Git ignores are searched too. */
  includeIgnored: boolean;
}

export const DEFAULT_CONTENT_SEARCH_OPTIONS: ContentSearchOptions = { matchCase: false, wholeWord: false, regex: false, fileMask: "", includeIgnored: false };

/** The matches of a search so far, or all of them once it has ended. */
export interface ContentSearchFound {
  state: "searching" | "done";
  /** The matching lines in the order they arrived, which is no particular order. */
  matches: readonly ContentMatch[];
  /** Every match in `matches`' lines, a line holding one for each of its ranges. */
  matchCount: number;
  /** The files `matches` are in. */
  fileCount: number;
  /** Once `done`: whether every file was searched. */
  complete: boolean;
  /** Once `done`: the bound that ended an incomplete search, absent for one that could not read some files. */
  stoppedBy?: ContentSearchStop;
}

export type ContentSearch =
  /** The query is empty: nothing is searched. */
  | { state: "idle" }
  | ContentSearchFound
  /** The daemon refused the query (`query`, not a valid regular expression) or the file mask (`fileMask`). */
  /** `error` is the refusal as `DaemonErrorMessage` renders it, worded from its `cause` (the matcher's own English
   * explanation, its `detail`, is never shown). */
  | { state: "invalid"; field: "query" | "fileMask"; error: ErrorPresentation }
  | { state: "error"; message: ErrorPresentation };

const IDLE: ContentSearch = { state: "idle" };
const STARTED: ContentSearchFound = { state: "searching", matches: [], matchCount: 0, fileCount: 0, complete: true };

/** A name for each search this window starts, unique for the connection it goes over (every event of a search carries
 * it). Module state: two searches of the window must never share one. */
let searchCount = 0;

function nextSearchName(): string {
  searchCount += 1;
  return `content-search-${searchCount}`;
}

/** `found` with a batch of lines added, the files counted among those already counted. */
function withBatch(found: ContentSearchFound, batch: readonly ContentMatch[], files: Set<string>): ContentSearchFound {
  let matchCount = found.matchCount;
  for (const match of batch) {
    matchCount += match.ranges.length;
    files.add(match.path);
  }
  return { ...found, matches: [...found.matches, ...batch], matchCount, fileCount: files.size };
}

/** Whether a received `stopped_by` names a bound this client knows; one it does not know reads, as `apps/daemon/PROTOCOL.md`
 * states, as an incomplete search with no bound named. */
const isContentSearchStop = knownValues<ContentSearchStop>({ time: true, matches: true, bytes_read: true, result_bytes: true });

/** What `event` makes of what is held for its search. */
function applyEvent(held: ContentSearch, event: ContentSearchEvent, files: Set<string>): ContentSearch {
  if (held.state !== "searching") return held;
  if (event.type === "project_content_matches") return withBatch(held, event.matches, files);
  return { ...held, state: "done", matchCount: event.matches, fileCount: event.files, complete: event.complete, ...(isContentSearchStop(event.stopped_by) && { stoppedBy: event.stopped_by }) };
}

/** The key of everything a search's answer depends on. */
function scopeOf(project: string, directory: string | undefined, query: string, options: ContentSearchOptions): string {
  return JSON.stringify([project, directory ?? null, query, options.matchCase, options.wholeWord, options.regex, options.fileMask, options.includeIgnored]);
}

/** The key of what a toggle changes, to tell it from typing. */
function togglesOf(options: ContentSearchOptions): string {
  return `${options.matchCase}${options.wholeWord}${options.regex}${options.includeIgnored}`;
}

/**
 * A debounced search of the contents of `project`'s files, or of those under `directory` (a wire path), streamed from
 * the daemon (`search_project_content`; see "Searching file contents" in `apps/daemon/PROTOCOL.md`): the request is
 * acknowledged, then batches of matching lines arrive, and the search ends with the counts and whether it is complete.
 * Typing (the query, the file mask) searches once the last keystroke is `SEARCH_DEBOUNCE_MS` old; a toggle searches at
 * once. Only the latest search's events are taken: the one before is let go of as its scope changes, and the daemon
 * stops it as the next one in the window's one slot arrives. An empty query searches nothing. A reconnect searches
 * again, since the daemon keeps nothing of a search across connections; `retry` does so on request.
 */
export function useContentSearch({ project, directory, query, options }: {
  project: string;
  directory?: string;
  query: string;
  options: ContentSearchOptions;
}): { result: ContentSearch; retry: () => Promise<void> } {
  const { request, onContentSearch } = useDaemon();
  const scope = scopeOf(project, directory, query, options);
  const latest = useLatestRequest<ContentSearch>({ scope, initial: STARTED, project, onReconnect: () => void send() });
  // The events of the search out, until it ends or is let go of.
  const listening = useRef<() => void>(undefined);
  // The toggles the last search was sent with, to tell a toggle from typing. Only `send` and the effect of an empty
  // query write it, so an effect that runs twice (StrictMode) reads the same answer both times.
  const sentWith = useRef(togglesOf(options));

  const stopListening = () => {
    listening.current?.();
    listening.current = undefined;
  };

  const send = (): Promise<void> => {
    stopListening();
    if (query === "") return Promise.resolve();
    sentWith.current = togglesOf(options);
    const search = nextSearchName();
    const files = new Set<string>();
    // Subscribed before the request is sent, so no event can arrive unheard; the scope is the one asked for.
    const unsubscribe = onContentSearch(search, (event) => {
      latest.hold((held) => applyEvent(held, event, files), scope);
      if (event.type === "project_content_search_ended") unsubscribe();
    });
    listening.current = unsubscribe;
    return latest.send({
      scope,
      slot: BROWSE_SLOTS.contentSearch,
      request: () =>
        request({
          type: "search_project_content",
          project,
          search,
          query,
          slot: BROWSE_SLOTS.contentSearch,
          ...(options.matchCase && { match_case: true }),
          ...(options.wholeWord && { whole_word: true }),
          ...(options.regex && { regex: true }),
          ...(options.fileMask.trim() !== "" && { file_mask: options.fileMask }),
          ...(options.includeIgnored && { include_ignored: true }),
          ...(directory !== undefined && { directory }),
        }).catch((err: unknown) => {
          // A refused pattern carries the query it was sent with, so the failure is worded counting where it fails as the
          // user sees it (`daemonParams`), not in the daemon's UTF-16 units.
          throw err instanceof CodedError && err.code === CODE.invalid_search_pattern ? new CodedError(err.message, err.code, { ...err.params, query }) : err;
        }),
      pending: () => STARTED,
      // The ack only says the search started; what it finds comes as events.
      accept: () => (held) => held,
      failed: (failure, _held, t): ContentSearch => {
        unsubscribe();
        if (failure.kind === "failed" && (failure.code === CODE.invalid_search_pattern || failure.code === CODE.invalid_file_mask)) {
          return { state: "invalid", field: failure.code === CODE.invalid_search_pattern ? "query" : "fileMask", error: failure.message };
        }
        return { state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.folderChanged") };
      },
    });
  };

  useEffect(() => {
    if (query === "") {
      sentWith.current = togglesOf(options);
      return;
    }
    const toggled = sentWith.current !== togglesOf(options);
    const timer = setTimeout(() => void send(), toggled ? 0 : SEARCH_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      stopListening();
    };
  }, [scope]);

  return { result: query === "" ? IDLE : latest.value, retry: send };
}
