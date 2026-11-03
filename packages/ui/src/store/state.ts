import { createStore, type StoreApi } from "zustand";

import type { ConnectionState } from "../daemon-client";
import { DEFAULT_SETTINGS } from "./defaultSettings";
import {
  AGENTS,
  type AccountLogin,
  type AccountQuota,
  type AccountSharing,
  type Agent,
  type AgentAvailability,
  type Console,
  type GitStatus,
  type Host,
  type Page,
  type Project,
  type Session,
  type Settings,
  type ShellTerminal,
} from "../protocol";
import type { SessionRequest } from "./sessionRequests";

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

export interface State {
  connectionState: ConnectionState;
  /** `undefined` until the first `snapshot` arrives — the tree has nothing to render before that. */
  hosts?: Map<string, Host>;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  sessions: Map<string, Session>;
  /** Keyed by the id of the session that pushed them (a console session, or a project session),
   * oldest first, as `page_list` delivers them. Absent for a session whose pages have not been
   * (re-)listed yet — distinct from an empty array, which means they have and there genuinely are
   * none. Pages are deliberately left out of `snapshot` (see `protocol.ts`), so this map starts
   * empty and is filled only by `list_pages` requests: the report panel's, and for a project
   * session the window's own, to know whether its Report panel has anything to show
   * (`report/useProjectReportSession.ts`). */
  pages: Map<string, Page[]>;
  /** The ids of the pages each session has had deleted, kept so that a `page_list` reply computed
   * before a deletion but applied after it does not bring them back. Cleared on a `snapshot`,
   * which clears `pages` too, and with the session. */
  deletedPages: Map<string, ReadonlySet<string>>;
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
  /** The logins this window started whose authorization page it has not opened yet, as `accountKey`s: the
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
  /** The sessions an account-switch batch of this window has not finished with, queued or running
   * (see `setAccountSwitchesQueued`). Client state, which the daemon knows nothing of: `sessions` hold
   * the daemon's records as sent, and what a session shows or allows adds this set to their
   * `Session.switching_account` (`holdsAccountSwitch`). */
  queuedAccountSwitches: ReadonlySet<string>;
}

/** The three entries every run begins with, before the daemon's one-time determination lands. */
const INITIAL_AGENT_AVAILABILITY: AgentAvailability[] = AGENTS.map((agent) => ({
  agent,
  availability: "not_determined",
  default_account_dir: null,
}));

const initialState: State = {
  connectionState: "connecting",
  consoles: new Map(),
  projects: new Map(),
  sessions: new Map(),
  pages: new Map(),
  deletedPages: new Map(),
  snapshotEpoch: 0,
  trustPrompts: [],
  trustedDirectories: [],
  sessionRequests: [],
  gitStatuses: new Map(),
  settings: DEFAULT_SETTINGS,
  agentAvailability: new Map(INITIAL_AGENT_AVAILABILITY.map((a) => [a.agent, a])),
  accountQuotas: [],
  accountLogins: [],
  loginsStartedHere: [],
  accountSharing: new Map(),
  homeDir: null,
  shellTerminals: new Map(),
  queuedAccountSwitches: new Set(),
};

/** A store holding the empty state with `initial` laid over it. */
export function createStateStore(initial: Partial<State> = {}): StoreApi<State> {
  return createStore<State>(() => ({ ...initialState, ...initial }));
}
