import React, { useEffect } from "react";

import { retryableLazy } from "../../components/hooks/retryableLazy";

// The renderer module, with the library and its grammars, loads the first time code is shown.
let rendererLoaded = false;
const renderer = () => {
  rendererLoaded = true;
  return import("./renderer");
};
const highlightedFile = retryableLazy(() => renderer().then((module) => ({ default: module.HighlightedFile })));
const renderedDiff = retryableLazy(() => renderer().then((module) => ({ default: module.RenderedDiff })));
export const HighlightedFile = highlightedFile.Component;
export const RenderedDiff = renderedDiff.Component;

/** Shows its fallback in place of a renderer that threw — while rendering, in an effect, or by
 * failing to load its module — instead of letting the failure take the viewer down. A failure also
 * makes the renderer module (and the lazy components in `retry`, if given) import again the next time
 * they mount, since `lazy` would keep the failed import; a new `resetKey` then mounts them anew, which
 * gives the renderer another try. */
export class RendererBoundary extends React.Component<
  {
    resetKey: string;
    fallback: React.ReactNode | ((error: unknown) => React.ReactNode);
    /** Makes the lazy components of the children, beyond the code renderer's, load again. */
    retry?: () => void;
    children: React.ReactNode;
  },
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
    highlightedFile.retry();
    renderedDiff.retry();
    this.props.retry?.();
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
