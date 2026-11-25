import React, { useState } from "react";

import { RetryStatusMark } from "../components/status/RetryStatusMark";
import { errorPresentation } from "../errors/errorPresentation";
import { useT } from "../i18n/react";
import { Button } from "../primitives";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { NativeMemoryNote } from "./NativeMemoryNote";
import { OrphanedMemoryDialog } from "./OrphanedMemoryDialog";
import { SettingRow } from "./SettingRow";
import { useOrphanedMemory } from "./useOrphanedMemory";

/** Two rows: whether Octoboard's memory is on, its description naming the agents that keep a memory of their own
 * (`NativeMemoryNote`), and the memory removed projects left behind, with how many folders hold
 * some and a Manage button that lists them in a dialog. That row is there whatever the switch says, since turning memory
 * off deletes nothing; with nothing left behind its button is unavailable, saying so. Where the places could not be
 * listed, a mark before the button says why and offers to try again, and the button is unavailable for that reason. */
export function MemorySection(): React.ReactElement {
  const t = useT();
  const { places, status, failure, retry, retrying } = useOrphanedMemory();
  const [managing, setManaging] = useState(false);
  const ready = status === "ready";
  const failed = status === "failed";
  return (
    <>
      <DaemonSwitchSetting field="memory_enabled" detail={<NativeMemoryNote />} />
      <SettingRow searchId="memory.leftBehind" label={t("settings.memory.leftBehind.title")} description={t("settings.memory.leftBehind.description")}>
        <div className="flex items-center gap-3">
          {ready && places.length > 0 && <span className="text-sm text-app-muted tabular-nums">{t("settings.memory.leftBehind.count", { count: places.length })}</span>}
          {failed && (
            <RetryStatusMark tone="danger" label={t("settings.memory.leftBehind.listFailed")} error={errorPresentation(failure)} retrying={retrying} onRetry={retry} />
          )}
          <Button
            size="sm"
            variant="outline"
            unavailable={failed ? t("settings.memory.leftBehind.listFailed") : !ready ? true : places.length === 0 ? t("settings.memory.leftBehind.empty") : ""}
            onPress={() => setManaging(true)}
          >
            {t("settings.memory.leftBehind.manage")}
          </Button>
        </div>
      </SettingRow>
      {managing && <OrphanedMemoryDialog places={places} onClose={() => setManaging(false)} />}
    </>
  );
}
