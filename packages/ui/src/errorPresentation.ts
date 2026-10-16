import { DaemonRequestError } from "./daemon-client";
import { CODE_PARAMS, NAME_PARAMS, runningSessionIds, messageCode, withUiParams } from "./daemonMessage";
import type { MessageParams } from "./protocol";

/** A request failure kept as metadata only when something presents it in a structured way: the
 * shell timeout's layout, and a message that sets some of its params as chips (`CODE_PARAMS` or `NAME_PARAMS` in
 * `daemonMessage.ts`). It renders through `DaemonErrorMessage` wherever the surface can render
 * markup; any other surface takes `errorText`. */
export type ErrorPresentation = string | DaemonRequestError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === "shell_environment_timeout" && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

/** Whether `error` has every param its message shows as a chip, the UI's own included. */
export function hasCodeParams(error: { code: string; params: MessageParams }): boolean {
  if (error.code === "session_has_running_sessions" && runningSessionIds(error.params)) return true;
  const params = withUiParams(error.code, error.params);
  const code = messageCode(error.code, params);
  const names = [...(CODE_PARAMS[code] ?? []), ...(NAME_PARAMS[code] ?? [])];
  return names.length > 0 && names.every((name) => !!params[name]);
}

/** The structured form of a `session_notice` that reports a failure the daemon words as an `error`
 * too: a `launch_failed` whose `reason_code` has a structured presentation, whose params arrive
 * beside it. `undefined` for any other notice, which is presented with its own code and params. */
export function noticeErrorPresentation(code: string, params: MessageParams, message: string): DaemonRequestError | undefined {
  if (code !== "launch_failed") return undefined;
  const error = new DaemonRequestError(message, params.reason_code ?? "", params);
  return hasShellTimeoutDetails(error) || hasCodeParams(error) ? error : undefined;
}

export function errorPresentation(error: Error): ErrorPresentation {
  return error instanceof DaemonRequestError && (hasShellTimeoutDetails(error) || hasCodeParams(error)) ? error : error.message;
}
