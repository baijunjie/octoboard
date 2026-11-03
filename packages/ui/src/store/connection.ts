import type { ReactNode } from "react";
import type { StoreApi } from "zustand";

import { DaemonClient } from "../daemon-client";
import type { RequestSender } from "../requests";
import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import { daemonWsUrl, type DaemonAddress } from "../daemon";
import { currentLanguage } from "../i18n/language";
import type { Event, RequestBody } from "../protocol";
import { createOwnTrustNotices } from "./ownTrustNotices";
import { reducer, type Action } from "./reducer";
import { createStateStore, type State } from "./state";
import { broadcastNotification, type NotifiedBroadcast, type NotificationRequest } from "./notifications";

/** The daemon connection and the state it feeds: one per window, created at startup (see
 * `main.tsx`) and alive until the window closes. The store is a vanilla one, so code outside React
 * can read and subscribe to it; components use `useDaemonStore`. */
export interface Daemon {
  store: StoreApi<State>;
  /** Sends a request and resolves with its correlated reply, of the type the request is answered with (`ReplyOf`), or
   * rejects with a `CodedError` worded in the current language. Callers present failures, allowing field validation to
   * remain local and execution errors to require acknowledgement. */
  request: RequestSender;
  /** Confirms something the user just did, which has no inline place to say so (a path copied);
   * about no session, so the toast is the message alone. */
  showNotice: (message: string, content?: ReactNode) => void;
  /** Calls `listener` for every notification to show from now on: the daemon's errors and notices that
   * belong to no request, and what `showNotice` is given. Returns the unsubscribe. */
  onNotification: (listener: (notification: NotificationRequest) => void) => () => void;
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
  const notificationListeners = new Set<(notification: NotificationRequest) => void>();
  const pendingNotifications: NotificationRequest[] = [];
  const notify = (notification: NotificationRequest) => {
    if (!notificationListeners.size) pendingNotifications.push(notification);
    else notificationListeners.forEach((listener) => listener(notification));
  };

  const emitBroadcast = (event: NotifiedBroadcast) => notify(broadcastNotification(event, store.getState(), currentLanguage()));
  const ownTrustNotices = createOwnTrustNotices(emitBroadcast);

  client.onEvents(dispatchEvents);
  client.onEvent((event) => {
    if (event.type === "snapshot") {
      ownTrustNotices.reset();
      return;
    }
    if (event.type === "session_notice" && ownTrustNotices.claim(event)) return;
    // A request's own `error` reply carries its request id and is delivered to its caller as a
    // rejected promise instead — only a genuine daemon broadcast (no id) is a notification.
    if (event.type === "session_notice" || (event.type === "error" && !event.id)) {
      emitBroadcast(event);
    }
  });
  client.onConnectionChange((state) => dispatch({ kind: "connection", state }));

  // A failure's message is worded here, in the language and with the names of the moment it
  // arrives, so every caller shows `err.message` as it always did.
  const request: RequestSender = (body) => {
    const plain: RequestBody = body;
    const settleTrust = plain.type === "confirm_trust" ? ownTrustNotices.begin(plain.session) : undefined;
    const reply = client.request(body);
    return reply.then(
      (reply) => {
        settleTrust?.();
        return reply;
      },
      (err: unknown) => {
        settleTrust?.(err instanceof CodedError ? err.code : undefined);
        if (!(err instanceof CodedError)) throw err;
        const message = daemonMessage(currentLanguage(), err.code, err.params, err.message, store.getState());
        throw new CodedError(message, err.code, err.params);
      },
    );
  };

  return {
    store,
    request,
    showNotice: (message, content) => notify({ kind: "notice", message, content }),
    onNotification: (listener) => {
      notificationListeners.add(listener);
      pendingNotifications.splice(0).forEach(listener);
      return () => void notificationListeners.delete(listener);
    },
    dismissTrustPrompt: (session) => dispatch({ kind: "dismiss_trust_prompt", session }),
    reconnect: () => client.reconnect(),
    terminalUrl: (session) => daemonWsUrl(address, `/ws/term/${session}`),
  };
}
