import React, { useLayoutEffect, useRef, useState } from "react";

import { RailSeparator, RailSpacer } from "./RailSeparator";

/** Whether the free space `element` holds is used up (under a pixel), followed as the rail is resized. */
function useSpaceUsedUp(element: React.RefObject<HTMLElement | null>): boolean {
  const [usedUp, setUsedUp] = useState(false);
  useLayoutEffect(() => {
    const target = element.current;
    if (!target) return;
    const read = (height: number) => setUsedUp(height < 1);
    read(target.getBoundingClientRect().height);
    const observer = new ResizeObserver((entries) => {
      const last = entries.at(-1);
      if (last) read(last.contentRect.height);
    });
    observer.observe(target);
    return () => observer.disconnect();
  }, [element]);
  return usedUp;
}

/**
 * What stands between the group of pages (Scheduled tasks, Remote devices) and the rail's bottom group, two items of the rail's stack: the
 * free space that pushes the group down (`RailSpacer`), and under it a separator that is drawn only once the free space
 * is used up, so the button and the group do not touch without a seam. The separator is an item whether or not it is
 * drawn, so drawing it changes no height and cannot turn the free space it is measured against back into some.
 */
export function RailRemoteGap(): React.ReactElement {
  const space = useRef<HTMLDivElement>(null);
  const usedUp = useSpaceUsedUp(space);
  return (
    <>
      <RailSpacer ref={space} data-rail-gap="" />
      <RailSeparator data-rail-groove="" drawn={usedUp} />
    </>
  );
}
