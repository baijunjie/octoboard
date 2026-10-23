import { daemonMessage, type RecordNames } from "./daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, isUncataloguedFailure, type ErrorPresentation } from "./errorPresentation";
import type { Translate } from "./i18n/catalog";
import type { Language } from "./i18n/languages";

/** The plain text of a failure (a request's rejection or its presentation), for a surface that
 * cannot render the structured one. */
export function errorText(error: ErrorPresentation | Error): string {
  return typeof error === "string" ? error : error.message;
}

/** The sentence an Alert's description says for `error`, as plain text for a surface that cannot render it
 * (a status region), for a failure whose summary is a single catalogue sentence: not the raw text, the remedy or the
 * details. A failure the application has no words for is its generic sentence. Not for the failures whose plain
 * wording carries more than that sentence (`daemonMessage` appends a sharing failure's items, cause and detail, and
 * `project_branches_not_determined`'s detail; a `launch_failed` fills its reason with the nested failure's full
 * message, a trust diagnostic with the whole trust reason). */
export function failureSummaryText(t: Translate, language: Language, error: ErrorPresentation, records: RecordNames): string {
  if (isUncataloguedFailure(error)) return t("error.unexpected.summary");
  if (typeof error === "string") return error;
  if (hasShellTimeoutDetails(error)) return t("daemon.shell_environment_timeout.cause");
  return hasCodeParams(error) ? daemonMessage(language, error.code, error.params, error.message, records, true) : error.message;
}

/** What a diagnostic block keeps of an unexpected failure shown without its message: a failure the daemon coded is its
 * message; a wrapped JavaScript error is its message and then its stack, the message put first when the stack lacks it
 * (WebKit's stacks hold frames only). */
export function unexpectedFailureDiagnostic(error: { code: string; message: string; stack?: string }): string {
  if (error.code !== "unexpected_error" || !error.stack) return error.message;
  return error.stack.split("\n", 1)[0].endsWith(error.message) ? error.stack : `${error.message}\n${error.stack}`;
}

/** What to keep of a failure for a diagnostic block: its stack when it has one, else its message. */
export function diagnosticText(error: unknown): string | undefined {
  if (typeof error === "string") return error;
  if (!error || typeof error !== "object") return error === undefined || error === null ? undefined : String(error);
  try {
    if ("stack" in error && typeof error.stack === "string" && error.stack) return error.stack;
    if ("message" in error && typeof error.message === "string") return error.message;
  } catch {
    return undefined;
  }
  return undefined;
}
