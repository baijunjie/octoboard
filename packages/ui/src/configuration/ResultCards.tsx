import { CircleCheck, CircleDot } from "lucide-react";
import React from "react";

import type { AccountFallback, AccountOutcome, ImportFailure, ImportStatus } from "../protocol";
import { FlatCard } from "../components/card/FlatCard";
import { CardTitle, EntryStatus, FailureReason, FallbackMark, ItemName, LEFT_OUT_STATUS, SHARED_STATUS, STATUS_TONE, type StatusEntry } from "./importParts";

type Status = ImportStatus | AccountOutcome;

/** What each outcome is called, drawn and coloured. */
const STATUS: Record<Status, StatusEntry> = {
  created: { label: "configuration.import.result.created", icon: CircleCheck, tone: STATUS_TONE.success },
  existing: { label: "configuration.import.result.existing", icon: CircleDot, tone: STATUS_TONE.muted },
  left_out: LEFT_OUT_STATUS,
  ...SHARED_STATUS,
};

/** An outcome as a glyph and its word. */
export function Outcome({ status }: { status: Status }): React.ReactElement {
  return <EntryStatus entry={STATUS[status]} />;
}

/**
 * One console's or one agent's result as a card, like the review's: its name (after it, a warning mark where its
 * account references fell back to a default account) heads the card, with its own `status` beside the name when it has
 * one (a console's, always: an `Outcome`, or the state of a running import), and the reason when it failed in the
 * header under the name, as a row writes its own, so it never reads as one of the rows. The rows, if any, follow as
 * `children`.
 */
export function ResultCard({
  name,
  fallbacks,
  status,
  failure,
  children,
}: {
  name: string;
  fallbacks?: AccountFallback[];
  status?: React.ReactNode;
  failure?: ImportFailure | null;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <FlatCard>
      <FlatCard.Header className="flex flex-col gap-1.5">
        <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
          <CardTitle name={name} fallbacks={fallbacks ?? []} className="min-w-40 flex-1" />
          {status}
        </div>
        {failure && <FailureReason failure={failure} />}
      </FlatCard.Header>
      {children}
    </FlatCard>
  );
}

/**
 * One project's or account's row: its name (with its fallback mark) and `status` on a line that wraps the status under
 * the name when there is no room, what there is to say about it under it, then the reason when it failed, written out
 * in the row.
 */
export function ResultRow({
  name,
  fallbacks,
  status,
  detail,
  failure,
}: {
  name: string;
  fallbacks?: AccountFallback[];
  status: React.ReactNode;
  detail?: string;
  failure?: ImportFailure | null;
}): React.ReactElement {
  return (
    <li className="flex flex-col gap-1.5 py-3">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
        <span className="flex min-w-40 flex-1 items-start gap-1">
          <ItemName>{name}</ItemName>
          <FallbackMark subject={name} fallbacks={fallbacks ?? []} size="text-sm" />
        </span>
        {status}
      </div>
      {detail && <p className="text-sm text-app-muted [overflow-wrap:anywhere]" dir="auto">{detail}</p>}
      {failure && <FailureReason failure={failure} />}
    </li>
  );
}
