import React, { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef } from "react";

import { DaemonClient, type ConnectionState } from "./daemon-client";
import type { Console, Event, Host, Project, RequestBody, Session } from "./protocol";

export interface DaemonError {
  id: string;
  message: string;
}

interface State {
  connectionState: ConnectionState;
  /** `undefined` until the first `snapshot` arrives — the tree has nothing to render before that. */
  hosts?: Map<string, Host>;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  sessions: Map<string, Session>;
  errors: DaemonError[];
}

type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "event"; event: Event }
  /** A message with nowhere inline to show it (no open dialog). Distinct from `"event"` so it
   * never counts as proof the control socket is up — see the "event" case below. */
  | { kind: "toast"; message: string }
  | { kind: "dismiss_error"; id: string };

const initialState: State = {
  connectionState: "connecting",
  consoles: new Map(),
  projects: new Map(),
  sessions: new Map(),
  errors: [],
};

function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      return { ...state, connectionState: action.state };
    case "toast":
      return { ...state, errors: [...state.errors, { id: crypto.randomUUID(), message: action.message }] };
    case "dismiss_error":
      return { ...state, errors: state.errors.filter((e) => e.id !== action.id) };
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
          return { ...state, consoles, projects, sessions };
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
        case "error": {
          // A request's own `error` reply carries its request id and is delivered to its caller as
          // a rejected promise instead (handled where the caller awaits it) — only a genuine
          // daemon broadcast (no id) lands here.
          if (event.id) return state;
          return { ...state, errors: [...state.errors, { id: crypto.randomUUID(), message: event.message }] };
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
  dismissError: (id: string) => void;
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

  const dismissError = useCallback((id: string) => dispatch({ kind: "dismiss_error", id }), []);

  const reconnect = useCallback(() => clientRef.current?.reconnect(), []);

  const value = useMemo<DaemonContextValue>(
    () => ({ ...state, request, toastError, dismissError, reconnect }),
    [state, request, toastError, dismissError, reconnect],
  );

  return <DaemonContext.Provider value={value}>{children}</DaemonContext.Provider>;
}

export function useDaemon(): DaemonContextValue {
  const ctx = useContext(DaemonContext);
  if (!ctx) throw new Error("useDaemon() called outside a DaemonProvider");
  return ctx;
}
