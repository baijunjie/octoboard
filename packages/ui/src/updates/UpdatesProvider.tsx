import React, { createContext, useContext, useEffect, useRef, useState, useSyncExternalStore } from "react";

import { DiagnosticBlock } from "../components/DiagnosticBlock";
import { ActionableError } from "../errorRecovery";
import { errorAlerts } from "../errorAlerts";
import { t } from "../i18n/language";
import { usePlatform } from "../platform/react";
import { updateChannel } from "./updateChannel";
import { createUpdateStore, type UpdateFailure, type UpdateState, type UpdateStore } from "./updateStore";

const UpdatesContext = createContext<UpdateStore | undefined>(undefined);

/** What the user sees of a failure of something they started: the alert that stays until it is acknowledged. The
 * updater's own text is behind the alert's disclosure, as raw text always is. */
function reportUpdateFailure({ action, reason, detail }: UpdateFailure): void {
  const key = reason === "gone" ? "updates.error.install.gone" : action === "check" ? "updates.error.check" : "updates.error.install";
  const message = t(key);
  const remedy = t(`${key}.remedy`);
  errorAlerts.report({
    message,
    error: new ActionableError(message, "none"),
    content: <><p>{message}</p><p>{remedy}</p></>,
    details: detail ? <DiagnosticBlock>{detail}</DiagnosticBlock> : null,
  });
}

/**
 * Owns the application's update state for the window and runs the automatic checks, where the platform can update
 * itself (`updater`); elsewhere it provides nothing, and the update notice and Settings rows are absent. Mounted once
 * above every screen, the error boundary included, so the checks run whichever screen is up and a crash that is
 * recovered from does not start the state over while an update is installed. A change of channel looks again at once,
 * on the new one (the store drops the result of a check that was asking about the old one). `firstCheckDelayMs` moves
 * the first automatic check from its default of `FIRST_CHECK_DELAY_MS`; `null` leaves out the automatic checks
 * altogether, for a surface that must hold a state of its own (the gallery).
 */
export function UpdatesProvider({ children, firstCheckDelayMs }: { children: React.ReactNode; firstCheckDelayMs?: number | null }): React.ReactElement {
  const { updater } = usePlatform();
  const [store] = useState(() => (updater ? createUpdateStore({ updater, getChannel: updateChannel.get, reportFailure: reportUpdateFailure }) : undefined));
  const channel = updateChannel.useValue();
  const checkedChannel = useRef(channel);

  useEffect(() => store?.start(firstCheckDelayMs), [store, firstCheckDelayMs]);
  useEffect(() => {
    if (checkedChannel.current === channel) return;
    checkedChannel.current = channel;
    void store?.check("automatic");
  }, [store, channel]);

  return <UpdatesContext.Provider value={store}>{children}</UpdatesContext.Provider>;
}

const NO_UPDATES = () => () => {};

/** The update store alone, without subscribing to its state, or `undefined` where the platform cannot update itself. */
export function useUpdateStore(): UpdateStore | undefined {
  return useContext(UpdatesContext);
}

/** What `select` makes of the update state, re-rendering the caller only when that changes (compared with `Object.is`,
 * so `select` returns a primitive or an unchanged reference), or `undefined` where the platform cannot update itself.
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
