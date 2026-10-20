import { DaemonRequestError } from "./daemon-client";
import { CODE_PARAMS, DETAIL_BLOCK_CODES, RECOVERY_CODES, referenceNameParams, leftBranchItems, NAME_PARAMS, runningSessionIds, messageCode, withUiParams } from "./daemonMessage";
import type { MessageParams } from "./protocol";

/** Daemon codes and parameters remain available for rendering and recovery classification. */
export type ErrorPresentation = string | DaemonRequestError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === "shell_environment_timeout" && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

/** Whether `error` has every param its message shows as a chip, the UI's own included. */
export function hasCodeParams(error: { code: string; params: MessageParams }): boolean {
  if (error.code === "shell_environment_timeout") return !hasShellTimeoutDetails(error) && !!error.params.detail;
  if (error.code === "session_has_running_sessions") return !!runningSessionIds(error.params);
  if (leftBranchItems(error.code, error.params)) return true;
  const params = withUiParams(error.code, error.params);
  const code = messageCode(error.code, params);
  const references = referenceNameParams(error.code, params);
  const names = [...(CODE_PARAMS[code] ?? []), ...(NAME_PARAMS[code] ?? []), ...references.filter((name) => name !== "appName")];
  if (!names.every((name) => !!params[name])) return false;
  return names.length > 0 || references.length > 0 || RECOVERY_CODES.has(error.code) || (DETAIL_BLOCK_CODES.has(error.code) && !!params.detail);
}

/** A launch failure retains the nested cause code and params even when it has no special markup.
 * Other notices retain their own code at their caller. */
export function noticeErrorPresentation(code: string, params: MessageParams, message: string): DaemonRequestError | undefined {
  if (code !== "launch_failed") return undefined;
  return new DaemonRequestError(message, params.reason_code ?? "", params);
}

export function errorPresentation(error: unknown): ErrorPresentation {
  if (error instanceof DaemonRequestError) return error;
  const foreign = error && typeof error === "object" ? error as { message?: unknown; stack?: unknown } : undefined;
  const message = error instanceof Error ? error.message || error.name : typeof foreign?.message === "string" ? foreign.message : String(error);
  const failure = new DaemonRequestError(message, "unexpected_error", {});
  failure.cause = error;
  if (typeof foreign?.stack === "string") failure.stack = foreign.stack;
  return failure;
}
