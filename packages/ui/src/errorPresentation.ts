import { DaemonRequestError } from "./daemon-client";
import type { MessageParams } from "./protocol";

/** A request failure kept as metadata only when its UI has a structured presentation. */
export type ErrorPresentation = string | DaemonRequestError;

export function hasShellTimeoutDetails(error: { code: string; params: MessageParams }): boolean {
  return error.code === "shell_environment_timeout" && ["shell", "command", "timeout"].every((name) => !!error.params[name]);
}

export function errorPresentation(error: Error): ErrorPresentation {
  return error instanceof DaemonRequestError && hasShellTimeoutDetails(error) ? error : error.message;
}
