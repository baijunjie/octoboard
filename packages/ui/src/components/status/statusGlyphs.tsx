import { Hand } from "lucide-react";
import React from "react";

import { DELEGATED_DOT_PX, DELEGATED_RING_PX, delegatedPingScale } from "./delegatedGlyph";
import { HAND_SHIFT, shiftedViewBox } from "./opticalCentring";
import { SLEEP_PATHS } from "./sleepGlyph";

/** The box every status glyph sits in, however it is drawn (a lucide icon, the "zz", a dot or a ring): 16px, or
 * 14px where `small`, the same for every status so that a column of them has one centre line, and centring what is
 * drawn inside it. Nothing drawn inside takes part in its layout: a ping's copy is laid out apart (see
 * `WorkingDot`) and a glyph is never larger than the box. `naming` is the glyph's accessible name, or its hiding. */
export function StatusSlot({ small, children, ...naming }: { small?: boolean; children: React.ReactNode } & React.AriaAttributes & { role?: string }): React.ReactElement {
  return (
    <span {...naming} className={`flex shrink-0 items-center justify-center ${small ? "size-3.5" : "size-4"}`}>
      {children}
    </span>
  );
}

/** The pinging dot of the busy statuses: Tailwind's own ping, a dot or hollow ring sending a fading
 * copy of itself outward, still where the system asks for reduced motion. The colour says whether the
 * session itself is busy (the accent blue) or at its prompt, where it can be talked to (idle's green); the
 * form progresses along the status order, so neighbours look related: `working` is a filled dot in the
 * accent blue; `background` (at the prompt, work still going) a hollow ring in green, telling it from the
 * dot by shape; `delegated` (at the prompt, the sessions under it working) a larger hollow green ring with
 * a filled dot in its centre, which tells it from the background ring by shape and holds still where the
 * ping does. Its geometry is set in `delegatedGlyph.ts`; its ring is larger than the others' dots, so its
 * ping reaches less far (`ping-small`, scaled by `--ping-scale`) and ends at the edge of the slot (`delegatedPingScale`).
 * It is the glyph alone, in `StatusSlot`'s centre. None differs from idle's speech bubble by colour alone, so the motion is never the only cue. The ping moves only `transform` and
 * `opacity`, and its box is laid out apart from the row (`contain-layout`; not `contain-paint`, which
 * would clip the ping's copy where it grows past the glyph), so the loop never asks the browser for a
 * layout. */
export function WorkingDot({ small, kind = "working" }: { small?: boolean; kind?: "working" | "background" | "delegated" }): React.ReactElement {
  const fill = kind === "working" ? "bg-accent-soft-foreground" : kind === "background" ? "border-2 border-success-soft-foreground" : "border border-success-soft-foreground";
  return (
    <span
      className={`relative flex shrink-0 contain-layout ${kind === "delegated" ? "" : "size-2"}`}
      style={kind === "delegated" ? ({ width: DELEGATED_RING_PX, height: DELEGATED_RING_PX, "--ping-scale": delegatedPingScale(small) } as React.CSSProperties) : undefined}
    >
      <span className={`absolute inline-flex size-full rounded-full ${fill} opacity-75 ${kind === "delegated" ? "motion-safe:animate-ping-small" : "motion-safe:animate-ping"}`} />
      <span className={`relative inline-flex size-full items-center justify-center rounded-full ${fill}`}>
        {kind === "delegated" && <span className="rounded-full bg-success-soft-foreground" style={{ width: DELEGATED_DOT_PX, height: DELEGATED_DOT_PX }} />}
      </span>
    </span>
  );
}

/** The "zz" of a dormant session (`SLEEP_PATHS`), drawn as a lucide icon is: `currentColor` strokes on
 * the 24-unit grid, sized by `className`. Where the system allows motion, three copies of the large Z drift up the
 * diagonal to the top right, shrinking as they rise (`status-scroll`, each a third of a period behind the last),
 * so a new one enters at the bottom left as the top one leaves; the svg clips them to its own box, so none leaves
 * the slot, and they stay at full opacity, so the glyph keeps the contrast of its colour at every frame. Where the
 * system asks for reduced motion the still pair, `SLEEP_PATHS` as it is, is drawn instead: the two are in groups
 * that each hide where the other shows. The motion moves only `transform`, and the svg is laid out apart from its
 * slot (`contain-layout`). */
export function SleepGlyph({ className }: { className: string }): React.ReactElement {
  const letter = "origin-center [transform-box:fill-box]";
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" className={`${className} contain-layout overflow-hidden`}>
      <g className="motion-safe:hidden">
        {SLEEP_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <g className="motion-reduce:hidden">
        <path d={SLEEP_PATHS[0]} className={`${letter} motion-safe:animate-status-scroll`} />
        <path d={SLEEP_PATHS[0]} className={`${letter} motion-safe:animate-status-scroll-second`} />
        <path d={SLEEP_PATHS[0]} className={`${letter} motion-safe:animate-status-scroll-third`} />
      </g>
    </svg>
  );
}

/** The waving hand of a session waiting for the user. */
export function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand viewBox={shiftedViewBox(HAND_SHIFT)} className={`${className} origin-[70%_90%] text-warning-soft-foreground motion-safe:animate-status-wave`} />;
}
