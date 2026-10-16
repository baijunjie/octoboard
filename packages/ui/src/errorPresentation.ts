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

export function errorPresentation(error: Error): ErrorPresentation {
  return error instanceof DaemonRequestError && (hasShellTimeoutDetails(error) || hasCodeParams(error)) ? error : error.message;
}
