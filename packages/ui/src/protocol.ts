// Wire types for `apps/daemon/PROTOCOL.md`. Kept as a hand-written mirror of the daemon's
// `protocol.rs` rather than generated, to keep the frontend build free of a codegen step. Field
// names and enum spellings must match `protocol.rs` exactly — in particular, no
// request field is named `id`: what a request acts on is named for its kind (`console`, `project`,
// `session`), because the envelope's own `id` and the request's fields share one flat object.

export type Agent = "claude" | "codex" | "grok";
export type Role = "console" | "project";
export type Origin = "console" | "user";
export type SessionStatus = "working" | "waiting_user" | "idle" | "interrupted" | "archived";
export type HostKind = "local" | "ssh";
export type ProjectSource = "local" | "parent" | "git";

/** A console session's badge colour (`Session.colour`), assigned once on creation from this fixed
 * palette and never changed afterwards. Each entry's light and dark CSS values live in the UI's
 * own colour system (`style.css`), not here. */
export type ConsoleSessionColour = "olive" | "jade" | "teal" | "azure" | "violet" | "rose";

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
  console_session_agent: Agent;
  default_agent: Agent;
  /** The account each agent's sessions opened in this console read, by id; absent means that
   * agent's default account — the state of pinning nothing. A session resolves its own: the one
   * chosen for it, else this, else the default. */
  claude_account_id?: string | null;
  codex_account_id?: string | null;
  grok_account_id?: string | null;
  /** A custom avatar as an `image/*` `data:` URL of at most 256 KiB; unset shows the default glyph. */
  icon?: string | null;
  created_at: number;
}

/** The three per-agent account reference fields of a console, as named on the wire. */
export type AccountField = "claude_account_id" | "codex_account_id" | "grok_account_id";

/** A named config directory of one agent, kept once for the whole application. The default
 * account of each agent is not one of these — it is the state of pinning nothing, and what it is
 * shown as is derived on the client from `AgentAvailability`, not sent as a record. */
export interface Account {
  id: string;
  agent: Agent;
  /** Required, unique within this account's agent (trimmed, case-insensitive); not compared
   * across agents. */
  name: string;
  /** Absolute, lexically normalised. Existence is not checked when this is set. */
  config_dir: string;
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
  /** The user has agreed that Octoboard may answer Claude Code's workspace-trust screen for this
   * project's directory. Only ever set by `confirm_claude_trust` with `remember`. */
  claude_trust_consent: boolean;
  /** The user pinned this project to the top of its console's project list. */
  pinned: boolean;
  /** The user's free-form labels for this project, used to filter the project list. There is no tag
   * registry: the tags in use are the distinct ones across projects. Stored trimmed, non-empty and
   * without case-insensitive duplicates, in the order the user gave them. */
  tags: string[];
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
  /** The console session this session is bound to, or absent for one outside the orchestration.
   * Set when the session is created and never changed afterwards; a console session is never
   * bound, so this is always absent for one of those. Reports are routed by this field. */
  bound_to?: string | null;
  /** A console session's badge colour, assigned on creation and fixed afterwards. Absent for a
   * project session, which carries no colour of its own. */
  colour?: ConsoleSessionColour | null;
  /** A console session's place in its console's history — one past the highest ordinal ever used
   * there, so a title is never reused after a console session is archived or deleted — and what
   * gives it its default title ("Hub" for the first, "Hub `<ordinal>`" after). Absent for a project
   * session. */
  ordinal?: number | null;
  /** The account this session's own agent reads, by id; absent means the default account. Written
   * when the session is opened and when its account is switched. */
  account_id?: string | null;
  /** The config directory of this session's own agent that it launches with, recorded with the
   * account so a resume finds its transcript even after the account's own directory changes. */
  config_dir?: string | null;
  /** The user pinned this session to the top of its list; survives archiving and resuming. */
  pinned: boolean;
  started_at: number;
  ended_at?: number | null;
}

/** One page a console session pushed to its report panel; it belongs to that console session, not to its
 * console. `anchor_message_id` is stored only — the rewind linkage that would read it does not exist. */
export interface Page {
  id: string;
  console_session_id: string;
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

/** `checking` while the remote is being contacted and the status read; `syncing` while the branch
 * is being fast-forwarded. A tri-state rather than a boolean so the two in-flight phases are told
 * apart in the UI's wording. */
export type GitActivity = "idle" | "checking" | "syncing";

/** A project's live git state: derived, never stored in SQLite, and held in memory by the daemon.
 * `repository` false means the project's directory is not a git repository, and every field below
 * is then at its empty value. `branch` is the branch name, the short commit id when `detached`, or
 * null when it cannot be read (an empty repository with no commit yet, or a failed read).
 * `upstream` null means the branch has none, which is also when `ahead` and `behind` are
 * meaningless and both zero. `error` is the verbatim, untranslatable message from the last failed
 * step, cleared by a step that succeeds — a failed fetch does not stop the local read, so a status
 * can carry both an error and usable numbers. */
export interface GitStatus {
  project: string;
  repository: boolean;
  branch?: string | null;
  detached: boolean;
  upstream?: string | null;
  ahead: number;
  behind: number;
  activity: GitActivity;
  error?: string | null;
}

/** Whether an agent's binary resolves on the user's login shell `PATH` — the only test an agent is
 * held to. Three states: `not_determined` is where every run of the daemon begins, for every
 * agent, until its one-time login-shell snapshot for this run lands; only that snapshot ever
 * moves an agent to `available` or `unavailable`. "No agent available" is `unavailable` on every
 * agent, never `not_determined` on any of them. */
export type Availability = "not_determined" | "available" | "unavailable";

/** One agent's availability and what its default account currently resolves to, derived once per
 * daemon start from one login-shell snapshot and held as the daemon's own derived state — never a
 * field of `Settings`, which only the user's own updates write. Always three entries, one per
 * agent, replayed in `snapshot` and broadcast whole by `agent_availability_updated`. */
export interface AgentAvailability {
  agent: Agent;
  availability: Availability;
  /** The directory this agent's default account currently resolves to: the directory its own
   * variable is exported to in the snapshot, else the agent's own usual default. For Grok Build
   * this is the *source* home a session's per-session home would be built from, never the
   * per-session home itself. Absent exactly while `availability` is `not_determined`. */
  default_account_dir?: string | null;
}

/** The app-wide settings the daemon stores; it is a record so it can grow. */
export interface Settings {
  /** Off (the default): the periodic check still runs `git fetch` so ahead/behind stays accurate,
   * but nothing in the repository changes. On: a branch that is behind and can fast-forward is
   * also fast-forwarded. Never pushes and never merges a non-fast-forward either way. */
  auto_sync_repositories: boolean;
  /** Where a `git` association clones into when no directory is named: the one the user set, or
   * `~/Projects` expanded while none is. Always an absolute path. */
  default_clone_dir: string;
  /** Every account of every agent, application-wide. The default account of each agent is not
   * among these — it is the state of pinning nothing, not a row. */
  accounts: Account[];
}

/** Where a project's files are read from — the reply to `get_project_source`. Every path-like
 * string here is a wire path (see "Wire paths" in `apps/daemon/PROTOCOL.md`): canonical text that
 * is the name's identity, shown by decoding it to bytes and reading those as UTF-8. */
export interface ProjectSourceInfo {
  project: string;
  /** The project's path as stored (a wire path too). */
  root: string;
  /** The directory it resolves to with its symbolic links followed. */
  resolved_root: string;
  /** That directory's identity; it changes when the directory is replaced. */
  root_id: string;
  /** Null when the directory is in no Git repository, or `git` cannot read it or list its
   * worktrees (`git_error`). */
  git: GitSourceInfo | null;
  git_error: string | null;
}

export interface GitSourceInfo {
  repository: string;
  common_dir: string;
  /** The worktree holding the project's directory. */
  worktree: string;
  /** The project's directory relative to that worktree's root; empty at the root. */
  scope: string;
  /** Every worktree of the repository, the one holding the project included. */
  worktrees: WorktreeInfo[];
}

export interface WorktreeInfo {
  /** Checked again on every request that names it: `worktree_unavailable` once the worktree is gone. */
  id: string;
  root: string;
  main: boolean;
  head: string | null;
  branch: string | null;
  /** Whether the project's scope is a directory in this worktree, reached through no symbolic link. */
  scope_present: boolean;
}

/** One entry of `project_dir`, in byte order of the names' wire forms. */
export interface BrowseEntry {
  name: string;
  kind: "file" | "directory" | "symlink" | "other";
  size: number | null;
  target: "file" | "directory" | "other" | "missing" | "outside" | null;
}

/** Which content `read_project_file` reads; absent means `live`. A branch is the name below
 * `refs/heads/`, as a wire path; a commit is a full object id. */
export type ReadFrom =
  | { kind: "live" }
  | { kind: "index" }
  | { kind: "branch"; branch: string }
  | { kind: "commit"; commit: string };

/** What a body was read from: the identity a client compares to decide whether a reply is still
 * the one it is showing. `version` changes whenever a live file may have; `blob` and `commit` name
 * immutable objects. */
export type ContentSource =
  | { kind: "live"; root_id: string; version: string }
  | { kind: "index"; worktree: string; blob: string }
  | { kind: "commit"; commit: string; branch: string | null; blob: string };

/** A file body: valid UTF-8 without NUL that JSON escaping at most doubles in `text`, anything else
 * base64-encoded in `data`. `media_type` is an image-format hint from a binary body's first bytes;
 * it is null for every text body, an SVG included. */
export interface FileContent {
  size: number;
  kind: "text" | "binary";
  media_type: string | null;
  text: string | null;
  data: string | null;
}

/**
 * The body of a client request, without the envelope's `id`. One variant per `RequestBody` case
 * in `protocol.rs` and per `BrowseBody` case (the browse requests, whose envelope also carries
 * `slot`), tagged the same way (`type`, snake_case).
 */
export type RequestBody =
  | {
      type: "create_console";
      name: string;
      console_session_agent: Agent;
      default_agent: Agent;
      /** Each agent's account, by id; absent means that agent's default account. */
      claude_account_id?: string;
      codex_account_id?: string;
      grok_account_id?: string;
      icon?: string;
    }
  | {
      type: "update_console";
      console: string;
      name?: string;
      console_session_agent?: Agent;
      default_agent?: Agent;
      /** Each agent's account: absent leaves it alone; an explicit `null` clears it back to that
       * agent's default account. */
      claude_account_id?: string | null;
      codex_account_id?: string | null;
      grok_account_id?: string | null;
      /** Absent leaves the avatar alone; an explicit `null` clears it back to the default glyph. */
      icon?: string | null;
    }
  | { type: "delete_console"; console: string }
  | {
      type: "add_project";
      console_id: string;
      source: ProjectSource;
      /** Required for `local` and `parent`; for `git`, absent clones into the default clone directory. */
      path?: string;
      remote_url?: string;
      name?: string;
      default_agent?: Agent;
      /** Absent means no tags. */
      tags?: string[];
    }
  | {
      type: "update_project";
      project: string;
      name?: string;
      /** Absent leaves the project's default agent alone; an explicit `null` clears it, which is
       * how the project goes back to inheriting the console's default — the two must stay
       * distinguishable, so this field is never sent as `undefined` when the user means "clear". */
      default_agent?: Agent | null;
      /** Absent leaves the project's pin alone. */
      pinned?: boolean;
      /** Absent leaves the project's tags alone; a present array replaces them wholesale. */
      tags?: string[];
    }
  /** `stop_sessions` ends the project's running sessions first (as `archive_session` does) instead
   * of being refused with `project_has_running_sessions`; default false. */
  | { type: "delete_project"; project: string; stop_sessions?: boolean }
  | { type: "list_dir"; path: string }
  | {
      type: "open_session";
      console_id: string;
      project_id?: string;
      agent?: Agent;
      /** The account this session's agent reads, by id. Absent leaves it to the console's account
       * for that agent, else the default; an explicit `null` chooses the default account outright,
       * whatever the console refers to. */
      account?: string | null;
      task?: string;
      title?: string;
      /** The console session this (project) session should report to. Absent means none: a
       * session the user opens by hand stays outside the orchestration unless they choose one.
       * Ignored for the console session itself, which is never bound. */
      bound_to?: string;
    }
  | { type: "resume_session"; session: string }
  | { type: "archive_session"; session: string }
  /** Moves the session to another account of its own agent: its process ends, its conversation is
   * copied into that account's directory, and it is relaunched there. `account` is the target by
   * id, or `null` for the default account; it is always sent. Answered once the relaunched session
   * has stayed up, or with the reason it did not. */
  | { type: "switch_session_account"; session: string; account: string | null }
  /** Removes Octoboard's record of one archived session; refused with `session_not_archived`
   * otherwise. The agent's own transcript is never touched. */
  | { type: "delete_session"; session: string }
  /** With `project`: every archived session of that project. With `console_session`: every
   * archived session bound to it. With neither: every archived console session of the console.
   * Naming both is refused. Each removal is broadcast as `session_deleted`. */
  | { type: "delete_archived_sessions"; console: string; project?: string; console_session?: string }
  | { type: "set_session_pinned"; session: string; pinned: boolean }
  | { type: "send_message"; session: string; text: string }
  | { type: "rename_session"; session: string; title: string }
  | { type: "list_pages"; console_session: string }
  /** What a report panel form was submitted with. The page is named rather than the console
   * session: it is what the panel knows, and it names the console session the submission goes to.
   * Only that console session's newest page may be submitted from. */
  | { type: "submit_page"; page: string; data: unknown }
  /** The user's go-ahead to a `claude_trust_prompt`: Octoboard may answer that session's trust
   * screen. `remember` also records the project's consent, so its later sessions are answered
   * without a prompt; `trust_parent_dir` records the project's parent directory as trusted instead
   * (the daemon derives it; it is the prompt's `trust_dir`), covering every project under it. */
  | { type: "confirm_claude_trust"; session: string; remember: boolean; trust_parent_dir?: boolean }
  /** Stops trusting a directory; projects' own consents are untouched. */
  | { type: "remove_trusted_directory"; path: string }
  /** Each settable field is optional: absent means leave it as it is. Answered with `ack`;
   * broadcasts `settings_updated` only when something actually changed (the trusted-folders
   * pattern). */
  | { type: "update_settings"; auto_sync_repositories?: boolean; default_clone_dir?: string }
  /** Answered with `ack` at once; the statuses arrive as `project_git_status` broadcasts as each
   * project finishes. Every project of `console` is checked, concurrently. A project whose check
   * is already in flight is not started again, so a client polling faster than the checks finish,
   * or several clients watching the same console, cannot pile work up. */
  | { type: "refresh_git_status"; console: string }
  /** Both fields are required: an account always has a name and a directory. Broadcasts
   * `settings_updated`. */
  | { type: "create_account"; agent: Agent; name: string; config_dir: string }
  /** Renames the account, repoints it, or both, independently; either field absent leaves it
   * alone. Broadcasts `settings_updated`, and a `console_upserted` for every console that refers
   * to it. */
  | { type: "update_account"; account: string; name?: string; config_dir?: string }
  /** Clears the reference of every console that refers to this account, putting each back on its
   * agent's default account, then removes the account. Broadcasts `settings_updated`, and a
   * `console_upserted` for every console whose reference was cleared. */
  | { type: "delete_account"; account: string }
  /** The browse requests (see "Browsing a project" in `apps/daemon/PROTOCOL.md`), answered with
   * `project_source`, `project_dir` and `project_file`. A newer browse request on this connection
   * with the same `slot` supersedes an older one still outstanding, which is answered
   * `request_superseded`; one that finished first still gets its real reply, so a reply is also
   * discarded by its `id`. */
  | { type: "get_project_source"; project: string; slot?: string }
  | { type: "list_project_dir"; project: string; path: string; worktree?: string; slot?: string }
  | {
      type: "read_project_file";
      project: string;
      path: string;
      worktree?: string;
      from?: ReadFrom;
      slot?: string;
    }
  | { type: "shutdown" };

/** A client request as sent on the wire: the body's fields plus an optional correlation id. */
export type Request = RequestBody & { id?: string };

export type Event =
  | {
      type: "snapshot";
      hosts: Host[];
      consoles: Console[];
      projects: Project[];
      sessions: Session[];
      /** Directories whose projects Octoboard answers Claude Code's trust prompt for. */
      trusted_directories: string[];
      settings: Settings;
      /** Every status the daemon currently holds; empty on a fresh start. Keeps a reconnecting
       * client from having to re-ask. */
      git_statuses: GitStatus[];
      /** Always three entries, one per agent, each `not_determined` until the daemon's one-time
       * determination for this run lands. */
      agent_availability: AgentAvailability[];
      /** The daemon host's home directory — not the browser's, the two may be on different
       * machines — so a path under it can be shown as `~/...`. Null when the daemon cannot
       * determine one. */
      home_dir: string | null;
    }
  /** Availability or a default account's resolved directory changed for one or more agents — the
   * whole three-entry list. Broadcast once, when the daemon's one-time determination lands; never
   * again afterwards. */
  | { type: "agent_availability_updated"; agent_availability: AgentAvailability[] }
  /** The whole list of trusted directories, sent when it changes. */
  | { type: "trusted_directories_updated"; trusted_directories: string[] }
  | { type: "console_upserted"; console: Console }
  | { type: "console_deleted"; console: string }
  | { type: "project_upserted"; project: Project }
  | { type: "project_deleted"; project: string }
  /** Broadcast whenever a project's `GitStatus` changes, including each transition of `activity`,
   * so the animated icon has something to follow. When a project is removed, no deletion event is
   * sent; the client drops the status with the project. */
  | { type: "project_git_status"; status: GitStatus }
  | { type: "settings_updated"; settings: Settings }
  | { type: "session_upserted"; session: Session }
  | { type: "session_deleted"; session: string }
  /** Something about a session the user has to be told that no status field carries — an injected
   * capability that will not apply, or a setting of theirs Octoboard had to work around. Broadcast
   * once, when the session starts; nothing stores it, so a client that connects later never sees
   * it. */
  | { type: "session_notice"; session: string; code: string; params: MessageParams; message: string }
  /** The reply to `open_session`: the session that was started. The same record is also
   * broadcast as `session_upserted`, but that broadcast carries no request id, so this is the
   * only way the caller can tell which session in the tree is the one it just opened. */
  | { type: "session_opened"; id?: string; session: Session }
  /** A Claude Code session of a project the user has not agreed Octoboard may answer for is at its
   * workspace-trust screen, which asks whether `path` is trusted. Broadcast once per screen, and sent
   * again to a client right after each `snapshot` (connect or lag recovery) for every screen still
   * waiting, so a client that missed it is still asked; a repeat is ignored. */
  | {
      type: "claude_trust_prompt";
      session: string;
      project: string;
      path: string;
      /** The directory `trust_parent_dir` would trust, or null when there is none to offer (too broad
       * to trust, the home directory cannot be determined, or `path` is not absolute): the button is shown only when it is not null. */
      trust_dir: string | null;
    }
  | { type: "dir_listing"; id?: string; path: string; entries: DirEntry[] }
  /** The reply to `list_pages`, oldest first. Pages are not in `snapshot` — a page carries a whole
   * HTML document — so the panel asks for them, and asks again after every `snapshot`: a client
   * that falls too far behind the daemon's broadcasts is sent a fresh snapshot in place of the
   * events it missed, on the same socket, so re-listing is the only way back to a correct list. */
  | { type: "page_list"; id?: string; console_session_id: string; pages: Page[] }
  /** A page the console session just pushed. The panel showing that console session refreshes to it. */
  | { type: "page_created"; page: Page }
  | { type: "project_source"; id?: string; source: ProjectSourceInfo }
  /** `project`, `worktree` and `path` echo the request; `complete` is false when the listing was
   * cut at a budget or an entry could not be read. */
  | {
      type: "project_dir";
      id?: string;
      project: string;
      worktree: string | null;
      path: string;
      root_id: string;
      entries: BrowseEntry[];
      complete: boolean;
    }
  | {
      type: "project_file";
      id?: string;
      project: string;
      worktree: string | null;
      path: string;
      source: ContentSource;
      file: FileContent;
    }
  | { type: "ack"; id?: string }
  /** A failure, worded from `code` and `params` (see `daemonMessage.ts`); `message` is the English
   * text, shown for a code the client does not know. A client also branches on some codes — see
   * `ALREADY_RUNNING_CODES`, `TRUST_REFUSED_CODES`, `CLAUDE_TRUST_NOT_WAITING`, `REQUEST_SUPERSEDED` and
   * `SOURCE_CHANGED`. */
  | { type: "error"; id?: string; code: string; params: MessageParams; message: string };

/** The named values a daemon message is filled with. `console`, `project` and `session` are record
 * ids, which `daemonMessage` shows as the record's name; the rest is text to show as is. */
export type MessageParams = Record<string, string>;

/** Codes meaning a launch was refused because the session was already running or already being
 * started. A double click produces them and is not worth showing. */
export const ALREADY_RUNNING_CODES: readonly string[] = ["session_already_running", "session_already_starting"];

/** Codes meaning no parent directory can be offered to trust: nothing was answered, and the dialog
 * stays open. */
export const TRUST_REFUSED_CODES: readonly string[] = [
  "trust_directory_too_broad",
  "trust_path_not_absolute",
  "trust_home_unknown",
];

/** The go-ahead was for a trust screen no longer waiting: nothing is wrong, so nothing is shown. */
export const CLAUDE_TRUST_NOT_WAITING = "claude_trust_not_waiting";

/** A browse request given up for a newer one in its slot, as the client asked: nothing is shown. */
export const REQUEST_SUPERSEDED = "request_superseded";

/** What was being read changed while it was read: reading it again gets a consistent copy. */
export const SOURCE_CHANGED = "source_changed";

/** Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input. */
export type TermControl = { type: "resize"; cols: number; rows: number };
