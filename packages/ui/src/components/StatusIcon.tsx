import { Archive, Hand, MessageCircleMore, Pause } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import type { SessionStatus } from "../protocol";
import { statusLabel } from "../sessionLabel";
import type { Activity } from "../sidebar/order";

const ICON_CLASS = "size-4 shrink-0";

/** A working session's glyph: Tailwind's own ping, an accent dot sending a fading copy of itself
 * outward, still where the system asks for reduced motion. It differs from idle's speech bubble
 * by shape, so the motion is never the only cue. */
function WorkingDot({ small }: { small?: boolean }): React.ReactElement {
  return (
    <span className={`flex shrink-0 items-center justify-center ${small ? "size-3" : "size-4"}`}>
      <span className="relative flex size-2">
        <span className="absolute inline-flex size-full rounded-full bg-accent opacity-75 motion-safe:animate-ping" />
        <span className="relative inline-flex size-2 rounded-full bg-accent" />
      </span>
    </span>
  );
}

/** The waving hand of a session waiting for the user. */
function WavingHand({ className }: { className: string }): React.ReactElement {
  return <Hand className={`${className} origin-[70%_90%] text-warning motion-safe:animate-status-wave`} />;
}

/**
 * The per-session status glyphs, one per status in the "Session statuses" table of
 * docs/product/sessions.md, as lucide glyphs and dots rather than emoji (the project keeps
 * user-facing UI free of decorative emoji, the same convention `CLAUDE.md` sets for committed
 * prose). Each carries the status as its accessible name, since it is the only status cue where
 * nothing else names it; `decorative` drops that for a row whose own label already does. Idle is a
 * speech bubble (sitting at the prompt, waiting to be told), and interrupted a pause sign (stopped
 * where it was, and can be resumed from there).
 */
export function StatusIcon({ status, decorative }: { status: SessionStatus; decorative?: boolean }): React.ReactElement {
  const t = useT();
  const a11y = decorative ? ({ "aria-hidden": true } as const) : ({ role: "img", "aria-label": statusLabel(t, status) } as const);
  switch (status) {
    case "working":
      return (
        <span {...a11y} className="flex shrink-0">
          <WorkingDot />
        </span>
      );
    case "waiting_user":
      return (
        <span {...a11y} className="flex shrink-0">
          <WavingHand className={ICON_CLASS} />
        </span>
      );
    case "idle":
      // HeroUI's own success colour, about 2.2:1 on white, under the 3:1 a meaningful glyph needs;
      // kept so on purpose, by the user's decision.
      return <MessageCircleMore {...a11y} className={`${ICON_CLASS} text-success`} />;
    case "interrupted":
      return <Pause {...a11y} className={`${ICON_CLASS} text-muted`} />;
    case "archived":
      return <Archive {...a11y} className={`${ICON_CLASS} text-muted`} />;
  }
}

/** The marker a project row or a console in the switcher shows for the sessions beneath it: a
 * waving hand when one is waiting for the user, a ringing dot when one is at work, a speech bubble
 * when one is merely running, awaiting instructions. Decorative: the row's own label carries the words. */
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
