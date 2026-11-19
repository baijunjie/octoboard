import { Eye, Pencil, Trash2 } from "lucide-react";
import React from "react";

import { ActionMenu } from "../../components/menus/ActionMenu";
import { FadeOverflow } from "../../components/text/FadeOverflow";
import { StatusBadge } from "../../components/status/StatusBadge";
import { useT } from "../../i18n/react";
import { isolate } from "../../i18n/isolate";
import type { MemoryIndexEntry } from "../../protocol";
import { BareButton } from "../../primitives";
import { MemoryUpdated } from "./MemoryUpdated";

/** One line of a memory: its summary on a line of its own, which fades where it is clipped and never wraps,
 * with the "This machine only" tag after it where the entry has it, and under it when and by whom the entry was last updated. Pressing the line shows the
 * entry; the menu beside it views, edits or deletes it. */
export function MemoryRow({ line, flagged, actionsRef, onFocus, onView, onEdit, onDelete }: {
  line: MemoryIndexEntry;
  /** The row's action menu button, which focus can be handed to. */
  actionsRef?: React.Ref<HTMLButtonElement>;
  /** Focus has moved into the row. */
  onFocus?: () => void;
  /** Whether entries of this memory can be marked machine-only, and so show the tag: a console's, not a project's. */
  flagged: boolean;
  onView: () => void;
  onEdit: () => void;
  onDelete: () => void;
}): React.ReactElement {
  const t = useT();
  return (
    <li data-memory-entry={line.id} onFocus={onFocus} className="flex items-start gap-2 py-3 first:pt-0 last:pb-0">
      <BareButton onPress={onView} className="flex min-w-0 flex-1 flex-col gap-1 rounded-md text-start outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus">
        <span className="flex min-w-0 items-center gap-2">
          <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 font-medium" titleWhenClipped={line.summary}>{line.summary}</FadeOverflow>
          {flagged && line.local && <span className="shrink-0"><StatusBadge>{t("memory.local")}</StatusBadge></span>}
        </span>
        <span className="text-xs text-app-muted">
          <MemoryUpdated updatedAt={line.updated_at} writer={line.updated_by} />
        </span>
      </BareButton>
      <ActionMenu
        triggerRef={actionsRef}
        label={t("memory.row.actions", { summary: isolate(line.summary) })}
        items={[
          { label: t("memory.view"), icon: Eye, onClick: onView },
          { label: t("common.edit"), icon: Pencil, onClick: onEdit },
          "separator",
          { label: t("common.delete"), icon: Trash2, onClick: onDelete, destructive: true },
        ]}
      />
    </li>
  );
}
