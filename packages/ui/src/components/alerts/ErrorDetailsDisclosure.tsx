import React, { useId } from "react";

import { useT } from "../../i18n/react";
import { TextDisclosure } from "../disclosure/TextDisclosure";
import { DiagnosticLabelContext } from "./DiagnosticBlock";

/** The "Error details" disclosure around details a caller laid out itself, which stand flush under the trigger
 * (`TextDisclosure`'s `code`): code blocks (`DiagnosticBlock`s, each possibly under a heading of its own), or a raw part
 * that sets itself in (a sharing failure's list, `SharingDetails`, indented by its own margin). A `DiagnosticBlock`
 * among them with no `labelledBy` of its own is labelled by the trigger. */
export function DetailsDisclosure({ children }: { children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const triggerId = useId();
  return (
    <div className="min-w-0 max-w-full">
      <TextDisclosure code id={triggerId} label={t("error.diagnostics")} contentClassName="flex min-w-0 max-w-full flex-col gap-3">
        <DiagnosticLabelContext.Provider value={triggerId}>{children}</DiagnosticLabelContext.Provider>
      </TextDisclosure>
    </div>
  );
}
