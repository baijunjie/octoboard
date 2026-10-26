import React, { useId } from "react";

import { useT } from "../i18n/react";
import { DiagnosticLabelContext } from "./DiagnosticBlock";

/** The "Error details" disclosure around details a caller laid out itself; a `DiagnosticBlock` among them with no
 * `labelledBy` of its own is labelled by its summary. */
export function DetailsDisclosure({ children }: { children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const summaryId = useId();
  return (
    <details className="min-w-0 max-w-full">
      <summary id={summaryId}>{t("error.diagnostics")}</summary>
      <DiagnosticLabelContext.Provider value={summaryId}>
        <div className="mt-2 flex min-w-0 max-w-full flex-col gap-3">{children}</div>
      </DiagnosticLabelContext.Provider>
    </details>
  );
}
