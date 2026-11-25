import React from "react";

import type { ErrorPresentation } from "../../errors/errorPresentation";
import { retryCanHelp } from "../../errors/errorRecovery";
import { useT } from "../../i18n/react";
import { DaemonErrorMessage } from "./DaemonErrorMessage";
import { errorDetails, errorRemedy } from "./errorParts";
import { failureRecoveryAction } from "./ErrorGuidance";
import { ErrorAlert, type AlertStatus } from "./ErrorAlert";

/** The Alert of a failure that is the state of what the screen shows: `title` says what failed, the failure's sentence
 * is the description with the one main thing to do under it, Try again, and the rest (further explanation, then the raw
 * text: the system's message, the diagnostics) is behind the `ErrorAlert`'s Details button, left out when there is
 * none. `onRetry` is optional: without it, or for a failure no retry can change (`retryCanHelp`), the Alert has no Try
 * again. A failure whose remedy is a link out (a missing Git: Download Git) has the link before Try again. `pending`
 * keeps the button where it is, unavailable, while the retry runs, so focus on it is not lost; the failure stays shown
 * until the retry answers. `announce`, `announceDelayMs` and `attempt` (which makes a failure that repeats itself in
 * the same words announce again) are `ErrorAlert`'s. The button never takes focus from a press, so a retry started with
 * the pointer leaves focus where it was. `otherwise` is what the Alert says and offers in place of Try again where
 * there is none, because no retry can change the failure or `onRetry` is not given: the one thing to do about it, and
 * the button that does it, after any link out. A focused control removed by a new failure hands focus to Try again
 * or to the view's `focusFallback`. */
export function FailureAlert({
  textId,
  title,
  message,
  onRetry,
  otherwise,
  focusFallback,
  pending = false,
  announce,
  attempt,
  announceDelayMs,
  delayEachAttempt,
  status,
  className,
}: {
  /** For a field that is described by what the failure says, and not by its buttons (`ErrorAlert`'s). */
  textId?: string;
  title: React.ReactNode;
  message: ErrorPresentation;
  onRetry?: () => void;
  otherwise?: { remedy: React.ReactNode; action: React.ReactNode };
  focusFallback?: () => HTMLElement | null | undefined;
  pending?: boolean;
  announce: boolean;
  attempt?: number;
  announceDelayMs?: number;
  delayEachAttempt?: boolean;
  status?: AlertStatus;
  className?: string;
}): React.ReactElement {
  const t = useT();
  const action = failureRecoveryAction(t, message);
  // Only a link out stands among the buttons; any other recovery's control is not this alert's.
  const recoveryLink = action?.kind === "link" ? action : undefined;
  const retry = onRetry && retryCanHelp(message) ? { onPress: onRetry, pending } : undefined;
  return (
    <ErrorAlert
      textId={textId}
      className={className}
      title={title}
      announce={announce}
      announceKey={attempt}
      announceDelayMs={announceDelayMs}
      delayEachAttempt={delayEachAttempt}
      status={status}
      remedy={retry || !otherwise ? errorRemedy(message) : otherwise.remedy}
      details={errorDetails(message)}
      recoveryLink={recoveryLink}
      retry={retry}
      actions={retry ? undefined : otherwise?.action}
      focusFallback={focusFallback}
    >
      <DaemonErrorMessage error={message} part="summary" />
    </ErrorAlert>
  );
}
