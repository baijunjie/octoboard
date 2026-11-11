import type { Page } from "../protocol";
import { applyDaemonEvent, type Daemon } from "../store";

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const isPageContent = (value: unknown): value is Page["content"] => isRecord(value) && Array.isArray(value.body);
const isSubmission = (value: unknown): value is Page["submission"] => value === null || isRecord(value);

/** A report-only browser probe entry, installed by the gallery frame, never the application: a document event
 * whose detail is a whole page, which the store takes as a `page_created`. The page is checked for the shape the
 * protocol gives it, a component tree and a submission, before it is applied. */
export function installReportPageFixture(document: Document, daemon: Daemon, scenarioId: string): () => void {
  if (!scenarioId.startsWith("report-")) return () => {};
  const receive = (event: Event) => {
    const page: unknown = (event as CustomEvent<unknown>).detail;
    if (!page || typeof page !== "object") return;
    const candidate = page as Partial<Page>;
    if (typeof candidate.id !== "string" || typeof candidate.session_id !== "string"
      || !isPageContent(candidate.content) || !isSubmission(candidate.submission)
      || typeof candidate.created_at !== "number" || !Number.isFinite(candidate.created_at)) return;
    const state = daemon.store.getState();
    if (!state.pages.has(candidate.session_id) || !state.sessions.has(candidate.session_id)
      || state.pages.get(candidate.session_id)?.some((existing) => existing.id === candidate.id)) return;
    applyDaemonEvent(daemon.store, { type: "page_created", page: candidate as Page });
  };
  document.addEventListener("gallery:report-page-created", receive);
  return () => document.removeEventListener("gallery:report-page-created", receive);
}
