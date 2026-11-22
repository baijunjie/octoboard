import { ProgressCircle as HeroProgressCircle } from "@heroui/react";
import React from "react";

/** HeroUI's progress circle: a ring filled to `value` per cent, named by `label`. `trackClassName` sizes the ring.
 * `contrastTrack` is for a ring on a filled surface, where the default track would not show (`ProgressCircle.css`). */
export function ProgressCircle({ label, value, indeterminate, contrastTrack = false, className, trackClassName }: {
  label: string;
  value?: number;
  indeterminate?: boolean;
  contrastTrack?: boolean;
  className?: string;
  trackClassName?: string;
}): React.ReactElement {
  return (
    <HeroProgressCircle aria-label={label} value={value} isIndeterminate={indeterminate} className={contrastTrack ? `app-progress-circle--contrast-track ${className ?? ""}` : className}>
      <HeroProgressCircle.Track className={trackClassName}>
        <HeroProgressCircle.TrackCircle />
        <HeroProgressCircle.FillCircle />
      </HeroProgressCircle.Track>
    </HeroProgressCircle>
  );
}
