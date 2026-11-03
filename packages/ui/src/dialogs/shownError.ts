import type { ShownFailure } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { Translate } from "../i18n/catalog";

/** What a dialog shows of a failed request: the daemon's refusal, or, for a source read again until it stopped changing
 * (which a request that reads no file never meets), the plain account of an unexpected reply. */
export function shownError(failure: ShownFailure, t: Translate): ErrorPresentation {
  return failure.kind === "failed" ? failure.message : t("error.unexpectedReply");
}
