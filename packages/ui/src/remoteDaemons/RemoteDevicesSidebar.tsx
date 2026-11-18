import { Plug, Plus, Server, Trash2 } from "lucide-react";
import React, { useRef } from "react";

import { RowControls, RowIconGroup, RowIconSlot } from "../components/controls/RowControls";
import { ActionMenu } from "../components/menus/ActionMenu";
import { useListMotion } from "../components/motion/useListMotion";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PANE_HEADER_FRAME_CLASS } from "../components/panel/paneHeaderFrame";
import { RowIconButton } from "../components/rows/RowIconButton";
import { RowLabel, TreeRow } from "../components/rows/TreeRow";
import { FadedScroller } from "../components/scroll/scrollFade";
import { StatusBadge } from "../components/status/StatusBadge";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { RemoteDaemonEntry } from "../platform";
import { CONNECTION_STATE_WORDS, connectionState, type ConnectionState } from "./connectionState";
import { useRemoteDevices } from "./RemoteDevices";

/** The sidebar's content while the Remote devices page shows: a header with the page's title and Add remote daemon, and
 * below it the saved devices in the order they were saved, one row each: the server glyph, the name, where the device
 * is connected a badge with how, and the row's menu (Connect or Open, Remove), also on a right-click. The selected row
 * is marked as the current item. With none saved, one compact empty row stands in the list's place, whose press starts
 * Add remote daemon. The menu's Connect and Open are unavailable while another device is being connected, as the
 * page's button is. */
export function RemoteDevicesSidebar(): React.ReactElement {
  const t = useT();
  const { entries, entriesRead, statuses, selectedId, select, startAdd, connecting, connect, askRemove } = useRemoteDevices();
  const listRef = useListMotion<HTMLDivElement>();
  return (
    <>
      <div className={`${PANE_HEADER_FRAME_CLASS} gap-2 px-(--pane-inset)`}>
        <h2 className="min-w-0 flex-1 text-base font-semibold">
          <RowLabel title={t("remoteDevices.label")} className="block">{t("remoteDevices.label")}</RowLabel>
        </h2>
        <RowIconGroup>
          <RowIconButton icon={Plus} label={t("remoteDaemons.add")} onPress={startAdd} />
        </RowIconGroup>
      </div>
      <FadedScroller className="min-h-0 flex-1 px-(--row-fill-inset) pt-2 pb-2">
        {entriesRead && entries.length === 0 && (
          // One compact row in the list's place, whose press starts what fills it.
          <EmptyPanel compact icon={Server} message={t("remoteDaemons.empty")} action={{ label: t("remoteDaemons.add"), onPress: startAdd }} />
        )}
        {entriesRead && entries.length > 0 && (
          <div ref={listRef} className="relative flex flex-col gap-0.5">
            {entries.map((entry) => {
              const status = statuses.find((one) => one.id === entry.id);
              return (
                <div key={entry.id} data-list-motion={entry.id}>
                  <DeviceRow
                    entry={entry}
                    state={status && connectionState(status)}
                    selected={entry.id === selectedId}
                    connectable={connecting === undefined || connecting === entry.id}
                    onSelect={select}
                    onConnect={connect}
                    onRemove={askRemove}
                  />
                </div>
              );
            })}
          </div>
        )}
      </FadedScroller>
    </>
  );
}

function DeviceRow({ entry, state, selected, connectable, onSelect, onConnect, onRemove }: {
  entry: RemoteDaemonEntry;
  state: ConnectionState | undefined;
  selected: boolean;
  /** No other device is being connected. */
  connectable: boolean;
  onSelect: (id: string) => void;
  onConnect: (entry: RemoteDaemonEntry) => void;
  onRemove: (entry: RemoteDaemonEntry) => void;
}): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const name = isolate(entry.name);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={state ? t("session.status.named", { name, status: t(CONNECTION_STATE_WORDS[state]) }) : entry.name}
      selected={selected}
      onActivate={() => onSelect(entry.id)}
    >
      <RowIconSlot placement="leading"><Server aria-hidden="true" className="size-4 text-app-muted" /></RowIconSlot>
      <RowLabel title={entry.name} className="min-w-0 flex-1">{entry.name}</RowLabel>
      {state && (
        <StatusBadge color={state === "reconnecting" ? "warning" : "success"} variant="soft">
          {t(CONNECTION_STATE_WORDS[state])}
        </StatusBadge>
      )}
      <RowControls>
        <ActionMenu
          label={t("remoteDaemons.actionsLabel", { name })}
          items={[
            { label: t(state ? "remoteDaemons.open" : "remoteDaemons.connect"), icon: Plug, disabled: !connectable, onClick: () => onConnect(entry) },
            "separator",
            { label: t("common.remove"), icon: Trash2, onClick: () => onRemove(entry), destructive: true },
          ]}
          contextTargetRef={rowRef}
        />
      </RowControls>
    </TreeRow>
  );
}
