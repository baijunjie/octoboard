import { Component, useMemo, type ReactNode, type Ref } from "react";
import { mergeRefs } from "react-aria";

import { useListMotion } from "../components/useListMotion";

/** Preserve the browser's scroll/focus ref and distinguish logical changes from its DOM window. */
export function useBrowserListMotion(forwarded: Ref<HTMLDivElement>, keys: ReadonlySet<string>) {
  const motion = useListMotion<HTMLDivElement>({ virtualized: true, keys });
  const ref = useMemo(() => mergeRefs(forwarded, motion), [forwarded, motion]);
  return { ref, prepare: motion.prepare };
}

/** A layout-effect cleanup runs after DOM mutation. This lifecycle captures departing content
 * while its old virtual wrappers and identity still exist, without copying rows on scrolling. */
export class BrowserListMotion extends Component<{ keys: ReadonlySet<string>; prepare: (keys: ReadonlySet<string>) => void; children: ReactNode }> {
  getSnapshotBeforeUpdate() {
    this.props.prepare(this.props.keys);
    return null;
  }

  componentDidUpdate() {}

  render() { return this.props.children; }
}
