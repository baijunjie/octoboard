import React from "react";

import { IndicatorTooltip } from "../components/IndicatorTooltip";
import { useT } from "../i18n/react";
import type { BranchOrigin } from "../protocol";

/**
 * The mark a branch carries for the session that created it: a hollow ring, named "Created by
 * session {title}" for assistive technology and on pointer hover. It answers who made the branch,
 * never who is working in it now. A branch with no origin gets no badge, which is the ordinary case.
 *
 * The ring carries no colour: the mark's presence says a session made the branch, and the title says
 * which. The title is the daemon's snapshot from when the attribution was made, so the badge reads
 * the same after the session is renamed or gone.
 *
 * Kept shortfall: it is an `IndicatorTooltip`, so the title shows on pointer hover only and the badge
 * is no tab stop. A screen-reader user has the accessible name, but a sighted keyboard-only user
 * has no route to the session's name.
 */
export function BranchOriginBadge({ origin }: { origin: BranchOrigin }): React.ReactElement {
  const t = useT();
  const label = t("git.compare.branchOrigin", { title: origin.title });
  return (
    <IndicatorTooltip tooltip={label} label={label} className="inline-flex shrink-0">
      <span aria-hidden="true" className="inline-block size-2.5 rounded-full border-2 border-muted" />
    </IndicatorTooltip>
  );
}
