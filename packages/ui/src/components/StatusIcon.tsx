import { Archive, Hand, MessageCircleMore, Pause } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import type { SessionStatus } from "../protocol";
import { statusLabel } from "../sessionLabel";
import type { Activity } from "../sidebar/order";
import { IndicatorTooltip } from "./IndicatorTooltip";

const ICON_CLASS = "size-4 shrink-0";

/** A working session's glyph: Tailwind's own ping, an accent dot sending a fading copy of itself
 * outward, still where the system asks for reduced motion. It differs from idle's speech bubble
 * by shape, so the motion is never the only cue. */
function WorkingDot({ small }: { small?: boolean }): React.ReactElement {
  return (
    <span className={`flex shrink-0 items-center justify-center ${small ? "size-3" : "size-4"}`}>
      <span className="relative flex size-2">
        <span className="absolute inline-flex size-full rounded-full bg-accent-glyph opacity-75 motion-safe:animate-ping" />
        <span className="relative inline-flex size-2 rounded-full bg-accent-glyph" />
      </span>
    </span>
  );
}

/** The waving hand of a session waiting for the user. */
function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand className={`${className} origin-[70%_90%] text-warning-glyph motion-safe:animate-status-wave`} />;
}

/** What a glyph is given for assistive technology: the status as its name, or nothing at all when
 * something around it already says the status. */
type Naming = { "aria-hidden": true } | { role: "img"; "aria-label": string };

/** The glyph itself, one per status in the "Session statuses" table of docs/product/sessions.md, as
 * lucide glyphs and dots rather than emoji (the project keeps user-facing UI free of decorative
 * emoji, the same convention `CLAUDE.md` sets for committed prose). Idle is a speech bubble
 * (sitting at the prompt, waiting to be told), and interrupted a pause sign (stopped where it was,
 * and can be resumed from there). */
function statusGlyph(status: SessionStatus, naming: Naming): React.ReactElement {
  switch (status) {
    case "working":
      return (
        <span {...naming} className="flex shrink-0">
          <WorkingDot />
        </span>
      );
    case "waiting_user":
      return (
        <span {...naming} className="flex shrink-0">
          <WavingHand className={ICON_CLASS} />
        </span>
      );
    case "idle":
      // HeroUI's own success colour, 1.98:1 on the sidebar and 1.70:1 on a selected row, under the 3:1
      // a meaningful glyph needs. The exception is deliberate: the green is only a secondary cue, as
      // the bubble's shape tells idle apart from the working dot and the accessible name says it.
      return <MessageCircleMore {...naming} className={`${ICON_CLASS} text-success`} />;
    case "interrupted":
      return <Pause {...naming} className={`${ICON_CLASS} text-muted`} />;
    case "archived":
      return <Archive {...naming} className={`${ICON_CLASS} text-muted`} />;
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
 */
export function StatusIcon({
  status,
  decorative,
  withTooltip,
}: {
  status: SessionStatus;
  decorative?: boolean;
  withTooltip?: boolean;
}): React.ReactElement {
  const t = useT();
  const label = statusLabel(t, status);
  const named = !decorative;
  const glyph = statusGlyph(status, named && !withTooltip ? { role: "img", "aria-label": label } : { "aria-hidden": true });
  if (!withTooltip) return glyph;
  return (
    <IndicatorTooltip tooltip={label} label={named ? label : undefined} className="inline-flex shrink-0">
      {glyph}
    </IndicatorTooltip>
  );
}

/** The marker a collapsed project row shows for the sessions beneath it (the rail's consoles show
 * the same as an `ActivityBadge`): a waving hand when one is waiting for the user, a ringing dot
 * when one is at work, a speech bubble when one is merely running, awaiting instructions.
 * Decorative: the row's own label carries the words. */
export function ActivityMarker({ activity }: { activity: Activity }): React.ReactElement | null {
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
      return <MessageCircleMore aria-hidden="true" className="size-3.5 shrink-0 text-success" />;
    default:
      return null;
  }
}

/** `ActivityMarker` as a badge on a console's avatar (the rail): a small disc in the activity's own
 * colour with its glyph in that colour's foreground, so the glyph reads against the disc whatever
 * the disc sits on — on the light chrome the warning and success colours are fills, too light to
 * read as glyphs of their own. The waiting hand still waves and the working dot still pings, as
 * they do in the sidebar. Decorative: the console's accessible name carries the activity. */
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
        <span aria-hidden="true" className={`${disc} bg-accent`}>
          <span className="relative flex size-1.5">
            <span className="absolute inline-flex size-full rounded-full bg-accent-foreground opacity-75 motion-safe:animate-ping" />
            <span className="relative inline-flex size-1.5 rounded-full bg-accent-foreground" />
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
