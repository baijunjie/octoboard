import type { ReactNode } from "react";
import type { StoreApi } from "zustand";

import { DaemonClient, DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { daemonWsUrl, type DaemonAddress } from "../daemon";
import { noticeErrorPresentation, noticeKind } from "../errorPresentation";
import type { ErrorRecovery } from "../errorRecovery";
import { currentLanguage } from "../i18n/language";
import type { Event, RequestBody } from "../protocol";
import { reducer, type Action } from "./reducer";
import { createStateStore, type State } from "./state";
import { errorToast, type ToastRequest } from "./toasts";

/** The daemon connection and the state it feeds: one per window, created at startup (see
 * `main.tsx`) and alive until the window closes. The store is a vanilla one, so code outside React
 * can read and subscribe to it; components use `useDaemonStore`. */
export interface Daemon {
  store: StoreApi<State>;
  /** Sends a request and resolves/rejects with its correlated reply. Callers present failures,
   * allowing field validation to remain local and execution errors to require acknowledgement. */
  request: (body: RequestBody) => Promise<Event>;
  /** `session` is passed by a caller that knows which session the message is about, so the toast
   * can name it; see `ToastRequest.session`. `title` names what failed in plain words, with no name in it; see
   * `ToastRequest.title`. */
  toastError: (message: string | Error, session?: string, title?: string, recovery?: ErrorRecovery) => void;
  /** Confirms something the user just did, which has no inline place to say so (a path copied);
   * about no session, so the toast is the message alone. */
  toastNotice: (message: string, content?: ReactNode) => void;
  /** Calls `listener` for every toast to show from now on: the daemon's errors and notices that
   * belong to no request, and what `toastError` and `toastNotice` are given. Returns the unsubscribe. */
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

  client.onEvents(dispatchEvents);
  client.onEvent((event) => {
    if (event.type === "session_notice") {
      // A failed launch is presented as the failure it reports, laid out beneath a title that does
      // not repeat the reason; any other notice as itself.
      const failure = noticeErrorPresentation(event.code, event.params, event.message);
      emitToast({
        kind: noticeKind(event.code, event.params),
        message: daemonMessage(currentLanguage(), event.code, event.params, event.message, store.getState()),
        daemonTitle: failure ? new DaemonRequestError(event.message, `${event.code}.title`, event.params) : undefined,
        daemonError: failure ?? new DaemonRequestError(event.message, event.code, event.params),
        session: event.session,
      });
    } else if (event.type === "error" && !event.id) {
      // A request's own `error` reply carries its request id and is delivered to its caller as a
      // rejected promise instead — only a genuine daemon broadcast (no id) is a toast.
      emitToast({
        kind: "error",
        daemonError: new DaemonRequestError(event.message, event.code, event.params),
        message: daemonMessage(currentLanguage(), event.code, event.params, event.message, store.getState()),
      });
    }
  });
  client.onConnectionChange((state) => dispatch({ kind: "connection", state }));

  return {
    store,
    // A failure's message is worded here, in the language and with the names of the moment it
    // arrives, so every caller shows `err.message` as it always did.
    request: (body) =>
      client.request(body).catch((err: unknown) => {
        if (!(err instanceof DaemonRequestError)) throw err;
        const message = daemonMessage(currentLanguage(), err.code, err.params, err.message, store.getState());
        throw new DaemonRequestError(message, err.code, err.params);
      }),
    toastError: (message, session, title, recovery) => emitToast(errorToast(message, session, title, recovery)),
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
