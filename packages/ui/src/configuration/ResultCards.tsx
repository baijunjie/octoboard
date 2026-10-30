import { CircleAlert, CircleCheck, CircleDot, CircleMinus, CircleSlash, TriangleAlert, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { AlertStatusContext } from "../components/AlertButton";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import type { AccountFallback, AccountOutcome, ImportFailure, ImportStatus } from "../protocol";
import { CARD_CLASS, CARD_HEADER_CLASS, FailureReason, FallbackMark } from "./importParts";

type Status = ImportStatus | AccountOutcome;

/** What each outcome is called, drawn and coloured. The colour only strengthens the word and the shape, which differ
 * between the outcomes. */
const STATUS: Record<Status, { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  created: { label: "configuration.import.result.created", icon: CircleCheck, tone: "text-success-soft-foreground" },
  existing: { label: "configuration.import.result.existing", icon: CircleDot, tone: "text-app-muted" },
  left_out: { label: "configuration.import.result.leftOut", icon: CircleMinus, tone: "text-app-muted" },
  not_attempted: { label: "configuration.import.result.status.notAttempted", icon: CircleSlash, tone: "text-app-muted" },
  failed: { label: "configuration.import.result.failed", icon: CircleAlert, tone: "text-danger-soft-foreground" },
};

/** An outcome as a glyph and its word. */
function Outcome({ status }: { status: Status }): React.ReactElement {
  const t = useT();
  const { label, icon: Icon, tone } = STATUS[status];
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-sm">
      <Icon aria-hidden="true" className={`size-4 shrink-0 ${tone}`} />
      {t(label)}
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
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={CARD_CLASS}>
      <div className={`flex flex-wrap items-start justify-between gap-x-4 gap-y-1 ${CARD_HEADER_CLASS}`}>
        <div className="flex min-w-40 flex-1 items-start gap-1">
          <h3 id={headingId} className="min-w-0 text-base font-semibold text-foreground [overflow-wrap:anywhere]" dir="auto">
            {name}
          </h3>
          <FallbackMark subject={name} fallbacks={fallbacks ?? []} size="text-base" />
        </div>
        {status && <Outcome status={status} />}
      </div>
      {failure && (
        <div className="groove-b px-4 py-3 last:border-b-0">
          <FailureReason failure={failure} />
        </div>
      )}
      {children}
    </section>
  );
}

/** The rows of a card, each divided from the next. */
export function ResultRows({ children }: { children: React.ReactNode }): React.ReactElement {
  return <ul className="flex flex-col">{children}</ul>;
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
          <span className="min-w-0 font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">{name}</span>
          <FallbackMark subject={name} fallbacks={fallbacks ?? []} size="text-sm" />
        </span>
        <Outcome status={status} />
      </div>
      {detail && <p className="text-sm text-app-muted [overflow-wrap:anywhere]" dir="auto">{detail}</p>}
      {failure && <FailureReason failure={failure} />}
      {warning && (
        <div className="flex items-start gap-2 text-sm">
          <TriangleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-soft-foreground" />
          <div className="flex min-w-0 flex-col gap-1.5">
            <p dir="auto">{warning.text}</p>
            {/* Outside an Alert, Details defaults to danger; under a warning it takes the warning's colour. */}
            <AlertStatusContext.Provider value="warning">
              <FailureReason failure={warning.failure} withoutSummary />
            </AlertStatusContext.Provider>
          </div>
        </div>
      )}
    </li>
  );
}
