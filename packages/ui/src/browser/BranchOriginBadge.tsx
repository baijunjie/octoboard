import React from "react";

import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { BranchOrigin } from "../protocol";
import { useDaemonStore } from "../store";

/**
 * The mark a branch carries for the session that created it: a ring, named "Created by session
 * “{title}”" for assistive technology, with the same words in a tooltip while the pointer is on the
 * ring. It answers who made the branch, never who is working in it now. A branch with no origin
 * gets no badge, which is the ordinary case.
 *
 * The title is the session's current one while it exists, because that is the one the sidebar
 * shows and a title that differs from it would name a session the user cannot find. The daemon's
 * snapshot from when the attribution was made is the fallback for a session that is gone, and the
 * name and tooltip then say it does not exist. An archived session is still in the store, and so
 * still exists. When the creator is the selected session its ring is filled and in the accent
 * colour, and the name and tooltip say so as well. Colour never carries it alone: the fill is a
 * shape.
 *
 * Kept shortfall: it is an `IndicatorTooltip`, so the title shows on pointer hover only and the badge
 * is no tab stop. A screen-reader user has it in the option's name, which includes the ring's, but a
 * sighted keyboard-only user has no route to the session's name.
 */
export function BranchOriginBadge({ origin, selectedSessionId }: { origin: BranchOrigin; selectedSessionId: string | undefined }): React.ReactElement {
  const t = useT();
  // Before the first snapshot the store holds no sessions at all, which says nothing about this one.
  const gone = useDaemonStore((s) => s.hosts !== undefined && !s.sessions.has(origin.session));
  const selected = origin.session === selectedSessionId;
  // Read as a string, not as the session or the map: either has a new identity after any daemon event.
  const liveTitle = useDaemonStore((s) => s.sessions.get(origin.session)?.title);
  const label = t(gone ? "git.compare.branchOriginGone" : selected ? "git.compare.branchOriginSelected" : "git.compare.branchOrigin", {
    title: isolate(liveTitle ?? origin.title),
  });
  return (
    <IndicatorTooltip tooltip={label} label={label} className="inline-flex shrink-0">
      <span aria-hidden="true" className={`inline-block size-2.5 rounded-full border-2 ${selected ? "border-accent-glyph bg-accent-glyph" : "border-app-muted"}`} />
    </IndicatorTooltip>
  );
}
