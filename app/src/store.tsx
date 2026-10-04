import React, { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef } from "react";

import { DaemonClient, type ConnectionState } from "./daemon-client";
import type { Console, Event, Host, Page, Project, RequestBody, Session } from "./protocol";

/**
 * One entry in the dismissible toast stack: a daemon `error` or `session_notice`, or a message with
 * nowhere inline to show it (see `toastError` below). `kind` is the only thing that tells an error
 * from a notice apart — they are otherwise the same shape and share one stack, which is what keeps
 * them in arrival order instead of a notice permanently parking itself ahead of (or behind) every
 * error around it.
 */
export interface Toast {
  id: string;
  kind: "error" | "notice";
  message: string;
  /** The session a `notice` is about, so it can be rendered alongside the message — absent for an
   * `error`, which is never about one particular session. */
  session?: string;
}

interface State {
  connectionState: ConnectionState;
  /** `undefined` until the first `snapshot` arrives — the tree has nothing to render before that. */
  hosts?: Map<string, Host>;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  sessions: Map<string, Session>;
  /** Keyed by console id, oldest first, as `page_list` delivers them. Absent for a console the
   * report panel has not (re-)listed yet — distinct from an empty array, which means it has and
   * there genuinely are none. Pages are deliberately left out of `snapshot` (see `protocol.ts`),
   * so this map starts empty and is filled only by the panel's own `list_pages` requests. */
  pages: Map<string, Page[]>;
  /** Bumped on every `snapshot`. A snapshot arrives on the same socket for a lag recovery, not
   * just a fresh connection (see the `page_list` row under "Daemon to client" in
   * `daemon/PROTOCOL.md`), so `connectionState` alone does not change — a consumer that needs to
   * re-list after either case depends on this counter instead. */
  snapshotEpoch: number;
  toasts: Toast[];
}

type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "event"; event: Event }
  /** A message with nowhere inline to show it (no open dialog). Distinct from `"event"` so it
   * never counts as proof the control socket is up — see the "event" case below. */
  | { kind: "toast"; message: string }
  | { kind: "dismiss_toast"; id: string };

const initialState: State = {
  connectionState: "connecting",
  consoles: new Map(),
  projects: new Map(),
  sessions: new Map(),
  pages: new Map(),
  snapshotEpoch: 0,
  toasts: [],
};

function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      return { ...state, connectionState: action.state };
    case "toast":
      return { ...state, toasts: [...state.toasts, { id: crypto.randomUUID(), kind: "error", message: action.message }] };
    case "dismiss_toast":
      return { ...state, toasts: state.toasts.filter((t) => t.id !== action.id) };
    case "event": {
      const event = action.event;
      switch (event.type) {
        case "snapshot":
          return {
            ...state,
            hosts: new Map(event.hosts.map((h) => [h.id, h])),
            consoles: new Map(event.consoles.map((c) => [c.id, c])),
            projects: new Map(event.projects.map((p) => [p.id, p])),
            sessions: new Map(event.sessions.map((s) => [s.id, s])),
            // A snapshot means either first connect or a lag recovery, and carries no pages either
            // way (see `Page` in protocol.ts). Cleared rather than kept, because a console that
            // was deleted while this client was behind would otherwise go on holding that
            // console's pages forever — nothing else ever removes an entry for a console the
            // snapshot no longer lists.
            pages: new Map(),
            snapshotEpoch: state.snapshotEpoch + 1,
          };
        case "console_upserted": {
          const consoles = new Map(state.consoles);
          consoles.set(event.console.id, event.console);
          return { ...state, consoles };
        }
        case "console_deleted": {
          const consoles = new Map(state.consoles);
          consoles.delete(event.console);
          // The daemon's database takes the console's projects and sessions with it, and says so
          // only through this one event — a client that kept them would go on offering a terminal
          // and a Resume for a session that no longer exists anywhere.
          const projects = new Map(
            Array.from(state.projects).filter(([, p]) => p.console_id !== event.console),
          );
          const sessions = new Map(
            Array.from(state.sessions).filter(([, s]) => s.console_id !== event.console),
          );
          const pages = new Map(state.pages);
          pages.delete(event.console);
          return { ...state, consoles, projects, sessions, pages };
        }
        case "session_opened": {
          // The reply to our own `open_session`. The broadcast carries the same record, but
          // whichever arrives first should already put the session in the tree.
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          return { ...state, sessions };
        }
        case "project_upserted": {
          const projects = new Map(state.projects);
          projects.set(event.project.id, event.project);
          return { ...state, projects };
        }
        case "project_deleted": {
          const projects = new Map(state.projects);
          projects.delete(event.project);
          // Same cascade as a deleted console, one level down.
          const sessions = new Map(
            Array.from(state.sessions).filter(([, s]) => s.project_id !== event.project),
          );
          return { ...state, projects, sessions };
        }
        case "session_upserted": {
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          return { ...state, sessions };
        }
        case "page_list": {
          // The reply to a `list_pages` request, but requests and broadcasts are not ordered
          // against each other (see "Client to daemon" and "Daemon to client" in
          // `daemon/PROTOCOL.md`), so a `page_created` for a page `show_page` inserted after this
          // reply was computed can have already landed here. Replacing wholesale would drop that
          // page; instead keep the reply's order first, then append whatever locally held page
          // the reply is missing.
          // Pages are append-only and a `page_created` is always the newest, so appending after
          // keeps the merged order correct.
          const existing = state.pages.get(event.console_id) ?? [];
          const seen = new Set(event.pages.map((p) => p.id));
          const pages = new Map(state.pages);
          pages.set(event.console_id, [...event.pages, ...existing.filter((p) => !seen.has(p.id))]);
          return { ...state, pages };
        }
        case "page_created": {
          // Appended even with no baseline yet: that window is exactly the first `list_pages`
          // being in flight, and dropping the event here would mean waiting for a second push to
          // ever see it. The transient "1 / 1" this produces is corrected once that reply's merge
          // (above) lands.
          const existing = state.pages.get(event.page.console_id) ?? [];
          const pages = new Map(state.pages);
          pages.set(event.page.console_id, [...existing, event.page]);
          return { ...state, pages };
        }
        case "session_notice": {
          const notice: Toast = {
            id: crypto.randomUUID(),
            kind: "notice",
            session: event.session,
            message: event.message,
          };
          return { ...state, toasts: [...state.toasts, notice] };
        }
        case "error": {
          // A request's own `error` reply carries its request id and is delivered to its caller as
          // a rejected promise instead (handled where the caller awaits it) — only a genuine
          // daemon broadcast (no id) lands here.
          if (event.id) return state;
          const toast: Toast = { id: crypto.randomUUID(), kind: "error", message: event.message };
          return { ...state, toasts: [...state.toasts, toast] };
        }
        default:
          return state;
      }
    }
    default:
      return state;
  }
}

interface DaemonContextValue extends State {
  /** Sends a request and resolves/rejects with its correlated reply. Does *not* toast on its own
   * rejection — a request made from a dialog shows its own inline error (the better place for it),
   * so toasting here too would show the same failure twice. A caller with nowhere inline to put an
   * error (no open dialog) calls `toastError` itself. */
  request: (body: RequestBody) => Promise<Event>;
  toastError: (message: string) => void;
  dismissToast: (id: string) => void;
  /** Retries the control connection right away after the automatic reconnect budget was spent. */
  reconnect: () => void;
}

const DaemonContext = createContext<DaemonContextValue | undefined>(undefined);

export function DaemonProvider({ url, children }: { url: string; children: React.ReactNode }): React.ReactElement {
  const [state, dispatch] = useReducer(reducer, initialState);
  const clientRef = useRef<DaemonClient>();

  // `url` is derived once from the resolved daemon port and never changes for the life of the
  // window, so this effect is deliberately mount/unmount-only (empty dependency array).
  useEffect(() => {
    const client = new DaemonClient(url);
    clientRef.current = client;
    const offEvent = client.onEvent((event) => dispatch({ kind: "event", event }));
    const offConn = client.onConnectionChange((state) => dispatch({ kind: "connection", state }));
    return () => {
      offEvent();
      offConn();
      client.close();
    };
  }, []);

  const request = useCallback((body: RequestBody) => {
    if (!clientRef.current) return Promise.reject(new Error("daemon client not ready"));
    return clientRef.current.request(body);
  }, []);

  const toastError = useCallback((message: string) => dispatch({ kind: "toast", message }), []);

  const dismissToast = useCallback((id: string) => dispatch({ kind: "dismiss_toast", id }), []);

  const reconnect = useCallback(() => clientRef.current?.reconnect(), []);

  const value = useMemo<DaemonContextValue>(
    () => ({ ...state, request, toastError, dismissToast, reconnect }),
    [state, request, toastError, dismissToast, reconnect],
  );

  return <DaemonContext.Provider value={value}>{children}</DaemonContext.Provider>;
}

export function useDaemon(): DaemonContextValue {
  const ctx = useContext(DaemonContext);
  if (!ctx) throw new Error("useDaemon() called outside a DaemonProvider");
  return ctx;
}
