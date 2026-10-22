import type { ErrorPresentation } from "./errorPresentation";

/** The plain text of a failure (a request's rejection or its presentation), for a surface that
 * cannot render the structured one. */
export function errorText(error: ErrorPresentation | Error): string {
  return typeof error === "string" ? error : error.message;
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
