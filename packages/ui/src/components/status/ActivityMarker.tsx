import { Check, Hand, LoaderCircle, MessageCircleMore } from "lucide-react";
import React from "react";

import { useT } from "../../i18n/react";
import { activityLabel } from "../../sessions/sessionLabel";
import { type ShownStatusKind } from "../../sessions/sessionStatus";
import type { Activity } from "../../sessions/order";
import { IndicatorTooltip } from "../hint/IndicatorTooltip";
import { DELEGATED_DOT_PX, DELEGATED_RING_PX } from "./delegatedGlyph";
import { WavingHand, WorkingDot } from "./statusGlyphs";

/** The badge's delegated ring; its dot keeps the glyph's proportion to the ring. */
const BADGE_DELEGATED_RING_PX = 10;
const BADGE_DELEGATED_DOT_PX = (BADGE_DELEGATED_RING_PX * DELEGATED_DOT_PX) / DELEGATED_RING_PX;

/** The marker a collapsed project row shows for the sessions beneath it, and focus mode's
 * console-switch chip for the sessions of that console session: a waving hand when one is waiting
 * for the user, a ringing dot when one is
 * at work, a hollow green ring when none is but work runs in the background, a speech bubble when
 * one is merely running, awaiting instructions.
 *
 * It carries the indicator tooltip that puts the activity into words for a sighted user, who has
 * only four glyphs of a similar size to tell them apart. The marker and the tooltip's trigger
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
    case "background":
      return (
        <span aria-hidden="true" className="flex shrink-0">
          <WorkingDot small kind="background" />
        </span>
      );
    case "running":
      return <MessageCircleMore aria-hidden="true" className="size-3.5 shrink-0 text-success-soft-foreground" />;
    default:
      return null;
  }
}

/** Inverted status glyphs stay legible over arbitrary avatar colours. Done wears its check mark on the
 * idle badge's green. Delegated, background and working follow their glyphs' progression: a green disc
 * with a ring around a dot, a green disc with a hollow centre, and a blue disc with a solid centre; only
 * the centre breathes, keeping the badge steady.
 * The avatar supplies its tooltip and accessible name, so the badge is decorative. */
export function ActivityBadge({ status, className = "" }: { status: ShownStatusKind | undefined; className?: string }): React.ReactElement | null {
  const disc = `flex size-3.5 items-center justify-center rounded-full ${className}`;
  switch (status) {
    case "waiting_user":
      return (
        <span aria-hidden="true" className={`${disc} bg-warning text-warning-foreground`}>
          <Hand strokeWidth={2.5} className="size-2.5 origin-[70%_90%] motion-safe:animate-status-wave" />
        </span>
      );
    case "idle":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <MessageCircleMore strokeWidth={2.5} className="size-2.5" />
        </span>
      );
    case "done":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <Check strokeWidth={2.5} className="size-2.5" />
        </span>
      );
    case "delegated":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <span
            className="flex items-center justify-center rounded-full border-[1.5px] border-current motion-safe:animate-status-breathe"
            style={{ width: BADGE_DELEGATED_RING_PX, height: BADGE_DELEGATED_RING_PX }}
          >
            <span className="rounded-full bg-current" style={{ width: BADGE_DELEGATED_DOT_PX, height: BADGE_DELEGATED_DOT_PX }} />
          </span>
        </span>
      );
    case "working":
      return (
        <span aria-hidden="true" className={`${disc} bg-accent text-accent-foreground`}>
          <span className="size-1.5 rounded-full bg-current motion-safe:animate-status-breathe" />
        </span>
      );
    case "background":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <span className="size-2 rounded-full border-[1.5px] border-current motion-safe:animate-status-breathe" />
        </span>
      );
    case "switching":
      return (
        <span aria-hidden="true" className={`${disc} bg-default text-default-foreground`}>
          <LoaderCircle className="size-2.5 motion-safe:animate-spin" />
        </span>
      );
    default:
      return null;
  }
}
