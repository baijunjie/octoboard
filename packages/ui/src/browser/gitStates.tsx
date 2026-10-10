import { Button, Spinner } from "@heroui/react";
import { TriangleAlert } from "lucide-react";
import React, { useRef } from "react";

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
    <div ref={ref} className="flex flex-1 flex-col items-center justify-center gap-3 px-4 text-center text-sm">
      <TriangleAlert aria-hidden="true" className="size-6 text-danger" />
      <div role="alert" className="flex flex-col gap-1">
        <p className="font-medium">{title}</p>
        <p className="text-muted">{message}</p>
      </div>
      <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => onRetry(ref.current)}>
        {t("error.tryAgain")}
      </Button>
    </div>
  );
}
