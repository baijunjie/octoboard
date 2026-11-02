import { Archive, CircleCheck, CirclePause, LoaderCircle, MessageCircleMore, Pause } from "lucide-react";
import React from "react";

import { useT } from "../../i18n/react";
import { statusLabel } from "../../sessions/sessionLabel";
import { type ShownStatusKind } from "../../sessions/sessionStatus";
import { IndicatorTooltip } from "../hint/IndicatorTooltip";
import { BUBBLE_SHIFT, shiftedViewBox } from "./opticalCentring";
import { SleepGlyph, StatusSlot, WavingHand, WorkingDot } from "./statusGlyphs";

const ICON_CLASS = "size-4 shrink-0";
/** The compact glyph's size: the marker beside a collapsed project's name and on a switch chip. */
const SMALL_ICON_CLASS = "size-3.5 shrink-0";

/** What a glyph is given for assistive technology: the status as its name, or nothing at all when
 * something around it already says the status. */
type Naming = { "aria-hidden": true } | { role: "img"; "aria-label": string };

/** The glyph itself, one per status in the status table of docs/product/session-statuses.md, as
 * lucide glyphs and dots rather than emoji (the project keeps user-facing UI free of decorative
 * emoji, the same convention `CLAUDE.md` sets for committed prose). Colour says whether the session
 * needs the person, is at its prompt or is busy, and form progresses along the status order, so neighbours
 * look related: in orange, the statuses that need the person, a raised hand for the session waiting for an
 * answer and a "zz" for dormant (held in its background-agents view until the person leaves it); in green, the
 * statuses where the session itself is at its prompt and can be talked to, a pause sign in a circle for
 * paused (its last turn ended abnormally), a check mark in a circle for done (its turn is over), a speech
 * bubble for idle (waiting to be told), a ring with a dot in its centre for delegated (the sessions under
 * it are working) and a hollow ring for working in the background; the solid blue working dot, the session
 * itself busy; and, in the muted ink, a bare pause sign for interrupted (stopped where it was, and can be
 * resumed from there) and a turning arc for starting. Every status has a shape of its own, so colour is
 * never the only cue. Each is drawn inside `StatusSlot`, one size for every status (`small` the compact one), which
 * holds the accessible name and centres the drawing; the bubble and the hand are moved a little in it
 * (`opticalCentring.ts`), their ink being lopsided by design. */
function statusGlyph(status: ShownStatusKind, naming: Naming, small: boolean): React.ReactElement {
  return (
    <StatusSlot {...naming} small={small}>
      {statusDrawing(status, small)}
    </StatusSlot>
  );
}

/** What a status draws inside its slot. */
function statusDrawing(status: ShownStatusKind, small: boolean): React.ReactElement {
  const iconClass = small ? SMALL_ICON_CLASS : ICON_CLASS;
  switch (status) {
    case "switching":
    case "starting":
      // An arc turning in the muted ink: coming up, with nothing to answer for yet. The arc's
      // shape tells it from the other glyphs where the system asks for reduced motion.
      return <LoaderCircle className={`${iconClass} text-app-muted motion-safe:animate-spin`} />;
    case "working":
      return <WorkingDot small={small} />;
    case "waiting_user":
      return <WavingHand className={iconClass} />;
    case "dormant":
      // The hand's orange, as it needs the person too: held in a view until they leave it. Its letters
      // move rather than wave, and its shape tells it from the hand where the system asks for reduced motion.
      return <SleepGlyph className={`${iconClass} text-warning-soft-foreground`} />;
    case "paused":
      return <CirclePause className={`${iconClass} text-success-soft-foreground`} />;
    case "idle":
      // HeroUI's success soft foreground, the colour of every glyph of that meaning (3.93:1 at the worst
      // of the surfaces it sits on, see `style.css`).
      return <MessageCircleMore viewBox={shiftedViewBox(BUBBLE_SHIFT)} className={`${iconClass} text-success-soft-foreground`} />;
    case "done":
      return <CircleCheck className={`${iconClass} text-success-soft-foreground`} />;
    case "delegated":
      // The ring is the green of the prompt statuses; the dot in its centre tells it from the background
      // ring, and holds still where the ping does not.
      return <WorkingDot small={small} kind="delegated" />;
    case "background":
      // A hollow ring in the green of the prompt statuses: at the prompt, but with work still going.
      return <WorkingDot small={small} kind="background" />;
    case "interrupted":
      return <Pause className={`${iconClass} text-app-muted`} />;
    case "archived":
      return <Archive className={`${iconClass} text-app-muted`} />;
  }
}

/**
 * A session's status glyph. It carries the status as its accessible name, since it is the only
 * status cue where nothing else names it; `decorative` drops that for a row whose own label already
 * does.
 *
 * `withTooltip` gives it the indicator tooltip that puts the status into words for a sighted user,
 * for the placements where no visible text beside it does. The tooltip says exactly what the name
 * says, and the name moves out onto the tooltip's trigger, so that the description react-aria links
 * to the trigger belongs to the named element rather than to a presentational wrapper around it. A
 * decorative glyph leaves the trigger unnamed too, adding nothing to the name of the row or trail
 * it sits in.
 *
 * `small` is the compact size of the marker beside a collapsed project's name and on a switch chip.
 *
 * `touchPopover` is the caller's to set (see `IndicatorTooltip`): on where the glyph stands alone, off
 * inside a pressable row, whose press a tap on the glyph would share.
 */
export function StatusIcon({
  status,
  decorative,
  withTooltip,
  touchPopover,
  small,
}: {
  status: ShownStatusKind;
  decorative?: boolean;
  withTooltip?: boolean;
  touchPopover?: boolean;
  small?: boolean;
}): React.ReactElement {
  const t = useT();
  const label = statusLabel(t, status);
  const named = !decorative;
  const glyph = statusGlyph(status, named && !withTooltip ? { role: "img", "aria-label": label } : { "aria-hidden": true }, !!small);
  if (!withTooltip) return glyph;
  return (
    <IndicatorTooltip tooltip={label} label={named ? label : undefined} className="inline-flex shrink-0" touchPopover={touchPopover}>
      {glyph}
    </IndicatorTooltip>
  );
}
