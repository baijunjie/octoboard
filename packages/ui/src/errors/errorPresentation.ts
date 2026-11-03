import { CodedError } from "./codedError";
import { lacksSummary, hasTrustDiagnostic, referenceNameParams, runningSessionIds, messageCode, withUiParams } from "./daemonMessage";
import { leftBranchItems } from "./branchCleanupItems";
import { classifyError, namedSystemCause, type ErrorRecovery } from "./errorRecovery";
import { globalParam, isMessageKey, type PlainMessageKey } from "../i18n/catalog";
import type { MessageParams } from "../protocol";
import { CODE, codeRecord, messageFacets } from "./codes";
import { isUncataloguedFailure } from "./uncataloguedFailure";

/** Daemon codes and parameters remain available for rendering and recovery classification. */
export type ErrorPresentation = string | CodedError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === CODE.shell_environment_timeout && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

/** The sentence an uncatalogued failure opens with: a cause the system's text names (a full disk, a denied access, a
 * failed connection) says so, any other failure the application can tell the user how to act on says only that the
 * operation did not complete, and only a failure classified unexpected is "Something unexpected went wrong." */
export function uncataloguedSummaryKey(error: { code: string; params: MessageParams; message: string }): PlainMessageKey {
  const classification = classifyError(error);
  if (classification.kind === "unexpected") return "error.unexpected.summary";
  return systemFailureKey(classification.recovery);
}

function systemFailureKey(recovery: ErrorRecovery): PlainMessageKey {
  switch (recovery) {
    case "diskSpace": return "error.systemFailure.diskSpace";
    case "access": return "error.systemFailure.access";
    case "connection": return "error.systemFailure.connection";
    default: return "error.systemFailure.other";
  }
}

/** The sentence for the cause the system's text names in a failure that states only its cause, which follows the
 * failure's own sentence (see `namedSystemCause`); undefined where the text names none. */
export function systemCauseKey(error: unknown): PlainMessageKey | undefined {
  const recovery = namedSystemCause(error);
  return recovery && systemFailureKey(recovery);
}

/** Whether the application words `error` in a structured way, with chips, the shell timeout's layout or the generic sentence
 * of an uncatalogued failure, rather than as the plain text of its message. */
export function isStructuredFailure(error: { code: string; params: MessageParams }): boolean {
  return hasCodeParams(error) || hasShellTimeoutDetails(error) || isUncataloguedFailure(error);
}

/** The raw text of a cause the application has no words for, which the failure's sentence says only as a generic
 * reason: a `launch_failed` or failed automatic switch whose own failure is uncatalogued (see `isUncataloguedFailure`), or a trust failure whose
 * `reason_code` has no `daemon.trust_reason.<code>`, whose `reason` is then the daemon's English account. Undefined
 * where the cause is worded. */
export function unwordedReason(code: string, params: MessageParams): string | undefined {
  if (codeRecord(code)?.nested) {
    const reason = params.reason_code;
    return reason && reason !== code && isUncataloguedFailure({ code: reason, params }) ? params.reason ?? "" : undefined;
  }
  if (code !== CODE.trust_answer_failed && code !== CODE.trust_not_carried_over) return undefined;
  return params.reason && !isMessageKey(`daemon.trust_reason.${params.reason_code}`) ? params.reason : undefined;
}

/** Whether `error` has every param its message shows as a chip, the UI's own included. A code with no opening sentence
 * (`noSummary`) always has them, having no message to show, so it stays structured with no `detail` too. */
export function hasCodeParams(error: { code: string; params: MessageParams }): boolean {
  if (lacksSummary(error.code)) return true;
  if (error.code === CODE.shell_environment_timeout) return !hasShellTimeoutDetails(error) && !!error.params.detail;
  if (error.code === CODE.session_has_running_sessions) return !!runningSessionIds(error.params);
  if (leftBranchItems(error.code, error.params)) return true;
  const params = withUiParams(error.code, error.params);
  const code = messageCode(error.code, params);
  const references = referenceNameParams(error.code, params);
  const facets = messageFacets(code);
  const record = codeRecord(error.code);
  const names = [...(facets?.codeParams ?? []), ...(facets?.nameParams ?? []), ...references.filter((name) => globalParam(name) === undefined)];
  if (!names.every((name) => !!params[name])) return false;
  return names.length > 0 || references.length > 0 || !!record?.recoveryLine || (!!record?.detailBlock && !!params.detail);
}

/** How a session notice is shown: a failure (`failureNotice` in `codes.ts`) as an error alert, anything else as a
 * toast. A trust entry not carried over is a notice, Grok Build only asking again the next time, unless it carries
 * what the system said (a `store_io_failed` detail, or a reason the catalog has no words for), which only an error
 * alert keeps, behind its "Error details". */
export function noticeKind(code: string, params: MessageParams): "error" | "notice" {
  if (codeRecord(code)?.failureNotice) return "error";
  return code === CODE.trust_not_carried_over && (hasTrustDiagnostic(code, params) || unwordedReason(code, params) !== undefined) ? "error" : "notice";
}

/** A launch failure retains the nested cause code and params even when it has no special markup.
 * Other notices retain their own code at their caller. */
export function noticeErrorPresentation(code: string, params: MessageParams, message: string): CodedError | undefined {
  if (code !== CODE.launch_failed) return undefined;
  return new CodedError(message, params.reason_code ?? "", params);
}

export function errorPresentation(error: unknown): CodedError {
  if (error instanceof CodedError) return error;
  const foreign = error && typeof error === "object" ? error as { message?: unknown; stack?: unknown } : undefined;
  const message = error instanceof Error ? error.message || error.name : typeof foreign?.message === "string" ? foreign.message : String(error);
  const failure = new CodedError(message, "unexpected_error", {});
  failure.cause = error;
  // The failure's stack is the one it came with, never the stack of this wrapping, which says nothing about it.
  failure.stack = typeof foreign?.stack === "string" ? foreign.stack : undefined;
  return failure;
}
