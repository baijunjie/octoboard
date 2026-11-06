import { Meter as HeroMeter } from "@heroui/react";
import React from "react";

/** HeroUI's meter: a bar of how much of a known amount is used, `value` per cent, named by `label`; `valueLabel` is
 * the value as it is read, where the bare percentage would say too little. `trackClassName` sizes its track. */
export function Meter({ label, value, valueLabel, className, style, trackClassName }: {
  label: string;
  value: number;
  valueLabel?: string;
  className?: string;
  style?: React.CSSProperties;
  trackClassName?: string;
}): React.ReactElement {
  return (
    <HeroMeter aria-label={label} value={value} valueLabel={valueLabel} className={className} style={style}>
      <HeroMeter.Track className={trackClassName}>
        <HeroMeter.Fill />
      </HeroMeter.Track>
    </HeroMeter>
  );
}
