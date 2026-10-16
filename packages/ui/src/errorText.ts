import type { ErrorPresentation } from "./errorPresentation";

/** The plain text of a failure (a request's rejection or its presentation), for a surface that
 * cannot render the structured one. */
export function errorText(error: ErrorPresentation | Error): string {
  return typeof error === "string" ? error : error.message;
}
