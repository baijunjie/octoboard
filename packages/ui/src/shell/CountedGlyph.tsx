import { Badge } from "@heroui/react";
import React from "react";

import { textDirection } from "../i18n/languages";
import { useCurrentLanguage } from "../i18n/react";

/** The badges on the rail are cut out of what they sit on: the glyph or avatar under one is masked
 * away in a ring around it, so the badge reads as separate from it and the ring shows whatever is
 * behind (the native material, the browser's chrome colour, a hover or selected tint) rather than a
 * colour of its own. Each mask is the hole for one badge geometry, so it is written next to the
 * classes placing that badge.
 *
 * A count is as large as the 16px glyph itself, so centred on the glyph's top end corner
 * it would hide a quarter of it; it sits above that corner, 4px past it along the diagonal, so the
 * glyph keeps its shape. The pill is 16px high with no padding (`*:px-0` takes away the
 * label's own `px-0.5`, which would make two digits a 19px pill); two semibold 11px digits measure
 * up to 15px, so up to two digits it is a 16px circle, and the overflow label ("99+", about 26px)
 * is a pill of its own 28px width, centred 2px nearer the glyph so that its end stays clear of the
 * content panel's edge. Its hole is the two end circles, each 1.5px wider than the
 * pill's own ends, as one mask layer each: the layers are intersected, so what is cut out is their
 * union.
 *
 * The count is a decoration of the layout rather than a physical thing, so under right-to-left it
 * moves to the glyph's other top corner, the one that still points into the content: the rail is at
 * the window's start side, which under right-to-left is its right edge, so left at the glyph's
 * physical top right the overflow pill would run into that edge.
 * HeroUI's `Badge` has only physical placements, so the corner is picked by `placement`, and the
 * offsets and the mask coordinates (measured across the 16px glyph) are mirrored to match it. */
const COUNT_PLACEMENT = "pointer-events-none translate-x-[calc(25%+4px)] -translate-y-[calc(25%+4px)] rtl:-translate-x-[calc(25%+4px)]";
const COUNT_OVERFLOW_PLACEMENT = "pointer-events-none translate-x-[calc(25%+2px)] -translate-y-[calc(25%+4px)] rtl:-translate-x-[calc(25%+2px)]";
const COUNT_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_18px_-8px,transparent_9.5px,black_10px)] rtl:[mask-image:radial-gradient(circle_at_-2px_-8px,transparent_9.5px,black_10px)]";
const COUNT_OVERFLOW_PILL = "w-7";
const COUNT_OVERFLOW_CUT_OUT =
  "[mask-image:radial-gradient(circle_at_10px_-8px,transparent_9.5px,black_10px),radial-gradient(circle_at_22px_-8px,transparent_9.5px,black_10px)] [mask-composite:intersect] rtl:[mask-image:radial-gradient(circle_at_6px_-8px,transparent_9.5px,black_10px),radial-gradient(circle_at_-6px_-8px,transparent_9.5px,black_10px)]";

/** A rail glyph with a count on its top end corner: the glyph (`children`, 16px) is cut out around the badge, and a
 * count past 99 reads `overflowLabel`. */
export function CountedGlyph({ count, overflowLabel, color, children }: {
  count: number;
  overflowLabel: string;
  color: "warning" | "accent";
  children: React.ReactElement<{ className?: string }>;
}): React.ReactElement {
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const overflow = count > 99;
  return (
    <Badge.Anchor>
      {React.cloneElement(children, { className: `${children.props.className ?? ""} ${overflow ? COUNT_OVERFLOW_CUT_OUT : COUNT_CUT_OUT}` })}
      <Badge
        aria-hidden="true"
        color={color}
        variant="primary"
        size="sm"
        placement={rtl ? "top-left" : "top-right"}
        // The overflow label is a number followed by a sign, which reads in that order in
        // any language; left to the surrounding direction it would come out as "+99".
        dir="ltr"
        className={`${overflow ? COUNT_OVERFLOW_PLACEMENT : COUNT_PLACEMENT} border-0 text-[11px] leading-none font-semibold *:px-0 ${overflow ? COUNT_OVERFLOW_PILL : ""}`}
      >
        {overflow ? overflowLabel : count}
      </Badge>
    </Badge.Anchor>
  );
}
