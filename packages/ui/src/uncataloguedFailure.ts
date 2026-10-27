import { ActionableError } from "./actionableError";
import { messageCode, withUiParams } from "./daemonMessage";
import type { ErrorPresentation } from "./errorPresentation";
import { isMessageKey } from "./i18n/catalog";
import type { MessageParams } from "./protocol";

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
