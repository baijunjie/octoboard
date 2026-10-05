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
  /** Absolute path of each agent's own config directory, one setting per agent. A session opened in
   * this console reads only its agent's: Claude Code is launched with it as `CLAUDE_CONFIG_DIR`,
   * Codex as `CODEX_HOME`, and for Grok it replaces `~/.grok` as the directory its per-session home
   * is built from — over any value in the user's shell environment. Unset leaves that environment
   * as it is. */
  claude_config_dir?: string | null;
  codex_config_dir?: string | null;
  grok_config_dir?: string | null;
  created_at: number;
}

/** The three per-agent config directory fields of a console, as named on the wire. */
export type ConfigDirField = "claude_config_dir" | "codex_config_dir" | "grok_config_dir";

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
  /** The config directory of this session's own agent that it was started with, fixed at creation
   * so a resume finds its transcript even after the console's setting changes. */
  config_dir?: string | null;
  started_at: number;
  ended_at?: number | null;
}

/** One page the hub pushed to its console's report panel. `anchor_message_id` is stored only — the
 * rewind linkage that reads it is after the MVP. */
export interface Page {
  id: string;
  console_id: string;
  html: string;
  anchor_message_id?: string | null;
  created_at: number;
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
  | {
      type: "create_console";
      name: string;
      hub_agent: Agent;
      default_agent: Agent;
      claude_config_dir?: string;
      codex_config_dir?: string;
      grok_config_dir?: string;
    }
  | {
      type: "update_console";
      console: string;
      name?: string;
      hub_agent?: Agent;
      default_agent?: Agent;
      /** Each config directory: absent leaves it alone; an explicit `null` clears it, so the user's
       * shell environment applies again. */
      claude_config_dir?: string | null;
      codex_config_dir?: string | null;
      grok_config_dir?: string | null;
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
  | { type: "list_pages"; console: string }
  /** What a report panel form was submitted with. The page is named rather than the hub session:
   * it is what the panel knows, and only a console's newest page may be submitted from. */
  | { type: "submit_page"; page: string; data: unknown }
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
  /** The reply to `list_pages`, oldest first. Pages are not in `snapshot` — a page carries a whole
   * HTML document — so the panel asks for them, and asks again after every `snapshot`: a client
   * that falls too far behind the daemon's broadcasts is sent a fresh snapshot in place of the
   * events it missed, on the same socket, so re-listing is the only way back to a correct list. */
  | { type: "page_list"; id?: string; console_id: string; pages: Page[] }
  /** A page the hub just pushed. The panel showing that console's hub refreshes to it. */
  | { type: "page_created"; page: Page }
  | { type: "ack"; id?: string }
  /** `code` is present only for failures a client has to branch on rather than just show — today
   * the one code is `"session_already_running"`. */
  | { type: "error"; id?: string; message: string; code?: string };

/** Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input. */
export type TermControl = { type: "resize"; cols: number; rows: number };
