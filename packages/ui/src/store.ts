import { createContext, useContext, useState, type ReactNode } from "react";
import { createStore, type StoreApi } from "zustand";
import { useStore } from "zustand/react";

import { DaemonClient, DaemonRequestError, type ConnectionState } from "./daemon-client";
import { daemonMessage } from "./daemonMessage";
import { errorPresentation, noticeErrorPresentation } from "./errorPresentation";
import { errorText } from "./errorText";
import { currentLanguage } from "./i18n/language";
import { daemonWsUrl, type DaemonOrigin } from "./daemon";
import { accountQuotaOf } from "./quota/accountQuotaOf";
import { sameValue } from "./sameValue";
import {
  isLive,
  SESSION_REQUEST_ACTIONS,
  type AccountLogin,
  type AccountSharing,
  type AccountQuota,
  type Agent,
  type AgentAvailability,
  type Console,
  type Event,
  type GitStatus,
  type Host,
  type Page,
  type Project,
  type RequestBody,
  type Role,
  type Session,
  type Settings,
  type ShellTerminal,
} from "./protocol";

/**
 * A window notification. Errors require acknowledgement in the shared error dialog; notices
 * remain timed toasts. The onToast name is retained for existing producers and consumers.
 */
export interface ToastRequest {
  kind: "error" | "notice";
  message: string;
  /** Optional markup; `message` stays plain for deduplication and text-only consumers. */
  content?: ReactNode;
  daemonTitle?: DaemonRequestError;
  /** The daemon's own code and params, kept for a structured rendering of the message (a literal
   * set as code), for a notice as well as an error. Absent for a message the UI worded itself. */
  daemonError?: DaemonRequestError;
  /** The original local exception, including its cause and platform error code. */
  error?: unknown;
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
  /** The directory the trust dialog's folder-wide checkbox would trust, or null when none is
   * offered. */
  trustDir: string | null;
}

/** What every session request says, whatever an approval does. */
interface SessionRequestBase {
  requestId: string;
  /** The requesting session. */
  session: string;
  /** Whether the session asked for would be a console session or a project session. */
  role: Role;
  /** The console the new session would be in, or the console of the session to bring back. */
  console: string;
  /** The project the new session would be in, or the project of the session to bring back; null
   * for a console session. */
  project: string | null;
  /** The agent the new session would run, or the agent of the session to bring back. */
  agent: Agent;
  /** The branches an approval would hand to the new session, as wire paths; empty for none. */
  branches: string[];
  requestedAt: number;
}

/** A session's request for a session with no owner, or to resume a conversation, waiting for the
 * user's answer. What it carries depends on what an approval does, which `action` tells. */
export type SessionRequest = SessionRequestBase & (
  | { action: "start" }
  | {
    action: "reopen" | "resume";
    /** The session an approval brings back. */
    target: string;
    /** The session the target reports to; absent when it has none. */
    owner?: string;
    /** The archived owners that come back with the target, topmost first. */
    reopenedOwners: string[];
  }
  | {
    action: "resume_record";
    /** The account that holds the record; null for the agent's default account. */
    account: string | null;
    /** The conversation's id. */
    conversation: string;
  }
);

/** The request a `session_request` event describes, or undefined for one that does not carry what
 * its action needs (a reopen or resume without a target, a record without its conversation) or
 * has an action this client does not know: the dialog is the one place an approval is enforced, so
 * a request it cannot describe is not offered at all, and the daemon's own timeout ends the call. */
function sessionRequestOf(event: Extract<Event, { type: "session_request" }>): SessionRequest | undefined {
  const base: SessionRequestBase = {
    requestId: event.request_id,
    session: event.session,
    role: event.role,
    console: event.console,
    project: event.project,
    agent: event.agent,
    branches: event.branches,
    requestedAt: event.requested_at,
  };
  // The wire carries what a newer daemon may add, which the type does not know.
  if (!(SESSION_REQUEST_ACTIONS as readonly string[]).includes(event.action)) return undefined;
  const { action } = event;
  switch (action) {
    case "reopen":
    case "resume":
      if (event.target === null) return undefined;
      return { ...base, action, target: event.target, owner: event.owner ?? undefined, reopenedOwners: event.reopened_owners };
    case "resume_record":
      if (event.conversation === null) return undefined;
      return { ...base, action, account: event.account, conversation: event.conversation };
    default:
      return { ...base, action };
  }
}

/** The requests with `request` added, kept in the order they were made whatever order the
 * broadcast and the repeat after a snapshot arrive in. */
function withRequest(requests: SessionRequest[], request: SessionRequest): SessionRequest[] {
  return [...requests, request].sort((a, b) => a.requestedAt - b.requestedAt);
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
  /** Oldest first by `requestedAt`, the order the dialog takes them in after the trust prompts.
   * Held by the daemon, not here: a request leaves only when the daemon says it stopped waiting
   * (answered from any client, timed out or withdrawn), and on a `snapshot`, after which the daemon
   * sends the ones still waiting again. */
  sessionRequests: SessionRequest[];
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
  /** The plan quota of every account of every available agent, as the last `snapshot` or
   * `account_quotas_updated` said. Runtime state of the daemon, so it starts empty and is never
   * kept anywhere else; an account with no entry is one the daemon has not reported on (see
   * `accountQuotaOf`). */
  accountQuotas: AccountQuota[];
  /** The login of each account that has one running or just ended, as the last `snapshot` (running ones only) or
   * `account_login_updated` said; an ended login stays until the account's next login, a snapshot, or the user
   * dismissing it (see `dismissAccountLogin`). */
  accountLogins: AccountLogin[];
  /** The logins this window started whose authorization page it has not opened yet, as `accountLoginKey`s: the
   * page is opened by the window that asked for the login, not by every window watching it. An entry goes when the
   * page has been opened or the login ends. Client state, which the daemon knows nothing of. */
  loginsStartedHere: string[];
  /** The sharing state of each account that has something to show — sessions in use, a change running or a failure —
   * keyed by account id, as the last `snapshot` or `account_sharing` said. An account with no entry is free. */
  accountSharing: Map<string, AccountSharing>;
  /** The daemon host's home directory as the last `snapshot` said, for showing paths under it as
   * `~/...` (see `abbreviateHome`). Null until the first snapshot, and when the daemon cannot
   * determine one. */
  homeDir: string | null;
  /** Every shell terminal the daemon holds, running or exited, keyed by id, in the order the
   * daemon lists them (oldest first) and then as created. Live state of the daemon: a `snapshot`
   * names them all again, and one goes only on `shell_terminal_deleted` or with its project. */
  shellTerminals: Map<string, ShellTerminal>;
}

/** The three entries every run begins with, before the daemon's one-time determination lands. */
const INITIAL_AGENT_AVAILABILITY: AgentAvailability[] = (["claude", "codex", "grok"] as Agent[]).map((agent) => ({
  agent,
  availability: "not_determined",
}));

type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "event"; event: Event }
  | { kind: "dismiss_trust_prompt"; session: string }
  | { kind: "track_account_login"; key: string; tracked: boolean }
  | { kind: "dismiss_account_login"; login: AccountLogin };

const initialState: State = {
  connectionState: "connecting",
  consoles: new Map(),
  projects: new Map(),
  sessions: new Map(),
  pages: new Map(),
  snapshotEpoch: 0,
  trustPrompts: [],
  trustedDirectories: [],
  sessionRequests: [],
  gitStatuses: new Map(),
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [] },
  agentAvailability: new Map(INITIAL_AGENT_AVAILABILITY.map((a) => [a.agent, a])),
  accountQuotas: [],
  accountLogins: [],
  loginsStartedHere: [],
  accountSharing: new Map(),
  homeDir: null,
  shellTerminals: new Map(),
};

/** A store holding the empty state with `initial` laid over it. */
export function createStateStore(initial: Partial<State> = {}): StoreApi<State> {
  return createStore<State>(() => ({ ...initialState, ...initial }));
}

/** The map of `records` by `idOf`, each one the record `previous` already holds for its id when that
 * is the same data. A snapshot (the first, or a lag recovery) names everything again, and what it
 * repeats unchanged must not look new to the components holding it. */
function keepSame<T>(previous: Map<string, T>, records: T[], idOf: (record: T) => string): Map<string, T> {
  return new Map(
    records.map((record) => {
      const id = idOf(record);
      const held = previous.get(id);
      return [id, held !== undefined && sameValue(held, record) ? held : record];
    }),
  );
}

/** `quotas` with each entry replaced by the one `previous` already holds for its account when that
 * is the same data: the list arrives whole after every reading, and a component showing one
 * account must not render again for another account's reading. */
function keepSameQuotas(previous: AccountQuota[], quotas: AccountQuota[]): AccountQuota[] {
  return quotas.map((quota) => {
    const held = accountQuotaOf(previous, quota.agent, quota.account);
    return held !== undefined && sameValue(held, quota) ? held : quota;
  });
}

/** Whether `sharing` says anything beyond naming the account; one that does not means the account is free. */
function hasSharingState(sharing: AccountSharing): boolean {
  return sharing.in_use !== undefined || sharing.progress !== undefined || sharing.idle_sessions !== undefined || sharing.failure !== undefined;
}

/** What names an account's login among the logins: its agent and account (`null` for the agent's default account). */
export function accountLoginKey(agent: Agent, account: string | null): string {
  return `${agent}:${account ?? ""}`;
}

/** Notes that this window is starting the login of an account, so that it opens the authorization page when it
 * comes (`tracked`), or that it is not any more: the page has been opened, or the request was refused. */
export function trackAccountLogin(store: StoreApi<State>, agent: Agent, account: string | null, tracked: boolean): void {
  store.setState((state) => reducer(state, { kind: "track_account_login", key: accountLoginKey(agent, account), tracked }));
}

/** Drops an ended login from the state, once the user has seen its outcome or it has cleared itself. Does nothing
 * when the login is not the one held, since the account's next login is a new object. */
export function dismissAccountLogin(store: StoreApi<State>, login: AccountLogin): void {
  store.setState((state) => reducer(state, { kind: "dismiss_account_login", login }));
}

/** Applies a daemon `event` to the store, as the daemon connection does for each one it receives. */
export function applyDaemonEvent(store: StoreApi<State>, event: Event): void {
  store.setState((state) => reducer(state, { kind: "event", event }));
}

/** Holds `terminal` as a `shell_terminal_upserted` for it would, unless the store already holds that
 * id. A request's reply and the broadcast of the same change arrive in no fixed order, so a caller
 * that acts on the reply seeds the record first; a record already held came from a broadcast, which
 * is at least as new as the reply, so it stands, and the broadcast of the same creation arriving
 * later changes nothing. */
export function seedShellTerminal(store: StoreApi<State>, terminal: ShellTerminal): void {
  if (store.getState().shellTerminals.has(terminal.id)) return;
  applyDaemonEvent(store, { type: "shell_terminal_upserted", shell_terminal: terminal });
}

/** The shell terminals of the projects still in `projects`. The daemon ends a removed project's
 * terminals and says so with a `shell_terminal_deleted` for each, but a client that kept them until
 * then would offer, for a moment, a terminal of a project that is gone. */
function withoutRemovedProjectTerminals(terminals: Map<string, ShellTerminal>, projects: Map<string, Project>): Map<string, ShellTerminal> {
  if (Array.from(terminals.values()).every((terminal) => projects.has(terminal.project_id))) return terminals;
  return new Map(Array.from(terminals).filter(([, terminal]) => projects.has(terminal.project_id)));
}

function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      return { ...state, connectionState: action.state };
    case "dismiss_trust_prompt":
      return { ...state, trustPrompts: state.trustPrompts.filter((p) => p.session !== action.session) };
    case "track_account_login": {
      const held = state.loginsStartedHere.includes(action.key);
      if (held === action.tracked) return state;
      return { ...state, loginsStartedHere: action.tracked ? [...state.loginsStartedHere, action.key] : state.loginsStartedHere.filter((key) => key !== action.key) };
    }
    case "dismiss_account_login":
      return state.accountLogins.includes(action.login) ? { ...state, accountLogins: state.accountLogins.filter((held) => held !== action.login) } : state;
    case "event": {
      const event = action.event;
      switch (event.type) {
        case "snapshot":
          return {
            ...state,
            hosts: new Map(event.hosts.map((h) => [h.id, h])),
            consoles: keepSame(state.consoles, event.consoles, (c) => c.id),
            projects: keepSame(state.projects, event.projects, (p) => p.id),
            sessions: keepSame(state.sessions, event.sessions, (s) => s.id),
            // A snapshot means either first connect or a lag recovery, and carries no pages either
            // way (see `Page` in protocol.ts). Cleared rather than kept, because a console session
            // that was deleted while this client was behind would otherwise go on holding its
            // pages forever — nothing else ever removes an entry for a session the snapshot no
            // longer lists.
            pages: new Map(),
            snapshotEpoch: state.snapshotEpoch + 1,
            trustPrompts: [],
            trustedDirectories: event.trusted_directories,
            sessionRequests: [],
            gitStatuses: keepSame(state.gitStatuses, event.git_statuses, (s) => s.project),
            settings: event.settings,
            agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])),
            accountQuotas: keepSameQuotas(state.accountQuotas, event.account_quotas),
            accountLogins: event.account_logins,
            loginsStartedHere: state.loginsStartedHere.filter((key) => event.account_logins.some((login) => accountLoginKey(login.agent, login.account) === key)),
            accountSharing: keepSame(state.accountSharing, event.account_sharing.filter(hasSharingState), (sharing) => sharing.account),
            homeDir: event.home_dir,
            shellTerminals: keepSame(state.shellTerminals, event.shell_terminals, (t) => t.id),
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
          if (sameValue(state.consoles.get(event.console.id), event.console)) return state;
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
          return { ...state, consoles, projects, sessions, pages, gitStatuses, shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects) };
        }
        case "session_opened": {
          // A title or status broadcast can arrive before this launch-time reply. Keep the
          // authoritative record if one already exists; otherwise seed the newly opened session.
          if (state.sessions.has(event.session.id)) return state;
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          return { ...state, sessions };
        }
        case "project_upserted": {
          if (sameValue(state.projects.get(event.project.id), event.project)) return state;
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
          return { ...state, projects, sessions, gitStatuses, shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects) };
        }
        case "session_upserted": {
          // A session's status broadcasts repeat what the store holds as often as they change it.
          if (sameValue(state.sessions.get(event.session.id), event.session)) return state;
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
          // A session deleted while it ran leaves no confirmation or request that anyone can answer.
          return {
            ...state,
            sessions,
            pages,
            trustPrompts: state.trustPrompts.filter((p) => p.session !== event.session),
            sessionRequests: state.sessionRequests.filter((r) => r.session !== event.session),
          };
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
        case "session_request": {
          if (state.sessionRequests.some((r) => r.requestId === event.request_id)) return state;
          const request = sessionRequestOf(event);
          if (!request) return state;
          return { ...state, sessionRequests: withRequest(state.sessionRequests, request) };
        }
        case "session_request_closed": {
          const sessionRequests = state.sessionRequests.filter((r) => r.requestId !== event.request_id);
          return sessionRequests.length === state.sessionRequests.length ? state : { ...state, sessionRequests };
        }
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
          if (sameValue(state.gitStatuses.get(event.status.project), event.status)) return state;
          const gitStatuses = new Map(state.gitStatuses);
          gitStatuses.set(event.status.project, event.status);
          return { ...state, gitStatuses };
        }
        case "shell_terminal_upserted": {
          const terminal = event.shell_terminal;
          if (sameValue(state.shellTerminals.get(terminal.id), terminal)) return state;
          const shellTerminals = new Map(state.shellTerminals);
          shellTerminals.set(terminal.id, terminal);
          return { ...state, shellTerminals };
        }
        case "shell_terminal_deleted": {
          if (!state.shellTerminals.has(event.shell_terminal)) return state;
          const shellTerminals = new Map(state.shellTerminals);
          shellTerminals.delete(event.shell_terminal);
          return { ...state, shellTerminals };
        }
        case "settings_updated":
          return { ...state, settings: event.settings };
        case "agent_availability_updated":
          return { ...state, agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])) };
        case "account_quotas_updated":
          return { ...state, accountQuotas: keepSameQuotas(state.accountQuotas, event.account_quotas) };
        case "account_sharing": {
          const { type: _type, ...sharing } = event;
          const accountSharing = new Map(state.accountSharing);
          if (hasSharingState(sharing)) accountSharing.set(sharing.account, sharing);
          else accountSharing.delete(sharing.account);
          return { ...state, accountSharing };
        }
        case "account_login_updated": {
          const { login } = event;
          const others = state.accountLogins.filter((held) => held.agent !== login.agent || held.account !== login.account);
          const key = accountLoginKey(login.agent, login.account);
          const running = login.status === "starting" || login.status === "waiting";
          return {
            ...state,
            accountLogins: [...others, login],
            loginsStartedHere: running ? state.loginsStartedHere : state.loginsStartedHere.filter((held) => held !== key),
          };
        }
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
  /** Sends a request and resolves/rejects with its correlated reply. Callers present failures,
   * allowing field validation to remain local and execution errors to require acknowledgement. */
  request: (body: RequestBody) => Promise<Event>;
  /** `session` is passed by a caller that knows which session the message is about, so the toast
   * can name it; see `ToastRequest.session`. */
  toastError: (message: string | Error, session?: string) => void;
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

export function createDaemon(origin: DaemonOrigin): Daemon {
  const store = createStateStore();
  const dispatch = (action: Action) => store.setState((state) => reducer(state, action));
  // The events that arrived together are one update: a burst renders once, not once per event.
  const dispatchEvents = (events: Event[]) =>
    store.setState((state) => events.reduce((current, event) => reducer(current, { kind: "event", event }), state));

  const client = new DaemonClient(daemonWsUrl(origin, "/ws/control"));
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
        kind: ["launch_failed", "trust_answer_failed", "queued_messages_dropped", "handoff_messages_dropped", "sharing_messages_dropped", "message_not_sent"].includes(event.code) ? "error" : "notice",
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
    toastError: (message, session) => {
      const presentation = typeof message === "string" ? message : errorPresentation(message);
      emitToast({ kind: "error", message: errorText(message), session, error: message,
        ...(typeof presentation !== "string" && { daemonError: presentation }) });
    },
    toastNotice: (message, content) => emitToast({ kind: "notice", message, content }),
    onToast: (listener) => {
      toastListeners.add(listener);
      pendingNotifications.splice(0).forEach(listener);
      return () => void toastListeners.delete(listener);
    },
    dismissTrustPrompt: (session) => dispatch({ kind: "dismiss_trust_prompt", session }),
    reconnect: () => client.reconnect(),
    terminalUrl: (session) => daemonWsUrl(origin, `/ws/term/${session}`),
  };
}

const DaemonContext = createContext<Daemon | undefined>(undefined);

export const DaemonProvider = DaemonContext.Provider;

const EMPTY_ERROR_STORE = createStateStore({});

/** Error acknowledgements also render before a daemon provider is available. */
export function useErrorRecords<T>(selector: (state: State) => T): T {
  const daemon = useContext(DaemonContext);
  return useStore(daemon?.store ?? EMPTY_ERROR_STORE, selector);
}

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

/** A selector's result that is the same value as the last one it gave, whatever new objects it was
 * built from, is that last result. */
function keepingSameValue<T>(): (next: T) => T {
  let last: { value: T } | undefined;
  return (next) => {
    if (last && sameValue(last.value, next)) return last.value;
    last = { value: next };
    return next;
  };
}

/** `useDaemonStore` for a selector that derives something new from the state each time — an object
 * with others inside it, say, which `useShallow` would take as changed every time: the component
 * renders again only when the result differs in what it holds. */
export function useDaemonStoreEqual<T>(selector: (state: State) => T): T {
  const [same] = useState(keepingSameValue<T>);
  return useDaemonStore((state) => same(selector(state)));
}
