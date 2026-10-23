import React from "react";

import { InlineReference } from "../components/InlineReference";
import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { BranchOrigin } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemonStore } from "../store";

/**
 * The mark a branch carries for the session working on it now, its responsible session: a ring,
 * named "Session {title} is working on this branch" for assistive technology, with the same words
 * in a tooltip while the pointer is on the ring. A branch with no responsible session gets no badge,
 * which is the ordinary case.
 *
 * The title is the session's current display title while it exists — its agent's task beside the
 * name the user gave it — because that is the one the sidebar shows and a title that differs from
 * it would name a session the user cannot find. The name recorded when it became responsible is
 * the fallback for a session that is gone, and the name and tooltip then say it no longer exists.
 * An archived session is still in the store, and so still exists. When the responsible session is
 * the selected session its ring is filled and in the accent colour, and the name and tooltip say so
 * as well. Colour never carries it alone: the fill is a shape.
 *
 * Kept shortfall: it is an `IndicatorTooltip`, so the title shows on pointer hover only and the badge
 * is no tab stop. A screen-reader user has it in the option's name, which includes the ring's, but a
 * sighted keyboard-only user has no route to the session's name.
 */
export function BranchOriginBadge({ origin, selectedSessionId }: { origin: BranchOrigin; selectedSessionId: string | undefined }): React.ReactElement {
  const t = useT();
  // Before the first snapshot the store holds no sessions at all, which says nothing about this one.
  const gone = useDaemonStore((s) => s.hosts !== undefined && !s.sessions.has(origin.responsible_session));
  const selected = origin.responsible_session === selectedSessionId;
  // Read as a string, not as the session or the map: either has a new identity after any daemon event.
  const liveTitle = useDaemonStore((s) => {
    const session = s.sessions.get(origin.responsible_session);
    return session && sessionDisplayTitle(t, session);
  });
  const message = gone ? "git.branch.responsibleGone" : selected ? "git.branch.responsibleSelected" : "git.branch.responsible";
  const label = t(message, {
    title: isolate(liveTitle ?? origin.responsible_title),
  });
  return (
    <IndicatorTooltip tooltip={<Message id={message} params={{ title: <InlineReference dir="auto" wrap>{liveTitle ?? origin.responsible_title}</InlineReference> }} />} label={label} className="inline-flex shrink-0">
      <span aria-hidden="true" className={`inline-block size-2.5 rounded-full border-2 ${selected ? "border-accent-soft-foreground bg-accent-soft-foreground" : "border-app-muted"}`} />
    </IndicatorTooltip>
  );
}
