import { errorPresentation } from "./errorPresentation";

/**
 * The browser's notice that a ResizeObserver callback changed layout and some observations were deferred to the
 * next frame. It carries no exception and nothing failed, so it is not reported.
 */
const resizeObserverLoop = /ResizeObserver loop (completed with undelivered notifications|limit exceeded)/;

/** Resource load events have no runtime exception; thrown primitives and foreign-realm errors do. */
export function runtimeError(event: Event): Error | string | undefined {
  if (!(event instanceof ErrorEvent)) return undefined;
  if (event.error == null && resizeObserverLoop.test(event.message)) return undefined;
  if (event.error instanceof Error) return event.error;
  const value: unknown = event.error ?? event.message;
  return typeof value === "object" && value !== null ? errorPresentation(value) : String(value);
}
