import { ProgressCircle as HeroProgressCircle } from "@heroui/react";
import React from "react";

/** HeroUI's progress circle: a ring filled to `value` per cent, named by `label`. `trackClassName` sizes the ring. */
export function ProgressCircle({ label, value, indeterminate, className, trackClassName }: {
  label: string;
  value?: number;
  indeterminate?: boolean;
  className?: string;
  trackClassName?: string;
}): React.ReactElement {
  return (
    <HeroProgressCircle aria-label={label} value={value} isIndeterminate={indeterminate} className={className}>
      <HeroProgressCircle.Track className={trackClassName}>
        <HeroProgressCircle.TrackCircle />
        <HeroProgressCircle.FillCircle />
      </HeroProgressCircle.Track>
    </HeroProgressCircle>
  );
}
