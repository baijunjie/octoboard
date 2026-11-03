import React, { useId } from "react";

import { remainingPercentAt, type QuotaBar } from "../../accounts/quotaFormat";
import type { QuotaWindow } from "../../protocol";

/** The rail button's compact indicator under its gauge: an outlined capsule 18 × 5 px filled by what
 * remains of the main window. With a weekly window too, the main capsule is 4 px tall and one 3 px tall
 * sits a pixel beneath it for that; the gauge rises 2 px further and the pair's bottom sits a pixel
 * lower, so the group stays near the button's middle. Decorative; the button's name states the numbers.
 * They draw in the button's text colour, so they suit a light and a dark rail. */
export function RailQuotaBars({ bar, now }: { bar: QuotaBar; now: number }): React.ReactElement {
  if (!bar.thin) {
    return (
      <span aria-hidden="true" className="absolute inset-x-[7px] bottom-1.5">
        <QuotaCapsule window={bar.main} height={5} now={now} />
      </span>
    );
  }
  return (
    <span aria-hidden="true" className="absolute inset-x-[7px] bottom-[5px] flex flex-col gap-px">
      <QuotaCapsule window={bar.main} height={4} now={now} />
      <QuotaCapsule window={bar.thin} height={3} now={now} />
    </span>
  );
}

/** The width every capsule spans, in pixels: the 32 px button less 7 on each side. Centred in its
 * row, so it stays centred where the button is wider. */
const CAPSULE_WIDTH = 18;

/** One outlined capsule, `height` pixels tall, with fully round ends, filling from the reading
 * direction's start. It is drawn as a vector, a square-ended fill clipped to the capsule and the
 * outline stroked over it: an HTML border beside a filled box leaves a pale antialiased seam between
 * them wherever the button sits at a fractional offset, as it does while scaled down on a press. */
function QuotaCapsule({ window, height, now }: { window: QuotaWindow; height: number; now: number }): React.ReactElement {
  const radius = height / 2;
  const clipId = useId();
  const fill = (CAPSULE_WIDTH * remainingPercentAt(window, now)) / 100;
  return (
    <svg data-slot="quota-capsule" viewBox={`0 0 ${CAPSULE_WIDTH} ${height}`} className="mx-auto my-0 block rtl:-scale-x-100" style={{ width: CAPSULE_WIDTH, height }}>
      <clipPath id={clipId}>
        <rect width={CAPSULE_WIDTH} height={height} rx={radius} />
      </clipPath>
      {fill > 0 && <rect width={fill} height={height} clipPath={`url(#${clipId})`} fill="currentColor" />}
      <rect x={0.5} y={0.5} width={CAPSULE_WIDTH - 1} height={height - 1} rx={radius - 0.5} fill="none" stroke="currentColor" />
    </svg>
  );
}
