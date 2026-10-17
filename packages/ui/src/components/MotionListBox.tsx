import { ListBox } from "@heroui/react";
import React, { useMemo } from "react";
import { mergeRefs } from "react-aria";

import { useListMotion } from "./useListMotion";

/** Own the motion observer inside the lazily mounted popover, so its first live update has a baseline. */
export function MotionListBox<T extends object>({
  virtualized,
  ref: forwarded,
  ...props
}: React.ComponentProps<typeof ListBox<T>> & { virtualized?: boolean }): React.ReactElement {
  const motion = useListMotion<HTMLDivElement>({ virtualized });
  const ref = useMemo(() => mergeRefs(forwarded, motion), [forwarded, motion]);
  return <ListBox {...props} ref={ref} />;
}
