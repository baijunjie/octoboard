import React, { useMemo } from "react";

import { ListBox, mergeRefs } from "../../primitives";
import { useListMotion } from "./useListMotion";

/** Own the motion observer inside the lazily mounted popover, so its first live update has a baseline. A virtualized
 * list (`rowHeight`) is observed as one. */
export function MotionListBox<T extends object>({
  ref: forwarded,
  ...props
}: React.ComponentProps<typeof ListBox<T>>): React.ReactElement {
  const motion = useListMotion<HTMLDivElement>({ virtualized: props.rowHeight !== undefined });
  const ref = useMemo(() => mergeRefs(forwarded, motion), [forwarded, motion]);
  return <ListBox {...props} ref={ref} />;
}
