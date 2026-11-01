import type { ReactNode } from "react";
import type { StoreApi } from "zustand";

import { DaemonClient, DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../errors/daemonMessage";
import { daemonWsUrl, type DaemonAddress } from "../daemon";
import { currentLanguage } from "../i18n/language";
import type { Event, RequestBody } from "../protocol";
import { createOwnTrustNotices } from "./ownTrustNotices";
import { reducer, type Action } from "./reducer";
import { createStateStore, type State } from "./state";
import { broadcastToast, errorToast, type AlertOptions, type BroadcastToastEvent, type ToastRequest } from "./toasts";

/** The daemon connection and the state it feeds: one per window, created at startup (see
 * `main.tsx`) and alive until the window closes. The store is a vanilla one, so code outside React
 * can read and subscribe to it; components use `useDaemonStore`. */
export interface Daemon {
  store: StoreApi<State>;
  /** Sends a request and resolves/rejects with its correlated reply. Callers present failures,
   * allowing field validation to remain local and execution errors to require acknowledgement. */
  request: (body: RequestBody) => Promise<Event>;
  /** Raises an error alert for `error`, which may be anything a request rejected with. The alert is queued through
   * `onToast`, so `Toasts` words it with the session's names as the store has them when it shows it; see `AlertOptions`
   * for what the call site says about it. */
  raiseError: (error: unknown, alert: AlertOptions) => void;
  /** Confirms something the user just did, which has no inline place to say so (a path copied);
   * about no session, so the toast is the message alone. */
  toastNotice: (message: string, content?: ReactNode) => void;
  /** Calls `listener` for every toast to show from now on: the daemon's errors and notices that
   * belong to no request, and what `raiseError` and `toastNotice` are given. Returns the unsubscribe. */
  onToast: (listener: (toast: ToastRequest) => void) => () => void;
  /** Drops a trust prompt from the queue, whether it was answered or declined — declining leaves the
   * confirmation for the user in the terminal, and the daemon asks again for it only after a
   * `snapshot`. */
  dismissTrustPrompt: (session: string) => void;
  /** Retries the control connection right away after the automatic reconnect budget was spent. */
  reconnect: () => void;
  /** The URL of a session's terminal stream (`GET /ws/term/:session`), on the same daemon the
   * control connection goes to. */
  terminalUrl: (session: string) => string;
  /** Overrides the delay before a terminal's automatic reconnect attempt number `attempt` (from 0);
   * only the gallery sets it, so that a spent budget can be shown without waiting for it. */
  terminalReconnectDelay?: (attempt: number) => number;
}

export function createDaemon(address: DaemonAddress): Daemon {
  const store = createStateStore();
  const dispatch = (action: Action) => store.setState((state) => reducer(state, action));
  // The events that arrived together are one update: a burst renders once, not once per event.
  const dispatchEvents = (events: Event[]) =>
    store.setState((state) => events.reduce((current, event) => reducer(current, { kind: "event", event }), state));

  const client = new DaemonClient(daemonWsUrl(address, "/ws/control"));
  const toastListeners = new Set<(toast: ToastRequest) => void>();
  const pendingNotifications: ToastRequest[] = [];
  const emitToast = (toast: ToastRequest) => {
    if (!toastListeners.size) pendingNotifications.push(toast);
    else toastListeners.forEach((listener) => listener(toast));
  };

  const emitBroadcast = (event: BroadcastToastEvent) => emitToast(broadcastToast(event, store.getState(), currentLanguage()));
  const ownTrustNotices = createOwnTrustNotices(emitBroadcast);

  client.onEvents(dispatchEvents);
  client.onEvent((event) => {
    if (event.type === "snapshot") {
      ownTrustNotices.reset();
      return;
    }
    if (event.type === "session_notice" && ownTrustNotices.claim(event)) return;
    // A request's own `error` reply carries its request id and is delivered to its caller as a
    // rejected promise instead — only a genuine daemon broadcast (no id) is a toast.
    if (event.type === "session_notice" || (event.type === "error" && !event.id)) {
      emitBroadcast(event);
    }
  });
  client.onConnectionChange((state) => dispatch({ kind: "connection", state }));

  return {
    store,
    // A failure's message is worded here, in the language and with the names of the moment it
    // arrives, so every caller shows `err.message` as it always did.
    request: (body) => {
      const settleTrust = body.type === "confirm_trust" ? ownTrustNotices.begin(body.session) : undefined;
      return client.request(body).then(
        (reply) => {
          settleTrust?.();
          return reply;
        },
        (err: unknown) => {
          settleTrust?.(err instanceof DaemonRequestError ? err.code : undefined);
          if (!(err instanceof DaemonRequestError)) throw err;
          const message = daemonMessage(currentLanguage(), err.code, err.params, err.message, store.getState());
          throw new DaemonRequestError(message, err.code, err.params);
        },
      );
    },
    raiseError: (error, alert) => emitToast(errorToast(error, alert)),
    toastNotice: (message, content) => emitToast({ kind: "notice", message, content }),
    onToast: (listener) => {
      toastListeners.add(listener);
      pendingNotifications.splice(0).forEach(listener);
      return () => void toastListeners.delete(listener);
    },
    dismissTrustPrompt: (session) => dispatch({ kind: "dismiss_trust_prompt", session }),
    reconnect: () => client.reconnect(),
    terminalUrl: (session) => daemonWsUrl(address, `/ws/term/${session}`),
  };
}
