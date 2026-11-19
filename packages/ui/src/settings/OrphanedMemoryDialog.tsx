import { Brain } from "lucide-react";
import React, { useRef, useState } from "react";

import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { Dialog } from "../components/dialog/Dialog";
import { useRefocusIfLost } from "../components/dialog/dialogHooks";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PlainMarkedPath } from "../components/text/MarkedPath";
import { formatRelativeTime, isJustNow } from "../format/relativeTime";
import { useNow } from "../format/useNow";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { OrphanedMemory } from "../protocol";
import { Button } from "../primitives";
import { useDaemon, useDaemonStore } from "../store";

/** The key a place is named by in the list: its host and path. */
const placeKey = (place: OrphanedMemory) => `${place.host_id}\u0000${place.path}`;

/** The name the daemon knows the machine a place is on by, or nothing for one it does not list. */
function useKnownHostName(hostId: string): string | undefined {
  return useDaemonStore((s) => s.hosts?.get(hostId)?.name);
}

/** The machine a place is on, by the name the daemon knows it by, or a neutral word for one it does not list. */
function useHostName(hostId: string): string {
  const t = useT();
  return useKnownHostName(hostId) ?? t("settings.memory.leftBehind.unknownHost");
}

/** One place's memory left behind: its directory, and under it the machine, how many entries it holds and when the most
 * recent was written, with Delete. */
function PlaceRow({ place, onDelete }: { place: OrphanedMemory; onDelete: () => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const host = useHostName(place.host_id);
  return (
    <li className="flex items-center justify-between gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0 text-sm">
        <div className="font-medium text-foreground"><PlainMarkedPath path={place.path} /></div>
        <div className="text-app-muted">
          {isJustNow(place.updated_at, now)
            ? t("settings.memory.leftBehind.detailsNow", { host: isolate(host), count: place.entries })
            : t("settings.memory.leftBehind.details", { host: isolate(host), count: place.entries, time: formatRelativeTime(language, place.updated_at, now) })}
        </div>
      </div>
      <Button
        size="sm"
        variant="danger-soft"
        focusOnPress={false}
        describedBy=""
        label={t("settings.memory.leftBehind.deleteLabel", { path: isolate(place.path) })}
        onPress={onDelete}
      >
        {t("common.delete")}
      </Button>
    </li>
  );
}

/** The memory removed projects left behind, one place to a line, each deleted after asking, or all of it at once with
 * Delete all. The list is the caller's, kept live from the daemon's broadcast, so it follows deletions made here or
 * elsewhere while the dialog is open, and once the last place is gone the dialog says nothing is left. */
export function OrphanedMemoryDialog({ places, onClose }: { places: readonly OrphanedMemory[]; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [deleting, setDeleting] = useState<OrphanedMemory>();
  const [deletingAll, setDeletingAll] = useState(false);
  // A machine's name is a reference to it; the word for one the daemon does not list is not a name.
  const knownHost = useKnownHostName(deleting?.host_id ?? "");
  const host = knownHost === undefined ? t("settings.memory.leftBehind.unknownHost") : <InlineReference dir="auto" wrap>{knownHost}</InlineReference>;
  // A deleted place takes its Delete button with it, and focus with that: it goes to Close.
  useRefocusIfLost(() => closeRef.current, [places, deleting, deletingAll]);
  const entries = places.reduce((sum, place) => sum + place.entries, 0);
  return (
    <>
      <Dialog
        title={t("settings.memory.leftBehind.title")}
        onClose={onClose}
        size="lg"
        footer={
          <>
            <Button type="button" variant="danger-soft" disabled={places.length === 0} onPress={() => setDeletingAll(true)}>
              {t("settings.memory.leftBehind.deleteAll")}
            </Button>
            <Button ref={closeRef} type="button" variant="secondary" onPress={onClose}>
              {t("common.close")}
            </Button>
          </>
        }
      >
        {places.length === 0 ? (
          <EmptyPanel icon={Brain} message={t("settings.memory.leftBehind.empty")} />
        ) : (
          <ul aria-label={t("settings.memory.leftBehind.title")} className="flex flex-col [&>li]:hairline-b [&>li:last-child]:border-b-0">
            {places.map((place) => (
              <PlaceRow key={placeKey(place)} place={place} onDelete={() => setDeleting(place)} />
            ))}
          </ul>
        )}
      </Dialog>
      {deleting && (
        <ConfirmDialog
          title={t("settings.memory.leftBehind.deleteTitle")}
          errorTitle={t("settings.memory.leftBehind.deleteFailedTitle")}
          subject={<Message id="settings.memory.leftBehind.deleteSubject" params={{ path: <InlineReference monospace wrap>{deleting.path}</InlineReference>, host }} />}
          message={t("settings.memory.leftBehind.deleteMessage", { count: deleting.entries })}
          confirmLabel={t("common.delete")}
          destructive
          resetKey={placeKey(deleting)}
          onConfirm={async () => {
            await request({ type: "delete_orphaned_memory", host_id: deleting.host_id, path: deleting.path });
            setDeleting(undefined);
          }}
          onCancel={() => setDeleting(undefined)}
        />
      )}
      {deletingAll && (
        <ConfirmDialog
          title={t("settings.memory.leftBehind.deleteAllTitle")}
          errorTitle={t("settings.memory.leftBehind.deleteFailedTitle")}
          subject={t("settings.memory.leftBehind.deleteAllSubject", { count: places.length })}
          message={t("settings.memory.leftBehind.deleteAllMessage", { count: entries })}
          confirmLabel={t("settings.memory.leftBehind.deleteAll")}
          typeToConfirm={t("dialog.typeToConfirm.delete")}
          destructive
          onConfirm={async () => {
            await request({ type: "delete_orphaned_memories" });
            setDeletingAll(false);
          }}
          onCancel={() => setDeletingAll(false)}
        />
      )}
    </>
  );
}
