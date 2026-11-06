import type React from "react";
import { mergeRefs as ariaMergeRefs } from "react-aria";

/** One ref that keeps every one of `refs`, whichever kind each is, in step with the element it is given. */
export function mergeRefs<T>(...refs: (React.Ref<T> | undefined)[]): React.Ref<T> {
  return ariaMergeRefs(...refs);
}
