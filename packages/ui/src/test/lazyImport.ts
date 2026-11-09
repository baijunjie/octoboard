// Shared by the tests that render a component behind a `React.lazy` boundary.
import { vi } from "vitest";
import { flushPromises } from "./clock";

/** How long a `vi.waitFor` lets a first-use dynamic `import()` (a lazily loaded library such as mermaid or katex) take
 * to load. A cold import transforms and evaluates a large module graph, which takes seconds on a loaded machine; the
 * wait ends when the import resolves, so the bound is only how long a hang is tolerated. */
export const LAZY_IMPORT_TIMEOUT_MS = 10_000;

/**
 * Lets a lazily imported component that the last render asked for resolve, and React render it in
 * place of the `Suspense` fallback, inside `act`. Call it after the first render that needs the
 * import, before reading anything inside the boundary.
 *
 * The import resolves several microtask hops after that render, so a bare `await` does not reach it;
 * a real 0 ms timer runs after them all. It cannot run under `vi.useFakeTimers()`, so a test on fake
 * timers asserts only on what sits outside the boundary, and the call is not folded into a render
 * helper that such tests share.
 */
export async function settleLazyImport(): Promise<void> {
  if (vi.isFakeTimers()) throw new Error("settleLazyImport needs the real clock: a lazy import cannot be settled under vi.useFakeTimers()");
  await flushPromises();
}
