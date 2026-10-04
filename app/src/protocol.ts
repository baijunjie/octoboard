// Wire types for `daemon/PROTOCOL.md`. Kept as a hand-written mirror of the daemon's
// `protocol.rs` rather than generated, to keep the frontend build free of a codegen step. Field
// names and enum spellings must match `protocol.rs` exactly — in particular, no
// request field is named `id`: what a request acts on is named for its kind (`console`, `project`,
// `session`), because the envelope's own `id` and the request's fields share one flat object.

export type Agent = "claude" | "codex" | "grok";
export type Role = "hub" | "worker";
export type Origin = "hub" | "user";
export type SessionStatus = "working" | "waiting_user" | "idle" | "interrupted" | "archived";
export type HostKind = "local" | "ssh";
export type ProjectSource = "local" | "parent" | "github";

/** Statuses meaning the session has a running process. The complement of `DORMANT_STATUSES`. */
export const LIVE_STATUSES: ReadonlySet<SessionStatus> = new Set(["working", "waiting_user", "idle"]);

/** Statuses meaning no process is running for this session — both are resumable via
 * `resume_session`. */
export const DORMANT_STATUSES: ReadonlySet<SessionStatus> = new Set(["interrupted", "archived"]);

export function isLive(status: SessionStatus): boolean {
  return LIVE_STATUSES.has(status);
}

export function isDormant(status: SessionStatus): boolean {
  return DORMANT_STATUSES.has(status);
}

export interface Host {
  id: string;
  name: string;
  kind: HostKind;
  ssh_config?: string | null;
}

export interface Console {
  id: string;
  name: string;
  workdir: string;
  hub_agent: Agent;
  default_agent: Agent;
  created_at: number;
}

export interface Project {
  id: string;
  console_id: string;
  host_id: string;
  name: string;
  path: string;
  default_agent?: Agent | null;
  source: ProjectSource;
  remote_url?: string | null;
}

export interface Session {
  id: string;
  agent: Agent;
  agent_session_id?: string | null;
  console_id: string;
  project_id?: string | null;
  host_id: string;
  role: Role;
  origin: Origin;
  title: string;
  status: SessionStatus;
  has_conversation: boolean;
  /** Whether this session's reports go to its console's hub. Always true for a hub-started session;
   * for one the user opened by hand, only when they asked for it. */
  include_in_hub: boolean;
  started_at: number;
  ended_at?: number | null;
}

/** Only directories are ever listed — a project is a directory — so there is no "is this a
 * directory" field. */
export interface DirEntry {
  name: string;
  path: string;
  is_git_repo: boolean;
}

/**
 * The body of a client request, without the envelope's `id`. One variant per `RequestBody` case
 * in `protocol.rs`, tagged the same way (`type`, snake_case).
 */
export type RequestBody =
  | { type: "create_console"; name: string; hub_agent: Agent; default_agent: Agent }
  | {
      type: "update_console";
      console: string;
      name?: string;
      hub_agent?: Agent;
      default_agent?: Agent;
    }
  | { type: "delete_console"; console: string }
  | {
      type: "add_project";
      console_id: string;
      source: ProjectSource;
      path?: string;
      remote_url?: string;
      name?: string;
      default_agent?: Agent;
    }
  | {
      type: "update_project";
      project: string;
      name?: string;
      /** Absent leaves the project's default agent alone; an explicit `null` clears it, which is
       * how the project goes back to inheriting the console's default — the two must stay
       * distinguishable, so this field is never sent as `undefined` when the user means "clear". */
      default_agent?: Agent | null;
    }
  | { type: "delete_project"; project: string }
  | { type: "list_dir"; path: string }
  | {
      type: "open_session";
      console_id: string;
      project_id?: string;
      agent?: Agent;
      task?: string;
      title?: string;
      /** Defaults to false: a session opened by hand stays outside the hub's orchestration and
       * sends it no reports unless this is set. */
      include_in_hub?: boolean;
    }
  | { type: "resume_session"; session: string }
  | { type: "archive_session"; session: string }
  | { type: "send_message"; session: string; text: string }
  | { type: "rename_session"; session: string; title: string }
  | { type: "shutdown" };

/** A client request as sent on the wire: the body's fields plus an optional correlation id. */
export type Request = RequestBody & { id?: string };

export type Event =
  | { type: "snapshot"; hosts: Host[]; consoles: Console[]; projects: Project[]; sessions: Session[] }
  | { type: "console_upserted"; console: Console }
  | { type: "console_deleted"; console: string }
  | { type: "project_upserted"; project: Project }
  | { type: "project_deleted"; project: string }
  | { type: "session_upserted"; session: Session }
  /** Something about a session the user has to be told that no status field carries — an injected
   * capability that will not apply, or a setting of theirs Octoboard had to work around. Broadcast
   * once, when the session starts; nothing stores it, so a client that connects later never sees
   * it. */
  | { type: "session_notice"; session: string; message: string }
  /** The reply to `open_session`: the session that was started. The same record is also
   * broadcast as `session_upserted`, but that broadcast carries no request id, so this is the
   * only way the caller can tell which session in the tree is the one it just opened. */
  | { type: "session_opened"; id?: string; session: Session }
  | { type: "dir_listing"; id?: string; path: string; entries: DirEntry[] }
  | { type: "ack"; id?: string }
  /** `code` is present only for failures a client has to branch on rather than just show — today
   * the one code is `"session_already_running"`. */
  | { type: "error"; id?: string; message: string; code?: string };

/** Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input. */
export type TermControl = { type: "resize"; cols: number; rows: number };
