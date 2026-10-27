import { DaemonRequestError } from "./daemon-client";
import { daemonMessage, DETAIL_BLOCK_CODES, type RecordNames } from "./daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, isUncataloguedFailure, uncataloguedSummaryKey, type ErrorPresentation } from "./errorPresentation";
import type { Translate } from "./i18n/catalog";
import type { Language } from "./i18n/languages";

/** The plain text of a failure (a request's rejection or its presentation), for a surface that
 * cannot render the structured one. */
export function errorText(error: ErrorPresentation | Error): string {
  return typeof error === "string" ? error : error.message;
}

/** The sentence an Alert's description says for `error`, as plain text for a surface that cannot render it
 * (a status region), for a failure whose summary is a single catalogue sentence: not the raw text, the remedy or the
 * details. A failure the application has no words for is its generic sentence (see `uncataloguedSummaryKey`). Not for the failures whose plain
 * wording carries more than that sentence (`daemonMessage` appends a sharing failure's items, cause and detail, and
 * `project_branches_not_determined`'s detail; a `launch_failed` fills its reason with the nested failure's full
 * message, a trust diagnostic with the whole trust reason). */
export function failureSummaryText(t: Translate, language: Language, error: ErrorPresentation, records: RecordNames): string {
  if (typeof error !== "string" && isUncataloguedFailure(error)) return t(uncataloguedSummaryKey(error));
  if (typeof error === "string") return error;
  if (hasShellTimeoutDetails(error)) return t("daemon.shell_environment_timeout.cause");
  return hasCodeParams(error) ? daemonMessage(language, error.code, error.params, error.message, records, true) : error.message;
}

/** A stack led by `heading` (the error's name and message, or its message): as it is where it already starts or
 * ends its first line with it, else with the heading put before it, WebKit's stacks holding frames only. */
export function stackLedBy(heading: string, stack: string | undefined): string {
  if (!stack) return heading;
  return stack.startsWith(heading) || stack.split("\n", 1)[0].endsWith(heading) ? stack : `${heading}\n${stack}`;
}

/** The text of a thrown error for a diagnostic block: its stack, led by its name and message (see `stackLedBy`). */
export function errorDiagnostic(error: { name: string; message: string; stack?: string }): string {
  return stackLedBy(error.message ? `${error.name}: ${error.message}` : error.name, error.stack);
}

/** What a diagnostic block keeps of an unexpected failure shown without its message: a failure the daemon coded is its
 * message; a wrapped JavaScript error is its message and then its stack, the message put first when the stack lacks it
 * (see `stackLedBy`). */
export function unexpectedFailureDiagnostic(error: { code: string; message: string; stack?: string }): string {
  if (error.code !== "unexpected_error" || !error.stack) return error.message;
  return stackLedBy(error.message, error.stack);
}

/** The raw text of exactly two kinds of failure, for a diagnostic block of its own: the daemon's `detail` of a code whose
 * sentence leaves it out (`DETAIL_BLOCK_CODES`), and the message of an uncatalogued failure, the application having no
 * words for it (see `unexpectedFailureDiagnostic`). Undefined for any other failure, whose sentence is all it says; its
 * raw text, where it has some (a sharing failure's entries, a trust diagnostic, a nested cause), is rendered by
 * `DaemonErrorMessage` with `part="raw"`. Raw text carries no bidirectional isolates, which belong to localized text. */
export function rawFailureText(error: ErrorPresentation): string | undefined {
  if (typeof error === "string") return undefined;
  const raw = error.params.detail && DETAIL_BLOCK_CODES.has(error.code) ? error.params.detail : isUncataloguedFailure(error) ? unexpectedFailureDiagnostic(error) : undefined;
  return raw?.replace(/[\u2066-\u2069]/g, "");
}

/** What to keep of a failure for a diagnostic block: for a failure the daemon coded, the daemon's own text (see
 * `unexpectedFailureDiagnostic`), whose stack would only be this window's; for anything else its stack when it has one,
 * else its message. */
export function diagnosticText(error: unknown): string | undefined {
  if (typeof error === "string") return error;
  if (error instanceof DaemonRequestError) return unexpectedFailureDiagnostic(error);
  if (!error || typeof error !== "object") return error === undefined || error === null ? undefined : String(error);
  try {
    if ("stack" in error && typeof error.stack === "string" && error.stack) return error.stack;
    if ("message" in error && typeof error.message === "string") return error.message;
  } catch {
    return undefined;
  }
  return undefined;
}
