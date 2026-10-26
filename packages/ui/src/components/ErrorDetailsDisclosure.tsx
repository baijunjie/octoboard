import React, { useId } from "react";

import { useT } from "../i18n/react";
import { DiagnosticBlock } from "./DiagnosticBlock";

/** The "Error details" disclosure around details a caller laid out itself. */
export function DetailsDisclosure({ children, summaryId }: { children: React.ReactNode; summaryId?: string }): React.ReactElement {
  const t = useT();
  return (
    <details className="min-w-0 max-w-full">
      <summary id={summaryId}>{t("error.diagnostics")}</summary>
      <div className="mt-2 flex min-w-0 max-w-full flex-col gap-3">{children}</div>
    </details>
  );
}

/** The "Error details" disclosure around one block of raw text. */
export function DiagnosticDetails({ text }: { text: string }): React.ReactElement {
  const summaryId = useId();
  return (
    <DetailsDisclosure summaryId={summaryId}>
      <DiagnosticBlock labelledBy={summaryId}>{text}</DiagnosticBlock>
    </DetailsDisclosure>
  );
}
