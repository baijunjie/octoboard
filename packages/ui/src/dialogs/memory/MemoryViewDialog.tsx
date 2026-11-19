import React from "react";

import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { Dialog } from "../../components/dialog/Dialog";
import { FadedScroller, useFadeRegion } from "../../components/scroll/scrollFade";
import { StatusBadge } from "../../components/status/StatusBadge";
import { useT } from "../../i18n/react";
import { Button } from "../../primitives";
import type { HeldMemory } from "./memoryState";
import { MemoryUpdated } from "./MemoryUpdated";

/** An entry of a memory in full: its summary, the "This machine only" tag where it has one, content, and when and by whom it was last
 * updated. The entry is read from `memory` as it stands, so a session's save shows without reopening the dialog, and a
 * deletion says so and leaves nothing to edit. */
export function MemoryViewDialog({ id, memory, flagged, onEdit, onClose }: {
  id: string;
  /** Whether entries of this memory can be marked machine-only, and so show the tag: a console's, not a project's. */
  flagged: boolean;
  memory: HeldMemory;
  /** Opens the entry's edit form over this dialog. */
  onEdit: () => void;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("memory.field.content") });
  const entry = memory.contents.get(id);
  return (
    <Dialog
      title={t("memory.entry.viewTitle")}
      onClose={onClose}
      size="lg"
      footer={
        <>
          <Button type="button" variant="secondary" onPress={onClose}>
            {t("common.close")}
          </Button>
          <Button type="button" unavailable={entry ? "" : t("memory.entryDeleted")} onPress={onEdit}>
            {t("common.edit")}
          </Button>
        </>
      }
    >
      <StatusAnnouncer text={entry ? undefined : t("memory.entryDeleted")} />
      {entry ? (
        <>
          <div className="flex flex-col gap-1">
            <div className="flex items-start gap-2">
              <p dir="auto" className="min-w-0 flex-1 font-medium break-words">{entry.summary}</p>
              {flagged && entry.local && <span className="shrink-0"><StatusBadge>{t("memory.local")}</StatusBadge></span>}
            </div>
            <p className="text-sm break-words text-app-muted">
              <MemoryUpdated updatedAt={entry.updated_at} writer={entry.updated_by} />
            </p>
          </div>
          <div className={`rounded-lg bg-(--control-fill) ${region.insetRing}`}>
            <FadedScroller region={region} className="max-h-96">
              <pre dir="auto" className="p-3 text-sm break-words whitespace-pre-wrap">{entry.content}</pre>
            </FadedScroller>
          </div>
        </>
      ) : (
        <p className="text-sm text-app-muted">{t("memory.entryDeleted")}</p>
      )}
    </Dialog>
  );
}
