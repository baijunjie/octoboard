import { createContext, useContext } from "react";
import { createStore, type StoreApi } from "zustand";
import { useStore } from "zustand/react";

import { DaemonClient, DaemonRequestError, type ConnectionState } from "./daemon-client";
import { daemonMessage } from "./daemonMessage";
import { currentLanguage } from "./i18n/language";
import { daemonWsUrl, type DaemonOrigin } from "./daemon";
import {
  isLive,
  type Agent,
  type AgentAvailability,
  type Console,
  type Event,
  type GitStatus,
  type Host,
  type Page,
  type Project,
  type RequestBody,
  type Session,
  type Settings,
} from "./protocol";

/**
 * Something to tell the user in a toast: a daemon `error` or `session_notice`, or a message with
 * nowhere inline to show it (see `toastError` and `toastNotice` below). `kind` is the only thing
 * that tells an error from a notice apart. The store does not keep them: the toast stack
 * (`Toasts.tsx`) subscribes through `Daemon.onToast` and owns what is on screen.
 */
export interface ToastRequest {
  kind: "error" | "notice";
  message: string;
  /** The session the toast is about, so the stack can say where it is. A daemon `notice` always
   * has one; a notice the UI raises itself (`toastNotice`) has none. An `error` has one only where
   * the call site knows which session the daemon's message is about — the daemon says "this
   * session" without naming it, having no notion of what the client calls it, so the toast's title
   * is the only thing that tells the user which one. */
  session?: string;
}

/** A session waiting at its agent's folder-trust confirmation for the user's say-so. */
export interface TrustPrompt {
  session: string;
  /** The agent that is asking. */
  agent: Agent;
  project: string;
  path: string;
  /** The directory the "Trust parent folder" choice would trust, or null when none is offered. */
  trustDir: string | null;
}

/** An unbound project session's request for a console session, waiting for the user's answer. */
export interface ConsoleRequest {
  requestId: string;
  /** The requesting session. */
  session: string;
  /** The console the console session would be started in: the requesting session's. */
  console: string;
  project: string | null;
  requestedAt: number;
}

/** The components of an absolute path with `.` dropped and each `..` folded into the one before it,
 * or undefined for a path that is not absolute. */
function pathParts(path: string): string[] | undefined {
  if (!path.startsWith("/")) return undefined;
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts;
}

/** Whether the absolute `path` is one of `directories` or lies below one. The paths are compared
 * component by component, never as text — `/a/Project` does not cover `/a/Project2` — after `.` and
 * `..` are folded, and no symlink is followed. A path that is not absolute is under nothing. */
function isUnderAny(path: string, directories: string[]): boolean {
  const mine = pathParts(path);
  if (!mine) return false;
  return directories.some((directory) => {
    const theirs = pathParts(directory);
    return theirs !== undefined && theirs.length <= mine.length && theirs.every((part, i) => mine[i] === part);
  });
}

export interface State {
  connectionState: ConnectionState;
  /** `undefined` until the first `snapshot` arrives — the tree has nothing to render before that. */
  hosts?: Map<string, Host>;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  sessions: Map<string, Session>;
  /** Keyed by console session id, oldest first, as `page_list` delivers them. Absent for a console
   * session the report panel has not (re-)listed yet — distinct from an empty array, which means it has and
   * there genuinely are none. Pages are deliberately left out of `snapshot` (see `protocol.ts`),
   * so this map starts empty and is filled only by the panel's own `list_pages` requests. */
  pages: Map<string, Page[]>;
  /** Bumped on every `snapshot`. A snapshot arrives on the same socket for a lag recovery, not
   * just a fresh connection (see the `page_list` row under "Daemon to client" in
   * `apps/daemon/PROTOCOL.md`), so `connectionState` alone does not change — a consumer that needs to
   * re-list after either case depends on this counter instead. */
  snapshotEpoch: number;
  /** Oldest first; the first is the one the dialog shows. A prompt is dropped when it is answered or
   * declined, when its session stops running, and on a `snapshot`, which cannot say whether the
   * confirmation is still up — the daemon re-sends the prompts still waiting right after each
   * snapshot. */
  trustPrompts: TrustPrompt[];
  /** The directories whose projects Octoboard presses every agent's trust confirmation for, as the
   * last `snapshot` or `trusted_directories_updated` said. */
  trustedDirectories: string[];
  /** Oldest first; the first is the one the dialog shows. Held by the daemon, not here: a request
   * leaves only when the daemon says it stopped waiting (answered from any client, timed out or
   * withdrawn), and on a `snapshot`, after which the daemon sends the ones still waiting again. */
  consoleRequests: ConsoleRequest[];
  /** Keyed by project id. Absent for a project the daemon has not reported on yet, or that is not
   * a git repository at all. Dropped when its project is deleted — the daemon sends no deletion
   * event for a `GitStatus` (see `protocol.ts`), so the client has to drop it itself. */
  gitStatuses: Map<string, GitStatus>;
  /** The app-wide settings the daemon stores, as the last `snapshot` or `settings_updated` said. */
  settings: Settings;
  /** Every agent's availability, keyed by agent, as the last `snapshot` or
   * `agent_availability_updated` said. Always holds all three agents, each `not_determined`
   * until the daemon's one-time determination for this run lands. */
  agentAvailability: Map<Agent, AgentAvailability>;
  /** The daemon host's home directory as the last `snapshot` said, for showing paths under it as
   * `~/...` (see `abbreviateHome`). Null until the first snapshot, and when the daemon cannot
   * determine one. */
  homeDir: string | null;
}

/** The three entries every run begins with, before the daemon's one-time determination lands. */
const INITIAL_AGENT_AVAILABILITY: AgentAvailability[] = (["claude", "codex", "grok"] as Agent[]).map((agent) => ({
  agent,
  availability: "not_determined",
}));

type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "event"; event: Event }
  | { kind: "dismiss_trust_prompt"; session: string };

const initialState: State = {
  connectionState: "connecting",
  consoles: new Map(),
  projects: new Map(),
  sessions: new Map(),
  pages: new Map(),
  snapshotEpoch: 0,
  trustPrompts: [],
  trustedDirectories: [],
  consoleRequests: [],
  gitStatuses: new Map(),
  settings: { auto_sync_repositories: false, default_clone_dir: "", accounts: [] },
  agentAvailability: new Map(INITIAL_AGENT_AVAILABILITY.map((a) => [a.agent, a])),
  homeDir: null,
};

/** A store holding the empty state with `initial` laid over it. */
export function createStateStore(initial: Partial<State> = {}): StoreApi<State> {
  return createStore<State>(() => ({ ...initialState, ...initial }));
}

function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      return { ...state, connectionState: action.state };
    case "dismiss_trust_prompt":
      return { ...state, trustPrompts: state.trustPrompts.filter((p) => p.session !== action.session) };
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
            // way (see `Page` in protocol.ts). Cleared rather than kept, because a console session
            // that was deleted while this client was behind would otherwise go on holding its
            // pages forever — nothing else ever removes an entry for a session the snapshot no
            // longer lists.
            pages: new Map(),
            snapshotEpoch: state.snapshotEpoch + 1,
            trustPrompts: [],
            trustedDirectories: event.trusted_directories,
            consoleRequests: [],
            gitStatuses: new Map(event.git_statuses.map((s) => [s.project, s])),
            settings: event.settings,
            agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])),
            homeDir: event.home_dir,
          };
        case "trusted_directories_updated":
          // A prompt for a project under a directory that is now trusted has nothing left to ask:
          // the daemon presses that confirmation itself. The others stay queued.
          return {
            ...state,
            trustedDirectories: event.trusted_directories,
            trustPrompts: state.trustPrompts.filter((p) => !isUnderAny(p.path, event.trusted_directories)),
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
          // A page goes with the console session that pushed it, so with the console's.
          const pages = new Map(
            Array.from(state.pages).filter(([consoleSessionId]) => sessions.has(consoleSessionId)),
          );
          // Same cascade one level further down: a git status for a project the console took
          // with it has nothing left to be about.
          const gitStatuses = new Map(
            Array.from(state.gitStatuses).filter(([projectId]) => projects.has(projectId)),
          );
          return { ...state, consoles, projects, sessions, pages, gitStatuses };
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
          // A project that now has the permission has nothing left to ask: the daemon presses its
          // waiting confirmations itself, whichever agent shows them.
          const trustPrompts = event.project.trust_consent
            ? state.trustPrompts.filter((p) => p.project !== event.project.id)
            : state.trustPrompts;
          return { ...state, projects, trustPrompts };
        }
        case "project_deleted": {
          const projects = new Map(state.projects);
          projects.delete(event.project);
          // Same cascade as a deleted console, one level down.
          const sessions = new Map(
            Array.from(state.sessions).filter(([, s]) => s.project_id !== event.project),
          );
          // The daemon sends no event for this; see `gitStatuses` above.
          const gitStatuses = new Map(state.gitStatuses);
          gitStatuses.delete(event.project);
          return { ...state, projects, sessions, gitStatuses };
        }
        case "session_upserted": {
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          // A session that no longer runs is no longer at a confirmation anyone can answer.
          const trustPrompts = isLive(event.session.status)
            ? state.trustPrompts
            : state.trustPrompts.filter((p) => p.session !== event.session.id);
          return { ...state, sessions, trustPrompts };
        }
        case "session_deleted": {
          const sessions = new Map(state.sessions);
          sessions.delete(event.session);
          // The daemon takes a console session's pages with it, and says so only through this event.
          const pages = new Map(state.pages);
          pages.delete(event.session);
          return { ...state, sessions, pages };
        }
        case "trust_prompt": {
          if (state.trustPrompts.some((p) => p.session === event.session)) return state;
          // Nothing left to ask about a project that has the permission, of its own or through a
          // trusted directory: the daemon presses its confirmations itself.
          if (state.projects.get(event.project)?.trust_consent) return state;
          if (isUnderAny(event.path, state.trustedDirectories)) return state;
          const prompt: TrustPrompt = {
            session: event.session,
            agent: event.agent,
            project: event.project,
            path: event.path,
            trustDir: event.trust_dir,
          };
          return { ...state, trustPrompts: [...state.trustPrompts, prompt] };
        }
        case "console_session_request": {
          if (state.consoleRequests.some((r) => r.requestId === event.request_id)) return state;
          const request: ConsoleRequest = {
            requestId: event.request_id,
            session: event.session,
            console: event.console,
            project: event.project,
            requestedAt: event.requested_at,
          };
          // Kept in the order they were made, whatever order the broadcast and the repeat after a
          // snapshot arrive in.
          const consoleRequests = [...state.consoleRequests, request].sort((a, b) => a.requestedAt - b.requestedAt);
          return { ...state, consoleRequests };
        }
        case "console_session_request_closed":
          return {
            ...state,
            consoleRequests: state.consoleRequests.filter((r) => r.requestId !== event.request_id),
          };
        case "page_list": {
          // The reply to a `list_pages` request, but requests and broadcasts are not ordered
          // against each other (see "Client to daemon" and "Daemon to client" in
          // `apps/daemon/PROTOCOL.md`), so a `page_created` for a page `show_page` inserted after this
          // reply was computed can have already landed here. Replacing wholesale would drop that
          // page; instead keep the reply's order first, then append whatever locally held page
          // the reply is missing.
          // Pages are append-only and a `page_created` is always the newest, so appending after
          // keeps the merged order correct.
          const existing = state.pages.get(event.console_session_id) ?? [];
          const seen = new Set(event.pages.map((p) => p.id));
          const pages = new Map(state.pages);
          pages.set(event.console_session_id, [...event.pages, ...existing.filter((p) => !seen.has(p.id))]);
          return { ...state, pages };
        }
        case "page_created": {
          // Appended even with no baseline yet: that window is exactly the first `list_pages`
          // being in flight, and dropping the event here would mean waiting for a second push to
          // ever see it. The transient "1 / 1" this produces is corrected once that reply's merge
          // (above) lands.
          const existing = state.pages.get(event.page.console_session_id) ?? [];
          const pages = new Map(state.pages);
          pages.set(event.page.console_session_id, [...existing, event.page]);
          return { ...state, pages };
        }
        case "project_git_status": {
          const gitStatuses = new Map(state.gitStatuses);
          gitStatuses.set(event.status.project, event.status);
          return { ...state, gitStatuses };
        }
        case "settings_updated":
          return { ...state, settings: event.settings };
        case "agent_availability_updated":
          return { ...state, agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])) };
        default:
          return state;
      }
    }
    default:
      return state;
  }
}

/** The daemon connection and the state it feeds: one per window, created at startup (see
 * `main.tsx`) and alive until the window closes. The store is a vanilla one, so code outside React
 * can read and subscribe to it; components use `useDaemonStore`. */
export interface Daemon {
  store: StoreApi<State>;
  /** Sends a request and resolves/rejects with its correlated reply. Does *not* toast on its own
   * rejection — a request made from a dialog shows its own inline error (the better place for it),
   * so toasting here too would show the same failure twice. A caller with nowhere inline to put an
   * error (no open dialog) calls `toastError` itself. */
  request: (body: RequestBody) => Promise<Event>;
  /** `session` is passed by a caller that knows which session the message is about, so the toast
   * can name it; see `ToastRequest.session`. */
  toastError: (message: string, session?: string) => void;
  /** Confirms something the user just did, which has no inline place to say so (a path copied);
   * about no session, so the toast is the message alone. */
  toastNotice: (message: string) => void;
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

export function createDaemon(origin: DaemonOrigin): Daemon {
  const store = createStateStore();
  const dispatch = (action: Action) => store.setState((state) => reducer(state, action));

  const client = new DaemonClient(daemonWsUrl(origin, "/ws/control"));
  const toastListeners = new Set<(toast: ToastRequest) => void>();
  const emitToast = (toast: ToastRequest) => toastListeners.forEach((listener) => listener(toast));

  client.onEvent((event) => {
    dispatch({ kind: "event", event });
    if (event.type === "session_notice") {
      emitToast({
        kind: "notice",
        message: daemonMessage(currentLanguage(), event.code, event.params, event.message, store.getState()),
        session: event.session,
      });
    } else if (event.type === "error" && !event.id) {
      // A request's own `error` reply carries its request id and is delivered to its caller as a
      // rejected promise instead — only a genuine daemon broadcast (no id) is a toast.
      emitToast({
        kind: "error",
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
    toastError: (message, session) => emitToast({ kind: "error", message, session }),
    toastNotice: (message) => emitToast({ kind: "notice", message }),
    onToast: (listener) => {
      toastListeners.add(listener);
      return () => void toastListeners.delete(listener);
    },
    dismissTrustPrompt: (session) => dispatch({ kind: "dismiss_trust_prompt", session }),
    reconnect: () => client.reconnect(),
    terminalUrl: (session) => daemonWsUrl(origin, `/ws/term/${session}`),
  };
}

const DaemonContext = createContext<Daemon | undefined>(undefined);

export const DaemonProvider = DaemonContext.Provider;

/** The requests and actions; they never change, so using this never re-renders. */
export function useDaemon(): Daemon {
  const daemon = useContext(DaemonContext);
  if (!daemon) throw new Error("useDaemon() called outside a DaemonProvider");
  return daemon;
}

/** Reads a slice of the state. A selector that builds a new object or array each call has to be
 * wrapped in zustand's `useShallow`, or the component re-renders on every change. */
export function useDaemonStore<T>(selector: (state: State) => T): T {
  return useStore(useDaemon().store, selector);
}
