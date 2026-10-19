import { Archive, LoaderCircle, MessageCircleMore, Pause } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { type StatusKind, statusLabel } from "../sessionLabel";
import { IndicatorTooltip } from "./IndicatorTooltip";
import { WavingHand, WorkingDot } from "./statusGlyphs";

const ICON_CLASS = "size-4 shrink-0";

/** What a glyph is given for assistive technology: the status as its name, or nothing at all when
 * something around it already says the status. */
type Naming = { "aria-hidden": true } | { role: "img"; "aria-label": string };

/** The glyph itself, one per status in the "Session statuses" table of docs/product/sessions.md, as
 * lucide glyphs and dots rather than emoji (the project keeps user-facing UI free of decorative
 * emoji, the same convention `CLAUDE.md` sets for committed prose). Idle is a speech bubble
 * (sitting at the prompt, waiting to be told), starting a turning arc (the agent is still being
 * launched), interrupted a pause sign (stopped where it was, and can be resumed from there), and
 * running in the background the working dot as a hollow ring in idle's green: at the prompt, but
 * with work still going. */
function statusGlyph(status: StatusKind, naming: Naming): React.ReactElement {
  switch (status) {
    case "starting":
      // An arc turning in the muted ink: coming up, with nothing to answer for yet. The arc's
      // shape tells it from the other glyphs where the system asks for reduced motion.
      return <LoaderCircle {...naming} className={`${ICON_CLASS} text-app-muted motion-safe:animate-spin`} />;
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
    case "background":
      // The same green and the same exception as the bubble above; the hollow ring's shape tells it
      // from the bubble and from the filled working dot, and the accessible name says it.
      return (
        <span {...naming} className="flex shrink-0">
          <WorkingDot background />
        </span>
      );
    case "interrupted":
      return <Pause {...naming} className={`${ICON_CLASS} text-app-muted`} />;
    case "archived":
      return <Archive {...naming} className={`${ICON_CLASS} text-app-muted`} />;
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
  status: StatusKind;
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
