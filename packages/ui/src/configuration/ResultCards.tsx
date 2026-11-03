import { CircleAlert, CircleCheck, CircleDot, CircleMinus, CircleSlash } from "lucide-react";
import React from "react";

import { AlertStatusContext } from "../components/alerts/AlertButton";
import { useT } from "../i18n/react";
import type { AccountFallback, AccountOutcome, ImportFailure, ImportStatus } from "../protocol";
import { Card, CARD_HEADER_CLASS, CardTitle, FailureReason, FallbackMark, ItemName, STATUS_TONE, type StatusEntry, StatusGlyph, ToneLine } from "./importParts";

type Status = ImportStatus | AccountOutcome;

/** What each outcome is called, drawn and coloured. */
const STATUS: Record<Status, StatusEntry> = {
  created: { label: "configuration.import.result.created", icon: CircleCheck, tone: STATUS_TONE.success },
  existing: { label: "configuration.import.result.existing", icon: CircleDot, tone: STATUS_TONE.muted },
  left_out: { label: "configuration.import.result.leftOut", icon: CircleMinus, tone: STATUS_TONE.muted },
  not_attempted: { label: "configuration.import.result.status.notAttempted", icon: CircleSlash, tone: STATUS_TONE.muted },
  failed: { label: "configuration.import.result.failed", icon: CircleAlert, tone: STATUS_TONE.danger },
};

/** An outcome as a glyph and its word. */
function Outcome({ status }: { status: Status }): React.ReactElement {
  const t = useT();
  const entry = STATUS[status];
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-sm">
      <StatusGlyph entry={entry} size="size-4" />
      {t(entry.label)}
    </span>
  );
}

/**
 * One console's or one agent's result as a card, like the review's: its name (after it, a warning mark where its
 * account references fell back to a default account) heads the card, with its own outcome beside the name when it has
 * one (a console's, always), and the reason under the header when it failed. The rows follow as `children`.
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
  status?: Status;
  failure?: ImportFailure | null;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Card
      header={(headingId) => (
        <div className={`flex flex-wrap items-start justify-between gap-x-4 gap-y-1 ${CARD_HEADER_CLASS}`}>
          <CardTitle id={headingId} name={name} fallbacks={fallbacks ?? []} className="min-w-40 flex-1" />
          {status && <Outcome status={status} />}
        </div>
      )}
    >
      {failure && (
        <div className="groove-b px-4 py-3 last:border-b-0">
          <FailureReason failure={failure} />
        </div>
      )}
      {children}
    </Card>
  );
}

/**
 * One project's or account's row: its name (with its fallback mark) and outcome on a line that wraps the outcome under
 * the name when there is no room, what there is to say about the outcome under it, then the reason when it failed and
 * the sharing warning when sharing could not be turned on, each written out in the row.
 */
export function ResultRow({
  name,
  fallbacks,
  status,
  detail,
  failure,
  warning,
}: {
  name: string;
  fallbacks?: AccountFallback[];
  status: Status;
  detail?: string;
  failure?: ImportFailure | null;
  /** What went wrong beside an outcome that was reached: the sentence, and under it the remedy and Details of the
   * failure behind it, whose own summary would only say the sentence again. */
  warning?: { text: string; failure: ImportFailure };
}): React.ReactElement {
  return (
    <li className="flex flex-col gap-1.5 groove-b px-4 py-3 last:border-b-0">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
        <span className="flex min-w-40 flex-1 items-start gap-1">
          <ItemName>{name}</ItemName>
          <FallbackMark subject={name} fallbacks={fallbacks ?? []} size="text-sm" />
        </span>
        <Outcome status={status} />
      </div>
      {detail && <p className="text-sm text-app-muted [overflow-wrap:anywhere]" dir="auto">{detail}</p>}
      {failure && <FailureReason failure={failure} />}
      {warning && (
        <ToneLine tone="warning" stacked>
          <p dir="auto">{warning.text}</p>
          {/* Outside an Alert, Details defaults to danger; under a warning it takes the warning's colour. */}
          <AlertStatusContext.Provider value="warning">
            <FailureReason failure={warning.failure} withoutSummary />
          </AlertStatusContext.Provider>
        </ToneLine>
      )}
    </li>
  );
}
