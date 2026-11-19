import { onTestFinished, vi } from "vitest";

import type { ClientRequestBody, ContentMatch, Event } from "../../../protocol";
import { createContentSearchListeners, type ContentSearchEvent, type State } from "../../../store";
import type { RequestHandler } from "../../../requests";
import { testDaemon } from "../../../test/testDaemon";

type SearchRequest = Extract<ClientRequestBody, { type: "search_project_content" }>;

/** A daemon stand-in for content searches: every `search_project_content` is recorded and acknowledged (or answered by
 * `refuse`, which rejects it instead), and the test streams events to the latest search with `matches` and `end`. Other
 * requests go to `request`, rejected by default. */
export function contentSearchDaemon({ state, request = () => Promise.reject(new Error("no daemon")), refuse }: {
  state?: Partial<State>;
  request?: RequestHandler;
  refuse?: (body: SearchRequest) => Error | undefined;
} = {}) {
  const listeners = createContentSearchListeners();
  const searches: SearchRequest[] = [];
  const handler = vi.fn(async (body: ClientRequestBody): Promise<Event> => {
    if (body.type !== "search_project_content") return request(body);
    searches.push(body);
    const refusal = refuse?.(body);
    if (refusal) throw refusal;
    return { type: "ack", id: null };
  });
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1, ...state }, request: handler, onContentSearch: listeners.subscribe });
  const latest = () => {
    const search = searches.at(-1);
    if (!search) throw new Error("no search was sent");
    return search;
  };
  const emit = (event: ContentSearchEvent) => listeners.deliver(event);
  return {
    daemon,
    handler,
    searches,
    latest,
    /** A batch of the latest search's matching lines, or of the search `search` names. */
    matches: (matches: ContentMatch[], search = latest().search) => emit({ type: "project_content_matches", search, project: latest().project, matches }),
    /** The latest search's end, or that of the search `search` names. */
    end: (ended: { matches: number; files: number; complete?: boolean; stopped_by?: string }, search = latest().search) =>
      emit({ type: "project_content_search_ended", search, project: latest().project, complete: true, ...ended }),
  };
}

/** A matching line, its match being `query`'s first occurrence in `text`. */
export function contentMatch(path: string, line: number, text: string, query: string): ContentMatch {
  const start = text.indexOf(query);
  return { path, line, text, text_start: 0, ranges: [{ start, end: start + query.length }], clipped_end: false };
}

/** What the modal reads of the page that jsdom lacks: `matchMedia`, and the breakpoints `style.css` declares, which tell
 * it whether to fill the screen: every width query matches in a `narrow` window. Taken off again when the test ends. */
export function modalPage({ narrow = false }: { narrow?: boolean } = {}): void {
  const media = window.matchMedia;
  window.matchMedia = ((query: string) => ({ matches: narrow && query.includes("width"), media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  onTestFinished(() => {
    window.matchMedia = media;
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
}

