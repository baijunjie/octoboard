import { Plus, Server } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { useListMotion } from "../components/motion/useListMotion";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { StatusBadge } from "../components/status/StatusBadge";
import { InlineReference } from "../i18n/InlineReference";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonEntry, RemoteDaemonsCapability } from "../platform";
import { usePlatform } from "../platform/react";
import { Button } from "../primitives";
import { CONNECTION_STATE_WORDS, connectionState } from "../remoteDaemons/connectionState";
import { RemoteDaemonDialog } from "../remoteDaemons/RemoteDaemonDialog";
import { useRemoteDaemonConnect } from "../remoteDaemons/useRemoteDaemonConnect";
import { useRemoteDaemons } from "../remoteDaemons/useRemoteDaemons";
import { ContentPanel } from "./ContentPanel";
import { BareTitleBar } from "./TitleBar";

/**
 * The page a new remote window opens on (`?connect=1`, see `windowKind`), with no daemon behind it: the saved remote
 * daemons with Connect, Edit and Remove, and Add remote daemon. Connecting happens here, and every question and failure
 * of it is shown in this window (`useRemoteDaemonConnect`): the password, the host key to confirm, a changed host key,
 * every failure, and the version mismatch. Once connected the shell turns this window into the daemon's own, or closes
 * it for the window the daemon already has open. `entryId` starts connecting to that entry at once, as if it had been
 * chosen. The entries' secrets never come to the page: the password typed here goes with the connect request and is
 * dropped.
 */
export function ConnectPage({ entryId }: { entryId?: string }): React.ReactElement {
  const { remoteDaemons } = usePlatform();
  return (
    <div className="flex h-full flex-col">
      <BareTitleBar />
      <ContentPanel bare>
        {remoteDaemons && <ConnectList capability={remoteDaemons} entryId={entryId} />}
      </ContentPanel>
    </div>
  );
}

function ConnectList({ capability, entryId }: { capability: RemoteDaemonsCapability; entryId: string | undefined }): React.ReactElement {
  const t = useT();
  const region = useFadeRegion({ label: t("remoteDaemons.label") });
  const { entries, statuses, statusesRead, reload } = useRemoteDaemons(capability);
  const listRef = useListMotion<HTMLUListElement>();
  const { connecting, connect, dialogs } = useRemoteDaemonConnect(capability, reload);
  const [editing, setEditing] = useState<RemoteDaemonEntry | "new">();
  const [removing, setRemoving] = useState<RemoteDaemonEntry>();
  const started = useRef(false);

  // Held by a window, which the shell brings forward, or for the browser, which it opens again: neither needs a
  // password.
  const held = (entry: RemoteDaemonEntry) => statuses.some((status) => status.id === entry.id);
  // The entry the page was opened for is connected to once, as soon as it is in the list and the states were read, so a
  // held one is not asked for a password.
  useEffect(() => {
    const entry = entries.find((one) => one.id === entryId);
    if (!entry || !statusesRead || started.current) return;
    started.current = true;
    connect(entry, held(entry));
  }, [entries, statusesRead, entryId]);
  const add = () => setEditing("new");

  return (
    <main className={`flex min-h-0 flex-1 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col">
        <div className="m-auto flex w-full min-w-0 max-w-xl flex-col gap-4 p-8">
          <h1 className="text-lg font-semibold">{t("remoteDaemons.label")}</h1>
          <p className="text-sm text-app-muted"><Message id="remoteDaemons.connectPage.lead" /></p>
          {entries.length === 0 ? (
            <EmptyPanel icon={Server} message={t("remoteDaemons.empty")} action={{ label: t("remoteDaemons.add"), icon: Plus, onPress: add }} />
          ) : (
            <ul ref={listRef} className="relative flex flex-col">
              {entries.map((entry) => {
                const status = statuses.find((one) => one.id === entry.id);
                const state = status && connectionState(status);
                return (
                  <li key={entry.id} data-list-motion={entry.id} className="flex items-center justify-between gap-4 py-3 hairline-b [&:not(:has(~[data-list-motion]))]:border-b-0">
                    <div className="min-w-0 text-sm">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span dir="auto" className="min-w-0 font-medium [overflow-wrap:anywhere]">{entry.name}</span>
                        {state && (
                          <StatusBadge color={state === "reconnecting" ? "warning" : "success"} variant="soft">
                            {t(CONNECTION_STATE_WORDS[state])}
                          </StatusBadge>
                        )}
                      </div>
                      <div dir="ltr" className="font-mono text-app-muted [overflow-wrap:anywhere]">{`${entry.user}@${entry.host}:${entry.port}`}</div>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <Button size="sm" variant="primary" focusOnPress={false} pending={connecting === entry.id} disabled={connecting !== undefined && connecting !== entry.id} describedBy="" label={t(state ? "remoteDaemons.openLabel" : "remoteDaemons.connectLabel", { name: isolate(entry.name) })} onPress={() => connect(entry, held(entry))}>
                        {/* A connected one is only brought forward, or opened in the browser again. */}
                        {t(state ? "remoteDaemons.open" : "remoteDaemons.connect")}
                      </Button>
                      <Button size="sm" variant="secondary" focusOnPress={false} describedBy="" label={t("remoteDaemons.editLabel", { name: isolate(entry.name) })} onPress={() => setEditing(entry)}>
                        {t("common.edit")}
                      </Button>
                      <Button size="sm" variant="danger-soft" focusOnPress={false} describedBy="" label={t("remoteDaemons.removeLabel", { name: isolate(entry.name) })} onPress={() => setRemoving(entry)}>
                        {t("common.remove")}
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {entries.length > 0 && (
            <div>
              <Button size="sm" variant="secondary" focusOnPress={false} onPress={add}>
                <Plus aria-hidden="true" className="size-4" />
                {t("remoteDaemons.add")}
              </Button>
            </div>
          )}
        </div>
      </FadedScroller>
      {editing && (
        <RemoteDaemonDialog
          entry={editing === "new" ? undefined : editing}
          onSave={async (entry) => {
            await capability.save(entry);
            reload();
          }}
          onClose={() => setEditing(undefined)}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={t("remoteDaemons.removeTitle")}
          errorTitle={t("remoteDaemons.removeFailedTitle")}
          subject={<Message id="remoteDaemons.removeSubject" params={{ name: <InlineReference dir="auto" wrap>{removing.name}</InlineReference> }} />}
          message={t("remoteDaemons.removeMessage")}
          confirmLabel={t("common.remove")}
          destructive
          resetKey={removing.id}
          onConfirm={async () => {
            await capability.remove(removing.id);
            setRemoving(undefined);
            reload();
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
      {dialogs}
    </main>
  );
}
