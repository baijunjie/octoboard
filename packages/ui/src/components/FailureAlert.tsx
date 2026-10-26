import React from "react";

import type { ErrorPresentation } from "../errorPresentation";
import { retryCanHelp } from "../errorRecovery";
import { useT } from "../i18n/react";
import { AlertButton } from "./AlertButton";
import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { errorDetails, errorRemedy } from "./errorParts";
import { ErrorAlert } from "./ErrorAlert";

/** The Alert of a failure that is the state of what the screen shows: `title` says what failed, the
 * failure's sentence is the description with the one main thing to do under it, Try again, and the rest
 * (further explanation, then the raw text: the system's message, the diagnostics) is behind the
 * `ErrorAlert`'s Details button, left out when there is none.
 * `onRetry` is optional: without it, or for a failure no retry can change (`retryCanHelp`), the Alert
 * has no Try again. `pending` keeps the button where it is, unavailable, while the retry
 * runs, so focus on it is not lost; the failure stays shown until the retry answers. `announce`,
 * `announceDelayMs` and `attempt` (which makes a failure that repeats itself in the same words
 * announce again) are `ErrorAlert`'s. The button never takes focus from a press, so a retry started
 * with the pointer leaves focus where it was. */
export function FailureAlert({
  id,
  title,
  message,
  onRetry,
  pending = false,
  announce,
  attempt,
  announceDelayMs,
  status,
  className,
}: {
  /** For a field that is described by the alert. */
  id?: string;
  title: React.ReactNode;
  message: ErrorPresentation;
  onRetry?: () => void;
  pending?: boolean;
  announce: boolean;
  attempt?: number;
  announceDelayMs?: number;
  status?: "danger" | "warning";
  className?: string;
}): React.ReactElement {
  const t = useT();
  return (
    <ErrorAlert
      id={id}
      className={className}
      title={title}
      announce={announce}
      announceKey={attempt}
      announceDelayMs={announceDelayMs}
      status={status}
      remedy={errorRemedy(message)}
      details={errorDetails(message)}
      actions={onRetry && retryCanHelp(message) && (
        <AlertButton kind="action" size="sm" preventFocusOnPress isPending={pending} onPress={onRetry}>
          {t("error.tryAgain")}
        </AlertButton>
      )}
    >
      <DaemonErrorMessage error={message} part="summary" />
    </ErrorAlert>
  );
}
