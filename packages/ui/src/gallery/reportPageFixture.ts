import type { Page } from "../protocol";
import { applyDaemonEvent, type Daemon } from "../store";

/** A report-only browser probe entry, installed by the gallery frame, never the application.
 * DOM events keep opaque report frames out of this path; no postMessage handler accepts it. */
export function installReportPageFixture(document: Document, daemon: Daemon, scenarioId: string): () => void {
  if (!scenarioId.startsWith("report-")) return () => {};
  const receive = (event: Event) => {
    const page: unknown = (event as CustomEvent<unknown>).detail;
    if (!page || typeof page !== "object") return;
    const candidate = page as Partial<Page>;
    if (typeof candidate.id !== "string" || typeof candidate.console_session_id !== "string"
      || typeof candidate.html !== "string" || typeof candidate.created_at !== "number"
      || !Number.isFinite(candidate.created_at)) return;
    const state = daemon.store.getState();
    if (!state.pages.has(candidate.console_session_id) || !state.sessions.has(candidate.console_session_id)
      || state.pages.get(candidate.console_session_id)?.some((existing) => existing.id === candidate.id)) return;
    applyDaemonEvent(daemon.store, { type: "page_created", page: candidate as Page });
  };
  document.addEventListener("gallery:report-page-created", receive);
  return () => document.removeEventListener("gallery:report-page-created", receive);
}
