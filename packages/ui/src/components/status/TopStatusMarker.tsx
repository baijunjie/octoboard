import { Check, CirclePause, Hand, MessageCircleMore } from "lucide-react";
import React from "react";

import { type ShownStatusKind } from "../../sessions/sessionStatus";
import { BUBBLE_SHIFT, HAND_SHIFT, shiftedViewBox } from "./opticalCentring";
import { StatusIcon } from "./StatusIcon";

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

/** A decorative 14px badge for the console's waiting, paused, done or awaiting-instructions summary.
 * The avatar supplies its tooltip and accessible name. */
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
    default:
      return null;
  }
}
