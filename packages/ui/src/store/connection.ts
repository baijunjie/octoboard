import type { ReactNode } from "react";
import type { StoreApi } from "zustand";

import { BRIEF_RECONNECT, DaemonClient, ENDLESS_RECONNECT } from "../daemon-client";
import { looseRequestSender, type RequestSender } from "../requests";
import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import { daemonRefusedKey, daemonWsUrl, isLocalDaemon, type DaemonAddress } from "../daemon";
import { currentLanguage, t } from "../i18n/language";
import type { Event, ClientRequestBody, ScheduledTaskRun } from "../protocol";
import { createContentSearchListeners, isContentSearchEvent, type ContentSearchEvent } from "./contentSearchListeners";
import { createImportProgressListeners, type ImportProgressEvent } from "./importProgressListeners";
import { createMemoryListeners, isMemoryChangeEvent, type MemoryChangeEvent } from "./memoryListeners";
import { createOwnTrustNotices } from "./ownTrustNotices";
import { reducer, type Action } from "./reducer";
import { createStateStore, type State } from "./state";
import { broadcastNotification, type NotifiedBroadcast, type WindowNotification, type WithdrawnBroadcast } from "./notifications";

/** The daemon connection and the state it feeds: one per window, created at startup (see
 * `main.tsx`) and alive until the window closes. The store is a vanilla one, so code outside React
 * can read and subscribe to it; components use `useDaemonStore`. */
export interface Daemon {
  /** True only when the connection establishes that the daemon runs on this machine. Unknown counts as remote, and so
   * does a window the desktop shell opened on a daemon elsewhere, which reaches it at a loopback address too. */
  readonly isLocal?: boolean;
  store: StoreApi<State>;
  /** Sends a request and resolves with its correlated reply, of the type the request is answered with (`ReplyOf`), or
   * rejects with a `CodedError` worded in the current language. Callers present failures, allowing field validation to
   * remain local and execution errors to require acknowledgement. */
  request: RequestSender;
  /** Confirms something the user just did, which has no inline place to say so (a path copied);
   * about no session, so the notice is the message alone. */
  showNotice: (message: string, content?: ReactNode) => void;
  /** Calls `listener` for every notification to show from now on: the daemon's errors and notices that
   * belong to no request, what `showNotice` is given, and a daemon notice's withdrawal, in the order they arrived.
   * Returns the unsubscribe. */
  onNotification: (listener: (notification: WindowNotification) => void) => () => void;
  /** Calls `listener` for each progress event of the configuration import `importId` names (the `import_id` its
   * request carried) until the returned unsubscribe is called. The daemon broadcasts them to every window, and one may
   * arrive after the import's reply, so the caller unsubscribes once that comes (the client does not do so itself) and
   * hears nothing of it. */
  onImportProgress: (importId: string, listener: (event: ImportProgressEvent) => void) => () => void;
  /** Calls `listener` for each event of the content search `search` names (the `search` its request carried): its
   * batches of matches and its end, until the returned unsubscribe is called. Events go to the asking connection only,
   * and those of a search no listener holds are dropped. */
  onContentSearch: (search: string, listener: (event: ContentSearchEvent) => void) => () => void;
  /** Calls `listener` for every change to any memory (a console's, or a project place's) and to the set of places left
   * with memory no project is at, from now on, in the order the daemon wrote them, until the returned unsubscribe is called. Memory is not in the store: what shows it lists it with `list_memories`, again
   * after every `snapshot`, and keeps it by these. */
  onMemoryChange: (listener: (event: MemoryChangeEvent) => void) => () => void;
  /** Calls `listener` with each run of a scheduled task the daemon records from now on (`scheduled_task_run_added`,
   * which every window is sent), after the store has taken it, until the returned unsubscribe is called. */
  onScheduledTaskRun: (listener: (run: ScheduledTaskRun) => void) => () => void;
  /** Drops a trust prompt from the queue, whether it was answered or declined — declining leaves the
   * confirmation for the user in the terminal, and the daemon asks again for it only after a
   * `snapshot`. */
  dismissTrustPrompt: (session: string) => void;
  /** Whether a lost control connection is retried until it connects, with Retry offered all the while: a window the
   * desktop shell opened on a remote daemon. Otherwise a couple of attempts are made, and Retry offered once they are
   * spent. */
  readonly retriesUntilConnected?: boolean;
  /** Retries the control connection right away: after the automatic attempts were spent, or, where they do not end,
   * without waiting for the next one. */
  reconnect: () => void;
  /** The URL of a session's terminal stream (`GET /ws/term/:session`), on the same daemon the
   * control connection goes to. */
  terminalUrl: (session: string) => string;
  /** Overrides the delay before a terminal's automatic reconnect attempt number `attempt` (from 0);
   * only the gallery sets it, so that a spent budget can be shown without waiting for it. */
  terminalReconnectDelay?: (attempt: number) => number;
}

/** `remote`: the window is one the desktop shell opened on a daemon elsewhere (the platform layer's `windowKind`). For
 * such a window, from the platform's `remoteConnection`: `beforeRetry`, what Retry waits for before it attempts, and
 * `onReachable`, which calls its listener when the daemon can be reached again and returns the unsubscribe. */
export function createDaemon(
  address: DaemonAddress,
  {
    remote = false,
    beforeRetry,
    onReachable,
  }: { remote?: boolean; beforeRetry?: () => Promise<void>; onReachable?: (listener: () => void) => () => void } = {},
): Daemon {
  const store = createStateStore();
  const dispatch = (action: Action) => store.setState((state) => reducer(state, action));
  // The events that arrived together are one update: a burst renders once, not once per event.
  const dispatchEvents = (events: Event[]) =>
    store.setState((state) => events.reduce((current, event) => reducer(current, { kind: "event", event }), state));

  const client = new DaemonClient(
    daemonWsUrl(address, "/ws/control"),
    address.servedByDaemon ? () => daemonRefusedKey(address) : undefined,
    remote ? { ...ENDLESS_RECONNECT, beforeRetry } : BRIEF_RECONNECT,
  );
  const importProgress = createImportProgressListeners();
  const contentSearches = createContentSearchListeners();
  const memoryListeners = createMemoryListeners();
  const notificationListeners = new Set<(notification: WindowNotification) => void>();
  const runListeners = new Set<(run: ScheduledTaskRun) => void>();
  const pendingNotifications: WindowNotification[] = [];
  const notify = (notification: WindowNotification) => {
    if (!notificationListeners.size) pendingNotifications.push(notification);
    else notificationListeners.forEach((listener) => listener(notification));
  };

  const emitBroadcast = (event: NotifiedBroadcast | WithdrawnBroadcast) => notify(broadcastNotification(event, store.getState(), currentLanguage()));
  const ownTrustNotices = createOwnTrustNotices(emitBroadcast);

  client.onEvents(dispatchEvents);
  client.onEvent((event) => {
    if (event.type === "snapshot") {
      ownTrustNotices.reset();
      return;
    }
    if (event.type === "configuration_import_progress") {
      importProgress.deliver(event);
      return;
    }
    if (isContentSearchEvent(event)) {
      contentSearches.deliver(event);
      return;
    }
    if (isMemoryChangeEvent(event)) {
      memoryListeners.deliver(event);
      return;
    }
    if (event.type === "scheduled_task_run_added") {
      runListeners.forEach((listener) => listener(event.run));
      return;
    }
    if (event.type === "session_notice" && ownTrustNotices.claim(event)) return;
    // A request's own `error` reply carries its request id and is delivered to its caller as a
    // rejected promise instead — only a genuine daemon broadcast (no id) is a notification.
    if (event.type === "session_notice" || event.type === "session_notice_withdrawn" || (event.type === "error" && !event.id)) {
      emitBroadcast(event);
    }
  });
  client.onConnectionChange((state) => dispatch({ kind: "connection", state }));
  client.onRetryPendingChange((pending) => dispatch({ kind: "retry_pending", pending }));
  // For as long as the window lives, as the client does.
  onReachable?.(() => client.reachable());

  // A failure's message is worded here, in the language and with the names of the moment it
  // arrives, so every caller shows `err.message` as it always did.
  const request: RequestSender = (body) => {
    const plain: ClientRequestBody = body;
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
    isLocal: !remote && isLocalDaemon(address),
    retriesUntilConnected: remote,
    store,
    request,
    showNotice: (message, content) => notify({ kind: "notice", message, content }),
    onNotification: (listener) => {
      notificationListeners.add(listener);
      pendingNotifications.splice(0).forEach(listener);
      return () => void notificationListeners.delete(listener);
    },
    onImportProgress: importProgress.subscribe,
    onContentSearch: contentSearches.subscribe,
    onMemoryChange: memoryListeners.subscribe,
    onScheduledTaskRun: (listener) => {
      runListeners.add(listener);
      return () => void runListeners.delete(listener);
    },
    dismissTrustPrompt: (session) => dispatch({ kind: "dismiss_trust_prompt", session }),
    reconnect: () => client.reconnect(),
    terminalUrl: (session) => daemonWsUrl(address, `/ws/term/${session}`),
  };
}

/**
 * A daemon that is not there, for the application's own window while it has none to connect to (it failed to start):
 * the window still has what does not need one (the Remote devices page), and it is told, by the screen it is given,
 * why. Its store never leaves the first state, nothing is connected to, and every request is refused.
 */
export function createInertDaemon(): Daemon {
  const store = createStateStore();
  return {
    isLocal: true,
    store,
    request: looseRequestSender(async () => {
      throw new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {});
    }),
    showNotice: () => {},
    onNotification: () => () => {},
    onImportProgress: () => () => {},
    onContentSearch: () => () => {},
    onMemoryChange: () => () => {},
    onScheduledTaskRun: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}
