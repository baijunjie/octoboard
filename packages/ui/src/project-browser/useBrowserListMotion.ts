import { Component, useMemo, type ReactNode, type Ref } from "react";
import { mergeRefs } from "react-aria";

import { useListMotion, type ListFolds } from "../components/motion/useListMotion";

/** The ref of the browser's scroll container, merged with the list motion's, and the `prepare` that
 * `BrowserListMotion` calls. The list is virtualized, so the motion is told the keys of the rows
 * that are logically present rather than the rows that happen to be in the DOM window. */
export function useBrowserListMotion(forwarded: Ref<HTMLDivElement>, keys: ReadonlySet<string>, folds?: ListFolds) {
  const motion = useListMotion<HTMLDivElement>({ virtualized: true, keys, folds });
  const ref = useMemo(() => mergeRefs(forwarded, motion), [forwarded, motion]);
  return { ref, prepare: motion.prepare };
}

/** Calls `prepare` as React commits an update, just before it changes the DOM, while the departing
 * rows and their virtual wrappers are still in the DOM: a layout-effect cleanup runs only after the
 * DOM has been mutated. It reads the DOM at that point instead of copying rows as the list scrolls.
 * A class, because only a class has `getSnapshotBeforeUpdate`. */
export class BrowserListMotion extends Component<{ keys: ReadonlySet<string>; folds?: ListFolds; prepare: (keys: ReadonlySet<string>, folds?: ListFolds) => void; children: ReactNode }> {
  getSnapshotBeforeUpdate() {
    this.props.prepare(this.props.keys, this.props.folds);
    return null;
  }

  // React warns when a component has `getSnapshotBeforeUpdate` and no `componentDidUpdate`.
  componentDidUpdate() {}

  render() { return this.props.children; }
}
