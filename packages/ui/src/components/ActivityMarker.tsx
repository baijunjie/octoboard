import { Hand, MessageCircleMore } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { activityLabel } from "../sessionLabel";
import type { Activity } from "../sidebar/order";
import { IndicatorTooltip } from "./IndicatorTooltip";
import { WavingHand, WorkingDot } from "./statusGlyphs";

/** The marker a collapsed project row shows for the sessions beneath it, and focus mode's
 * console-switch chip for the sessions of that console session (the rail's consoles show the same
 * as an `ActivityBadge`): a waving hand when one is waiting for the user, a ringing dot when one is
 * at work, a speech bubble when one is merely running, awaiting instructions.
 *
 * It carries the indicator tooltip that puts the activity into words for a sighted user, who has
 * only three glyphs of a similar size to tell the three apart. The marker and the tooltip's trigger
 * are both left unnamed: the row's and the chip's own accessible names already say the activity.
 *
 * On the chip that puts the trigger's `div` inside a `<button>`, which no content model allows. It
 * is accepted rather than worked around: the trigger cancels only the focus a mouse press would
 * otherwise give it, and the press itself still reaches the button, so pressing the marker switches
 * console session exactly as pressing the chip's title does; the anchored shape, the only
 * alternative the library offers, would answer a hover anywhere on the chip rather than on the
 * marker, which is the one thing this tooltip is for.
 *
 * It never opens a touch popover (`touchPopover={false}`): both places are pressable, a project's
 * row (a tap folds it) and the chip (a tap switches console session), and the tap is theirs.
 */
export function ActivityMarker({ activity }: { activity: Activity }): React.ReactElement | null {
  const t = useT();
  const marker = activityMarker(activity);
  const label = activityLabel(t, activity);
  if (label === undefined) return marker;
  return (
    <IndicatorTooltip tooltip={label} className="inline-flex shrink-0" touchPopover={false}>
      {marker}
    </IndicatorTooltip>
  );
}

function activityMarker(activity: Activity): React.ReactElement | null {
  switch (activity) {
    case "waiting":
      return (
        <span aria-hidden="true" className="flex shrink-0">
          <WavingHand className="size-3.5" />
        </span>
      );
    case "working":
      return (
        <span aria-hidden="true" className="flex shrink-0">
          <WorkingDot small />
        </span>
      );
    case "running":
      return <MessageCircleMore aria-hidden="true" className="size-3.5 shrink-0 text-success-soft-foreground" />;
    default:
      return null;
  }
}

/** `ActivityMarker` as a badge on a console's avatar (the rail): a small disc carrying the activity's
 * glyph, so it reads against the disc whatever the disc sits on. Work keeps the session glyph's
 * blue over the opaque theme background with a green rim, so the window material cannot change
 * their contrast. The waiting hand still waves and the working dot still pings, as in the sidebar.
 * Decorative, and with no tooltip of its own, unlike `ActivityMarker`: the avatar carries the
 * tooltip, which says the activity (the rail's `ConsoleTile`), and the console's accessible name
 * says it too. */
export function ActivityBadge({ activity, className = "" }: { activity: Activity; className?: string }): React.ReactElement | null {
  if (!activity) return null;
  const disc = `flex size-3.5 items-center justify-center rounded-full ${className}`;
  switch (activity) {
    case "waiting":
      return (
        <span aria-hidden="true" className={`${disc} bg-warning text-warning-foreground`}>
          <Hand strokeWidth={2.5} className="size-2.5 origin-[70%_90%] motion-safe:animate-status-wave" />
        </span>
      );
    case "working":
      return (
        <span aria-hidden="true" className={`${disc} border-2 border-success-soft-foreground bg-background`}>
          <span className="relative flex size-1.5 contain-layout">
            <span className="absolute inline-flex size-full rounded-full bg-accent-soft-foreground opacity-75 motion-safe:animate-ping" />
            <span className="relative inline-flex size-1.5 rounded-full bg-accent-soft-foreground" />
          </span>
        </span>
      );
    case "running":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <MessageCircleMore strokeWidth={2.5} className="size-2.5" />
        </span>
      );
  }
}
