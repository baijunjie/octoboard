import React, { lazy, useEffect } from "react";

// The renderer module, with the library and its grammars, loads the first time code is shown.
let rendererLoaded = false;
const renderer = () => {
  rendererLoaded = true;
  return import("./renderer");
};
export const HighlightedFile = lazy(() => renderer().then((module) => ({ default: module.HighlightedFile })));
export const RenderedDiff = lazy(() => renderer().then((module) => ({ default: module.RenderedDiff })));

/** Shows its fallback in place of a renderer that threw — while rendering, in an effect, or by
 * failing to load its module — instead of letting the failure take the viewer down. A new `resetKey`
 * gives the renderer another try. */
export class RendererBoundary extends React.Component<
  { resetKey: string; fallback: React.ReactNode | ((error: unknown) => React.ReactNode); children: React.ReactNode },
  { failed: boolean; key: string; error?: unknown }
> {
  state: { failed: boolean; key: string; error?: unknown } = { failed: false, key: this.props.resetKey };

  static getDerivedStateFromProps(props: { resetKey: string }, state: { key: string }): { failed: boolean; key: string; error?: unknown } | null {
    return props.resetKey === state.key ? null : { failed: false, key: props.resetKey, error: undefined };
  }

  static getDerivedStateFromError(error: unknown): { failed: boolean; error: unknown } {
    return { failed: true, error };
  }

  componentDidCatch(error: unknown): void {
    console.warn("Viewer: the renderer failed", error);
  }

  render(): React.ReactNode {
    if (!this.state.failed) return this.props.children;
    return typeof this.props.fallback === "function" ? this.props.fallback(this.state.error) : this.props.fallback;
  }
}

/** Keeps the renderer's worker pool for as long as a viewer that calls this is mounted, and ends
 * it, if code was shown at all, once the last one unmounts. The count is checked again once the
 * renderer module is at hand, so a viewer opened in the meantime keeps the pool. */
let mountedViewers = 0;
export function useRendererScope(): void {
  useEffect(() => {
    mountedViewers += 1;
    return () => {
      mountedViewers -= 1;
      if (mountedViewers === 0 && rendererLoaded) {
        void renderer().then((module) => mountedViewers === 0 && module.endRendererPool());
      }
    };
  }, []);
}
