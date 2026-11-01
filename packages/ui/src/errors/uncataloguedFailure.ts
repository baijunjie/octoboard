import { ActionableError } from "./actionableError";
import { codeRecord } from "./codes";
import { messageCode, withUiParams } from "./daemonMessage";
import type { ErrorPresentation } from "./errorPresentation";
import { isMessageKey } from "../i18n/catalog";
import type { MessageParams } from "../protocol";

/** Whether the application has no message of its own for `error`, so the only words it has are the
 * daemon's raw text: a code the catalog lacks, or `internal_error` / `unexpected_error`, whose
 * message is whatever the system said. Such a failure is shown as the application's generic
 * sentence with the raw text behind Details. A failure with no code at all is the application's
 * own wording, and a string and an `ActionableError` are too, as are the codes the client raises
 * itself with a localized message (the codes of `client` origin: `daemon-client.ts` raises them with a catalog message
 * `connection.error.*` in the current language, never an English literal). */
export function isUncataloguedFailure(error: ErrorPresentation | { code: string; params: MessageParams }): boolean {
  if (typeof error === "string" || error.code === "" || error instanceof ActionableError || codeRecord(error.code)?.origin === "client") return false;
  if (error.code === "internal_error" || error.code === "unexpected_error") return true;
  return !isMessageKey(`daemon.${messageCode(error.code, withUiParams(error.code, error.params))}`);
}
