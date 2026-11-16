import React, { createContext, useContext, useEffect, useRef, useState } from "react";

import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import type { RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonEntryInput, RemoteDaemonHandedOutcome, RemoteDaemonStatus, RemoteDaemonsCapability } from "../platform";
import { useRemoteDaemonConnect } from "./useRemoteDaemonConnect";
import type { useRemoteDaemons } from "./useRemoteDaemons";

/** The outcomes that ask the owner something, as against the failures, which are only reported. */
const QUESTIONS: ReadonlySet<RemoteDaemonConnectOutcome["outcome"]> = new Set([
  "secret_needed",
  "host_key_unknown",
  "device_key_refused",
  "octoboardd_not_found",
  "no_daemon",
  "application_not_running",
  "generation_mismatch",
]);

/** What the Remote devices page's sidebar and content area share. */
export interface RemoteDevicesState {
  entries: RemoteDaemonEntry[];
  entriesRead: boolean;
  statuses: RemoteDaemonStatus[];
  /** The device the content area shows, if any is saved; none while a new one is being added. */
  selectedId: string | undefined;
  /** A new device's form is in the content area. */
  adding: boolean;
  select: (id: string) => void;
  startAdd: () => void;
  cancelAdd: () => void;
  /** Saves a new device or changes one; a new one is then the one shown. */
  save: (entry: RemoteDaemonEntryInput) => Promise<void>;
  /** The id of the entry being connected, one at a time. */
  connecting: string | undefined;
  /** Whether the shell holds a connection to `entry` (a window, or the browser). */
  held: (entry: RemoteDaemonEntry) => boolean;
  /** Connects, or brings the connected one forward. */
  connect: (entry: RemoteDaemonEntry) => void;
  askRemove: (entry: RemoteDaemonEntry) => void;
}

const RemoteDevicesContext = createContext<RemoteDevicesState | undefined>(undefined);

export function useRemoteDevices(): RemoteDevicesState {
  const state = useContext(RemoteDevicesContext);
  if (!state) throw new Error("useRemoteDevices outside RemoteDevicesProvider");
  return state;
}

/**
 * What the Remote devices page of the application's own window keeps, which has no daemon behind it: the connection
 * being made, the new device's form, the Remove confirmation and every question and failure of a connection
 * (`useRemoteDaemonConnect`): the password, the host key to confirm, a changed host key, every failure, and the version
 * mismatch, whether the connection was started from the page or away from it (a menu's saved daemon), which the shell
 * hands over once it has something to show (`onHandedOver`). Its dialogs are drawn over whatever the window shows. It
 * stays mounted while the page is not shown, since a connection goes on when the page is left, and what the connection
 * asks or reports shows the page (`onShow`), as does the shell's request for it (`onPageRequested`) and a hand-over,
 * which also selects the device (`onSelect`). What is handed over waits while a connection is under way or a dialog of
 * the page is open, and is shown one at a time once the list has been read; one for an entry that is no longer saved is
 * dropped, and so is a question for an entry that has been connected since (a failure is still shown). The list itself
 * is read by the window (`list`), which the page's history entries name devices by. The entries' secrets never come to
 * the page: the password typed here goes with the connect request and is dropped. The sidebar (`RemoteDevicesSidebar`)
 * and the content area (`RemoteDevicePage`) read this through `useRemoteDevices`.
 */
export function RemoteDevicesProvider({ capability, list, selectedId, shown, onShow, onSelect, onAdd, onAddingChange, children }: {
  capability: RemoteDaemonsCapability;
  list: ReturnType<typeof useRemoteDaemons>;
  /** The device the window selects while the page shows. */
  selectedId: string | undefined;
  shown: boolean;
  onShow: () => void;
  onSelect: (id: string) => void;
  /** A new device's form was asked for. */
  onAdd: () => void;
  /** Told whenever a new device's form opens or closes, for what names the page elsewhere (the top bar's trail). */
  onAddingChange?: (adding: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const { entries, entriesRead, statuses, reload } = list;
  const { connecting, connect, resume, idle: connectIdle, dialogs } = useRemoteDaemonConnect(capability, reload, onShow);
  const [adding, setAdding] = useState(false);
  // Counts what ends or replaces the form of a new device, so that a save that is answered after the form was
  // cancelled, or the page left, selects nothing.
  const addEpoch = useRef(0);
  const [removing, setRemoving] = useState<RemoteDaemonEntry>();
  const [handed, setHanded] = useState<RemoteDaemonHandedOutcome[]>([]);
  const idle = connectIdle && removing === undefined;

  // Held by a window, which the shell brings forward, or for the browser, which it opens again: neither needs a
  // password.
  const held = (entry: RemoteDaemonEntry) => statuses.some((status) => status.id === entry.id);
  useEffect(() => capability.onPageRequested(onShow), [capability, onShow]);
  useEffect(
    () =>
      capability.onHandedOver((items) => {
        setHanded((waiting) => [...waiting, ...items]);
        onShow();
      }),
    [capability, onShow],
  );
  useEffect(() => {
    if (!idle || !entriesRead || handed.length === 0) return;
    const [next, ...rest] = handed;
    setHanded(rest);
    const entry = entries.find((one) => one.id === next.id);
    if (!entry || (QUESTIONS.has(next.outcome.outcome) && held(entry))) return;
    addEpoch.current++;
    setAdding(false);
    onSelect(entry.id);
    resume(entry, next.outcome);
  }, [idle, entriesRead, entries, statuses, handed]);
  useEffect(() => onAddingChange?.(adding), [adding]);
  // A form being filled in is dropped with the page.
  useEffect(() => {
    if (shown) return;
    addEpoch.current++;
    setAdding(false);
  }, [shown]);

  const state: RemoteDevicesState = {
    entries,
    entriesRead,
    statuses,
    selectedId: adding ? undefined : selectedId,
    adding,
    select: (id) => {
      addEpoch.current++;
      setAdding(false);
      onSelect(id);
    },
    startAdd: () => {
      addEpoch.current++;
      setAdding(true);
      onAdd();
    },
    cancelAdd: () => {
      addEpoch.current++;
      setAdding(false);
    },
    save: async (entry) => {
      const epoch = addEpoch.current;
      const saved = await capability.save(entry);
      // The new device is selected once the list holds it, so that nothing else is shown, and recorded, in between.
      await reload();
      if (entry.id !== null || epoch !== addEpoch.current) return;
      setAdding(false);
      onSelect(saved.id);
    },
    connecting,
    held,
    connect: (entry) => connect(entry, held(entry)),
    askRemove: setRemoving,
  };

  return (
    <RemoteDevicesContext.Provider value={state}>
      {children}
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
    </RemoteDevicesContext.Provider>
  );
}
