import React from "react";

import { useT } from "../../i18n/react";
import { FailureBody } from "../alerts/errorParts";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import { retryCanHelp } from "../../errors/errorRecovery";
import { StatusMark, type MarkTone } from "./StatusMark";
import { Button } from "../../primitives";

/**
 * The mark of something that failed to be read, beside what is still shown: a glyph named by `label`
 * whose popover says why (`FailureBody`: `summary` or the failure's own sentence, its remedy and the
 * raw text it carried), after any sentence of the caller's (`children`), and ends with Try again,
 * which closes the popover and calls `onRetry`. A failure that asking again cannot mend
 * (`retryCanHelp`), such as the state of the repository, gets no Try again, as it would only give
 * the same answer. `retrying` shows the button busy while a retry is out.
 */
export function RetryStatusMark({
  error,
  label,
  onRetry,
  summary,
  children,
  tone,
  inHeader,
  markId,
  retrying,
}: {
  error: ErrorPresentation;
  label: string;
  onRetry: () => void;
  /** The opening sentence of the failure, in place of its own (see `FailureBody`). */
  summary?: React.ReactNode;
  /** What the popover says before the failure. */
  children?: React.ReactNode;
  tone?: MarkTone;
  inHeader?: boolean;
  markId?: string;
  retrying?: boolean;
}): React.ReactElement {
  const t = useT();
  return (
    <StatusMark
      tone={tone}
      label={label}
      inHeader={inHeader}
      markId={markId}
      actions={retryCanHelp(error) ? (close) => (
        <Button size="sm" variant="secondary" focusOnPress={false} pending={retrying} onPress={() => { close(); onRetry(); }}>
          {t("error.tryAgain")}
        </Button>
      ) : undefined}
    >
      {children}
      <FailureBody error={error} summary={summary} />
    </StatusMark>
  );
}
