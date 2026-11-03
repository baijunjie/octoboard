import React from "react";

import { ErrorAlert } from "../../components/alerts/ErrorAlert";
import { errorDetails } from "../../components/alerts/errorParts";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { ConsequenceLines } from "../../components/dialog/ConfirmConsequences";
import { useT } from "../../i18n/react";
import type { useProjectProcesses } from "../../process/useProjectProcesses";
import { StoppedRow } from "../ProcessStopOptions";
import { ConfirmSection } from "./ConfirmSection";

/** The confirmation section of the project processes a removal stops, one `StoppedRow` each and the count. It is
 * shown when there are any, or when a list could not be read: then a warning Alert in its place says so, since the
 * processes are stopped all the same. */
export function ProjectProcessesSection({ processes, divided }: { processes: ReturnType<typeof useProjectProcesses>; divided: boolean }): React.ReactElement {
  const t = useT();
  const { trees, error } = processes;
  return (
    <ConfirmSection heading={t("dialog.projectProcesses.heading")} shown={trees.length > 0 || error !== undefined} divided={divided}>
      {error && (
        <ErrorAlert status="warning" title={t(trees.length > 0 ? "dialog.projectProcesses.listPartlyFailedTitle" : "dialog.projectProcesses.listFailedTitle")} details={errorDetails(error)} announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}>
          {t("dialog.projectProcesses.listFailed")}
        </ErrorAlert>
      )}
      {trees.length > 0 && (
        <>
          <div className="flex min-w-0 flex-col gap-2">
            {trees.map((tree) => <StoppedRow key={tree.id} tree={tree} />)}
          </div>
          {/* After a failed list the count would read as the total, while the warning says unlisted ones are stopped too. */}
          {error === undefined && <ConsequenceLines lines={[{ key: "projectProcesses", text: t("dialog.projectProcesses.stopped", { count: trees.length }) }]} />}
        </>
      )}
    </ConfirmSection>
  );
}
