import { errorPresentation } from "./errorPresentation";

/** Resource load events have no runtime exception; thrown primitives and foreign-realm errors do. */
export function runtimeError(event: Event): Error | string | undefined {
  if (!(event instanceof ErrorEvent)) return undefined;
  if (event.error instanceof Error) return event.error;
  const value: unknown = event.error ?? event.message;
  return typeof value === "object" && value !== null ? errorPresentation(value) : String(value);
}
