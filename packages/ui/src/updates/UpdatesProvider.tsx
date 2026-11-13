import React, { createContext, useContext, useEffect, useState, useSyncExternalStore } from "react";

import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { ActionableError } from "../errors/actionableError";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { alertAppDataFailure, raiseError } from "../components/alerts/raiseError";
import { codeRecord } from "../errors/codes";
import { daemonMessage, NO_RECORDS } from "../errors/daemonMessage";
import { currentLanguage, t } from "../i18n/language";
import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty, useOptionalDaemon, type Daemon } from "../store";
import { createDaemonUpdateStore } from "./daemonUpdateStore";
import { effectiveUpdateChannel, updateAutoCheck, updateChannel } from "./updateChannel";
import { createUpdateStore, type UpdateFailure, type UpdateState, type UpdateStore } from "./updateStore";

const UpdatesContext = createContext<UpdateStore | undefined>(undefined);

/** What the user sees of a failure of something they started: the alert that stays until it is acknowledged. The updater's
 * own text is behind the alert's disclosure, as raw text always is. A failure the daemon names by a code (its own update)
 * is worded by that code's copy; the two that say the download itself was not what was published, or could not be put in
 * place, have no remedy in a connection or a disk. */
function reportUpdateFailure({ action, reason, detail, code }: UpdateFailure): void {
  const key = reason === "gone" ? "updates.error.install.gone" : action === "check" ? "updates.error.check" : "updates.error.install";
  // A code this client does not know is worded by the generic sentence, never by the code itself. A check's own failure
  // keeps the check's sentence, which says what is not known and that the current version keeps running.
  const own = code && action === "install" && codeRecord(code) ? daemonMessage(currentLanguage(), code, {}, code, NO_RECORDS) : undefined;
  const message = own ?? (action === "install" && reason !== "gone" ? undefined : t(key));
  const remedy = code === "daemon_update_unverified" || code === "daemon_update_replace_failed" ? undefined : t(`${key}.remedy`);
  const title = t(action === "check" ? "updates.error.check.title" : "updates.error.install.title");
  errorAlerts.report({
    message: message ?? title,
    title,
    error: new ActionableError(message ?? title, "none"),
    content: <div className="flex flex-col gap-4">{message && <p>{message}</p>}{remedy && <p>{remedy}</p>}</div>,
    details: detail ? <DiagnosticBlock>{detail}</DiagnosticBlock> : null,
  });
}

/** A request of the daemon's update that the daemon refused, as the error alert of what the user asked for. */
function reportRequestFailure(error: unknown, action: UpdateFailure["action"]): void {
  raiseError(error, { title: t(action === "check" ? "updates.error.check.title" : "updates.error.install.title") });
}

function createDaemonUpdateStoreFor(daemon: Daemon): UpdateStore {
  return createDaemonUpdateStore({ daemon, reportFailure: reportUpdateFailure, reportRequestFailure, reportSaveFailure: (error) => alertAppDataFailure(t("settings.saveFailedTitle"))(error) });
}

/**
 * Owns the application's update state for the window and runs the automatic checks, where there is something to update:
 * the desktop application, through its `updater`, or a daemon that updates itself (`snapshot.daemon_update` is not null),
 * through its own status (`createDaemonUpdateStore`); elsewhere it provides nothing, and the update notice and the
 * Settings section are absent. The platform's updater wins where there is one: a window of the desktop application
 * updates the application, whichever daemon it is connected to. Mounted once above every screen, the error boundary
 * included, inside the daemon's provider, so the checks run whichever screen is up and a crash that is recovered from does
 * not start the state over while an update is installed. The channel is the one the user chose, else the running build's
 * default (`effectiveUpdateChannel`); choosing another looks again at once, on the new one (the store drops the result of
 * a check that was asking about the old one). `firstCheckDelayMs` moves the first automatic check from its default of
 * `FIRST_CHECK_DELAY_MS`; `null` leaves out the automatic checks altogether, for a surface that must hold a state of its
 * own (the gallery).
 */
export function UpdatesProvider({ children, firstCheckDelayMs }: { children: React.ReactNode; firstCheckDelayMs?: number | null }): React.ReactElement {
  const { updater } = usePlatform();
  const daemon = useOptionalDaemon();
  const hasDaemonUpdate = useDaemonStoreOrEmpty((state) => !!state.daemonUpdate);
  const [desktopStore] = useState(() =>
    updater
      ? createUpdateStore({
        updater,
        getChannel: (currentVersion) => effectiveUpdateChannel(updateChannel.get(), currentVersion),
        setChannel: (channel) => updateChannel.set(channel),
        getAutoCheck: () => updateAutoCheck.get(),
        setAutoCheck: (value) => updateAutoCheck.set(value),
        reportFailure: reportUpdateFailure,
      })
      : undefined,
  );
  // The daemon's store lives as long as its connection does and the daemon updates itself: one made for another daemon, or
  // none, is replaced while rendering.
  const wanted = !updater && hasDaemonUpdate ? daemon : undefined;
  const [held, setHeld] = useState(() => ({ wanted, store: wanted && createDaemonUpdateStoreFor(wanted) }));
  if (held.wanted !== wanted) setHeld({ wanted, store: wanted && createDaemonUpdateStoreFor(wanted) });
  const store = desktopStore ?? held.store;

  useEffect(() => store?.start(firstCheckDelayMs), [store, firstCheckDelayMs]);

  return <UpdatesContext.Provider value={store}>{children}</UpdatesContext.Provider>;
}

const NO_UPDATES = () => () => {};

/** The update store alone, without subscribing to its state, or `undefined` where nothing updates itself. */
export function useUpdateStore(): UpdateStore | undefined {
  return useContext(UpdatesContext);
}

/** What `select` makes of the update state, re-rendering the caller only when that changes (compared with `Object.is`,
 * so `select` returns a primitive or an unchanged reference), or `undefined` where nothing updates itself.
 * The download's progress changes the state on every tick: a component that does not draw it reads through this. */
export function useUpdateSelector<T>(select: (state: UpdateState) => T): T | undefined {
  const store = useUpdateStore();
  return useSyncExternalStore(store?.subscribe ?? NO_UPDATES, () => (store ? select(store.getState()) : undefined));
}

/** The update store and its whole state, which a download's progress changes on every tick, or `undefined` where the
 * platform cannot update itself. */
export function useUpdates(): { store: UpdateStore; state: UpdateState } | undefined {
  const store = useUpdateStore();
  const state = useSyncExternalStore(store?.subscribe ?? NO_UPDATES, () => store?.getState());
  return store && state ? { store, state } : undefined;
}
