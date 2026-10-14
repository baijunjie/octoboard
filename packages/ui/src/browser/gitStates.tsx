import { Button, Spinner } from "@heroui/react";
import React, { useRef } from "react";

import { ErrorAlert } from "../components/ErrorAlert";
import { useT } from "../i18n/react";

/** The Git mode's loading line: what is being read. Seen only; a `StatusAnnouncer` mounted beside
 * it says it. */
export function GitLoading({ label }: { label: string }): React.ReactElement {
  return (
    <div aria-hidden="true" className="flex flex-1 items-center justify-center gap-2 text-sm text-muted">
      <Spinner size="sm" aria-hidden="true" />
      {label}
    </div>
  );
}

/** Something the Git mode could not read: why, and a way to try again. `from` is the control
 * pressed, which goes away with the failure. */
export function GitFailure({
  title,
  message,
  onRetry,
}: {
  title: string;
  message: string;
  onRetry: (from: Element | null) => void;
}): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="flex min-w-0 flex-1 flex-col items-center justify-center gap-3 p-4 text-sm">
      <ErrorAlert title={title} className="w-full max-w-lg">
        {message}
      </ErrorAlert>
      <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => onRetry(ref.current)}>
        {t("error.tryAgain")}
      </Button>
    </div>
  );
}
