import { createElement, lazy, type ComponentProps, type ComponentType } from "react";

/**
 * A lazily loaded component whose load can be tried again. `lazy` keeps the rejection of a failed
 * import, so every later render of the same lazy component throws it again; `retry` puts a fresh
 * one in place, and the next time `Component` mounts it imports again. A boundary that catches the
 * failure calls `retry` and mounts `Component` anew to recover. `retry` does nothing unless the
 * import failed, so a boundary catching some other error does not remount what loaded fine.
 *
 * `Component` itself never changes, so it can be exported and compared like any other. Call this
 * at module level, once per module to load.
 */
// `any` props: a component type with specific props is not assignable to one with `never` props.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function retryableLazy<T extends ComponentType<any>>(
  load: () => Promise<{ default: T }>,
): { Component: ComponentType<ComponentProps<T>>; retry: () => void } {
  let failed = false;
  const start = () =>
    lazy(() =>
      load().catch((error: unknown) => {
        failed = true;
        throw error;
      }),
    );
  let current = start();
  function Component(props: ComponentProps<T>) {
    // Kept out of the React Compiler: `current` is state outside React that `retry` replaces, which its
    // memoization would take for constant and so keep rendering the one that failed.
    "use no memo";
    return createElement(current as ComponentType<ComponentProps<T>>, props);
  }
  return {
    Component,
    retry: () => {
      if (!failed) return;
      failed = false;
      current = start();
    },
  };
}
