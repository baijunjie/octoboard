import React from "react";

import { FadedScroller, useFadeRegion } from "./scrollFade";

/** A box that scrolls sideways instead of wrapping its text. Its frame (the box's border, fill and corners, `className`
 * and `style`) is around the scroller, which takes focus only while it overflows, as a region named `label`, so the
 * keyboard reaches the text beyond its edge; the focus ring is drawn on the frame. */
export function ScrollBox({ label, className = "", style, children }: { label: string; className?: string; style?: React.CSSProperties; children: React.ReactNode }): React.ReactElement {
  const region = useFadeRegion({ label });
  return (
    <div style={style} className={`${className} ${region.ring}`}>
      <FadedScroller orientation="horizontal" region={region}>{children}</FadedScroller>
    </div>
  );
}
