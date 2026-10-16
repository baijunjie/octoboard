import { DaemonRequestError } from "./daemon-client";
import { CODE_PARAMS, messageCode, withUiParams } from "./daemonMessage";
import type { MessageParams } from "./protocol";

/** A request failure kept as metadata only when something presents it in a structured way: the
 * shell timeout's layout, and a message that sets some of its params as code (`CODE_PARAMS` in
 * `daemonMessage.ts`). It renders through `DaemonErrorMessage` wherever the surface can render
 * markup; any other surface takes `errorText`. */
export type ErrorPresentation = string | DaemonRequestError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === "shell_environment_timeout" && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

/** Whether `error` has every param its message shows as code, the UI's own included. */
export function hasCodeParams(error: { code: string; params: MessageParams }): boolean {
  const params = withUiParams(error.code, error.params);
  return CODE_PARAMS[messageCode(error.code, params)]?.every((name) => !!params[name]) ?? false;
}

/** The structured form of a `session_notice` that reports a failure the daemon words as an `error`
 * too: a `launch_failed` whose `reason_code` is a shell environment timeout, whose params arrive
 * beside it. `undefined` for any other notice, which is presented with its own code and params. */
export function noticeErrorPresentation(code: string, params: MessageParams, message: string): DaemonRequestError | undefined {
  if (code !== "launch_failed") return undefined;
  const error = new DaemonRequestError(message, params.reason_code ?? "", params);
  return hasShellTimeoutDetails(error) ? error : undefined;
}

export function errorPresentation(error: Error): ErrorPresentation {
  return error instanceof DaemonRequestError && (hasShellTimeoutDetails(error) || hasCodeParams(error)) ? error : error.message;
}
