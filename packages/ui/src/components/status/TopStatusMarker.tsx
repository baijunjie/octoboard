import { Check, CirclePause, Hand, LoaderCircle, MessageCircleMore, Pause } from "lucide-react";
import React from "react";

import { type ShownStatusKind } from "../../sessions/sessionStatus";
import { DELEGATED_DOT_PX, DELEGATED_RING_PX } from "./delegatedGlyph";
import { BUBBLE_SHIFT, HAND_SHIFT, shiftedViewBox } from "./opticalCentring";
import { StatusIcon } from "./StatusIcon";
import { SleepGlyph } from "./statusGlyphs";

/** The badge's delegated ring; its dot keeps the glyph's proportion to the ring. */
const BADGE_DELEGATED_RING_PX = 10;
const BADGE_DELEGATED_DOT_PX = (BADGE_DELEGATED_RING_PX * DELEGATED_DOT_PX) / DELEGATED_RING_PX;

/** The marker a collapsed project row shows for the sessions beneath it, and focus mode's
 * console-switch chip for the sessions of that console session: the glyph of the top status among
 * them (`topStatus`), which is `StatusIcon`'s own, compact, in the one 14px `StatusSlot` every status shares, so
 * every status has its one shape in the sidebar and beside the avatars. Nothing when `status` is `undefined`.
 *
 * It carries the indicator tooltip that puts the status into words for a sighted user, who has
 * only glyphs of a similar size to tell them apart. The glyph and the tooltip's trigger are both
 * left unnamed: the row's and the chip's own accessible names already say the status.
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
export function TopStatusMarker({ status }: { status: ShownStatusKind | undefined }): React.ReactElement | null {
  if (status === undefined) return null;
  return <StatusIcon status={status} decorative withTooltip touchPopover={false} small />;
}

/** The disc is the badge's fixed 14px box, centring whatever it holds. Inverted status glyphs stay legible over arbitrary avatar colours, one for every status but archived,
 * which a console's badge never shows. Done wears its check mark, and paused
 * its pause sign in a circle (a bare pause sign is interrupted's), on the idle badge's green; dormant wears its
 * "zz" on the waiting badge's orange, as both need the person. Delegated, background and working follow their glyphs' progression: a green disc
 * with a ring around a dot, a green disc with a hollow centre, and a blue disc with a solid centre; only
 * the centre breathes, keeping the badge steady. Starting and switching account wear the turning arc on a
 * grey disc, as neither has settled into a status yet, and interrupted its pause sign on the same grey, as
 * it has no process.
 * The avatar supplies its tooltip and accessible name, so the badge is decorative. */
export function AvatarStatusBadge({ status, className = "" }: { status: ShownStatusKind | undefined; className?: string }): React.ReactElement | null {
  const disc = `flex size-3.5 items-center justify-center rounded-full ${className}`;
  switch (status) {
    case "waiting_user":
      return (
        <span aria-hidden="true" className={`${disc} bg-warning text-warning-foreground`}>
          <Hand strokeWidth={2.5} viewBox={shiftedViewBox(HAND_SHIFT)} className="size-2.5 origin-[70%_90%] motion-safe:animate-status-wave" />
        </span>
      );
    case "idle":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <MessageCircleMore strokeWidth={2.5} viewBox={shiftedViewBox(BUBBLE_SHIFT)} className="size-2.5" />
        </span>
      );
    case "dormant":
      return (
        <span aria-hidden="true" className={`${disc} bg-warning text-warning-foreground`}>
          {/* The lucide stroke rather than the badge's thicker one: at 2.5 the two letters' corners meet. */}
          <SleepGlyph className="size-2.5" />
        </span>
      );
    case "paused":
      return (
        <span aria-hidden="true" className={`${disc} bg-success text-success-foreground`}>
          <CirclePause strokeWidth={2.5} className="size-2.5" />
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
    case "starting":
    case "switching":
      return (
        <span aria-hidden="true" className={`${disc} bg-default text-default-foreground`}>
          <LoaderCircle className="size-2.5 motion-safe:animate-spin" />
        </span>
      );
    case "interrupted":
      return (
        <span aria-hidden="true" className={`${disc} bg-default text-default-foreground`}>
          <Pause strokeWidth={2.5} className="size-2.5" />
        </span>
      );
    default:
      return null;
  }
}
