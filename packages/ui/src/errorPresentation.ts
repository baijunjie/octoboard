import { DaemonRequestError } from "./daemon-client";
import { CODE_PARAMS, DETAIL_BLOCK_CODES, hasTrustDiagnostic, RECOVERY_CODES, referenceNameParams, leftBranchItems, NAME_PARAMS, runningSessionIds, messageCode, withUiParams } from "./daemonMessage";
import { ActionableError, classifyError } from "./errorRecovery";
import { globalParam, isMessageKey, type PlainMessageKey } from "./i18n/catalog";
import type { MessageParams } from "./protocol";

/** Daemon codes and parameters remain available for rendering and recovery classification. */
export type ErrorPresentation = string | DaemonRequestError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === "shell_environment_timeout" && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

/** Codes `daemon-client.ts` gives the failures it raises itself, always with a catalog message (`connection.error.*`) in the
 * current language, never an English literal, so the message is already the application's own. */
const CLIENT_CODES = new Set(["connection_closed", "request_timed_out"]);

/** Whether the application has no message of its own for `error`, so the only words it has are the
 * daemon's raw text: a code the catalog lacks, or `internal_error` / `unexpected_error`, whose
 * message is whatever the system said. Such a failure is shown as the application's generic
 * sentence with the raw text behind Details. A failure with no code at all is the application's
 * own wording, and a string and an `ActionableError` are too, as are the codes the client raises
 * itself with a localized message (`CLIENT_CODES`). */
export function isUncataloguedFailure(error: ErrorPresentation | { code: string; params: MessageParams }): boolean {
  if (typeof error === "string" || error.code === "" || error instanceof ActionableError || CLIENT_CODES.has(error.code)) return false;
  if (error.code === "internal_error" || error.code === "unexpected_error") return true;
  return !isMessageKey(`daemon.${messageCode(error.code, withUiParams(error.code, error.params))}`);
}

/** The sentence an uncatalogued failure opens with: a cause the system's text names (a full disk, a denied access, a
 * failed connection) says so, any other failure the application can tell the user how to act on says only that the
 * operation did not complete, and only a failure classified unexpected is "Something unexpected went wrong." */
export function uncataloguedSummaryKey(error: { code: string; params: MessageParams; message: string }): PlainMessageKey {
  const classification = classifyError(error);
  if (classification.kind === "unexpected") return "error.unexpected.summary";
  switch (classification.recovery) {
    case "diskSpace": return "error.systemFailure.diskSpace";
    case "access": return "error.systemFailure.access";
    case "connection": return "error.systemFailure.connection";
    default: return "error.systemFailure.other";
  }
}

/** Whether the application words `error` in a structured way, with chips, the shell timeout's layout or the generic sentence
 * of an uncatalogued failure, rather than as the plain text of its message. */
export function isStructuredFailure(error: { code: string; params: MessageParams }): boolean {
  return hasCodeParams(error) || hasShellTimeoutDetails(error) || isUncataloguedFailure(error);
}

/** The raw text of a cause the application has no words for, which the failure's sentence says only as a generic
 * reason: a `launch_failed` whose own failure is uncatalogued (see `isUncataloguedFailure`), or a trust failure whose
 * `reason_code` has no `daemon.trust_reason.<code>`, whose `reason` is then the daemon's English account. Undefined
 * where the cause is worded. */
export function unwordedReason(code: string, params: MessageParams): string | undefined {
  if (code === "launch_failed") {
    const reason = params.reason_code;
    return reason && reason !== "launch_failed" && isUncataloguedFailure({ code: reason, params }) ? params.reason ?? "" : undefined;
  }
  if (code !== "trust_answer_failed" && code !== "trust_not_carried_over") return undefined;
  return params.reason && !isMessageKey(`daemon.trust_reason.${params.reason_code}`) ? params.reason : undefined;
}

/** Whether `error` has every param its message shows as a chip, the UI's own included. */
export function hasCodeParams(error: { code: string; params: MessageParams }): boolean {
  if (error.code === "shell_environment_timeout") return !hasShellTimeoutDetails(error) && !!error.params.detail;
  if (error.code === "session_has_running_sessions") return !!runningSessionIds(error.params);
  if (leftBranchItems(error.code, error.params)) return true;
  const params = withUiParams(error.code, error.params);
  const code = messageCode(error.code, params);
  const references = referenceNameParams(error.code, params);
  const names = [...(CODE_PARAMS[code] ?? []), ...(NAME_PARAMS[code] ?? []), ...references.filter((name) => globalParam(name) === undefined)];
  if (!names.every((name) => !!params[name])) return false;
  return names.length > 0 || references.length > 0 || RECOVERY_CODES.has(error.code) || (DETAIL_BLOCK_CODES.has(error.code) && !!params.detail);
}

/** The session notices that are failures, shown as an error alert rather than a toast. */
const FAILURE_NOTICES = new Set(["launch_failed", "trust_answer_failed", "queued_messages_dropped", "handoff_messages_dropped", "sharing_messages_dropped", "sharing_resume_failed", "account_sharing_repair_failed", "message_not_sent"]);

/** How a session notice is shown: a failure as an error alert, anything else as a toast. A trust entry not carried
 * over is a notice, Grok Build only asking again the next time, unless it carries what the system said (a
 * `store_io_failed` detail, or a reason the catalog has no words for), which only an error alert keeps, behind its
 * "Error details". */
export function noticeKind(code: string, params: MessageParams): "error" | "notice" {
  if (FAILURE_NOTICES.has(code)) return "error";
  return code === "trust_not_carried_over" && (hasTrustDiagnostic(code, params) || unwordedReason(code, params) !== undefined) ? "error" : "notice";
}

/** A launch failure retains the nested cause code and params even when it has no special markup.
 * Other notices retain their own code at their caller. */
export function noticeErrorPresentation(code: string, params: MessageParams, message: string): DaemonRequestError | undefined {
  if (code !== "launch_failed") return undefined;
  return new DaemonRequestError(message, params.reason_code ?? "", params);
}

export function errorPresentation(error: unknown): DaemonRequestError {
  if (error instanceof DaemonRequestError) return error;
  const foreign = error && typeof error === "object" ? error as { message?: unknown; stack?: unknown } : undefined;
  const message = error instanceof Error ? error.message || error.name : typeof foreign?.message === "string" ? foreign.message : String(error);
  const failure = new DaemonRequestError(message, "unexpected_error", {});
  failure.cause = error;
  // The failure's stack is the one it came with, never the stack of this wrapping, which says nothing about it.
  failure.stack = typeof foreign?.stack === "string" ? foreign.stack : undefined;
  return failure;
}
