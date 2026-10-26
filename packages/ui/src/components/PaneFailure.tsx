import { CircleAlert } from "lucide-react";
import React, { useRef } from "react";

import type { ErrorPresentation } from "../errorPresentation";
import { retryCanHelp } from "../errorRecovery";
import { useT } from "../i18n/react";
import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { errorDetails, errorRemedy } from "./errorParts";
import { EmptyPanel } from "./EmptyPanel";

/** A pane's whole subject that could not be read, in its place: the empty-state shape in the danger
 * tone, centred in the pane (see "Errors shown among other content" in `docs/ui-accessibility.md`).
 * `title` is the heading saying what could not be loaded; the line under it is `summary`, or the
 * failure's own sentence when `message` is given; under it goes the one main thing to do, `remedy`
 * or what `message` holds of it, and the rest is behind Details (further explanation, then raw
 * text): `details`, or what `message` holds of it. `onRetry` is optional (without it, or for a failure
 * no retry can change (`retryCanHelp`), there is no Try again) and gets the element the pane is drawn in, which the pressed button does not
 * outlive, to hand focus on from. `announce` is `ErrorAlert`'s: for a failure that appears after the
 * pane has painted; `attempt`, `announceDelayMs` and `pending` are `FailureAlert`'s. `landing` makes Try again the landing of an F6 cycle (`data-region-landing`). */
export function PaneFailure({
  title,
  message,
  summary,
  remedy,
  details,
  onRetry,
  announce,
  attempt,
  announceDelayMs,
  pending,
  landing,
}: {
  title: React.ReactNode;
  message?: ErrorPresentation;
  summary?: React.ReactNode;
  remedy?: React.ReactNode;
  details?: React.ReactNode;
  onRetry?: (from: Element | null) => void;
  announce: boolean;
  attempt?: number;
  announceDelayMs?: number;
  pending?: boolean;
  landing?: boolean;
}): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="flex min-h-0 min-w-0 flex-1 flex-col justify-center-safe">
      <EmptyPanel
        tone="danger"
        icon={CircleAlert}
        message={title}
        description={summary ?? (message !== undefined && <DaemonErrorMessage error={message} part="summary" />)}
        remedy={remedy ?? (message !== undefined && errorRemedy(message))}
        details={details ?? (message !== undefined && errorDetails(message))}
        action={onRetry && (message === undefined || retryCanHelp(message)) ? { label: t("error.tryAgain"), variant: "primary", isPending: pending, landing, onPress: () => onRetry(ref.current) } : undefined}
        announce={announce}
        announceKey={attempt}
        announceDelayMs={announceDelayMs}
      />
    </div>
  );
}
