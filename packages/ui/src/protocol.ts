// Wire types for `apps/daemon/PROTOCOL.md`. Kept as a hand-written mirror of the daemon's
// `protocol.rs` rather than generated, to keep the frontend build free of a codegen step. Field
// names and enum spellings must match `protocol.rs` exactly — in particular, no
// request field is named `id`: what a request acts on is named for its kind (`console`, `project`,
// `session`), because the envelope's own `id` and the request's fields share one flat object.

/** Unknown when the daemon cannot prove presence or absence of the sessions’ native files. */
export type SessionRecordsPresence = "present" | "absent" | "unknown";

export type Agent = "claude" | "codex" | "grok";
export type Role = "console" | "project";
export type Origin = "console" | "user";
export type SessionStatus = "starting" | "working" | "waiting_user" | "idle" | "interrupted" | "archived";
export type HostKind = "local" | "ssh";
export type ProjectSource = "local" | "parent" | "git";

/** A console session's badge colour (`Session.colour`): one of a fixed palette of sixteen, in hue
 * order around the colour wheel and then the muted ones, assigned on creation and changed
 * afterwards only by the user. The ids are never shown: the user sees the colour, not a name. What
 * each one looks like in the light and the dark appearance is the UI's own (`sessionColours.ts`),
 * not the daemon's. */
export const CONSOLE_SESSION_COLOURS = [
  "red", "orange", "amber", "yellow", "lime", "green", "teal", "cyan", "sky", "blue", "indigo", "violet", "fuchsia", "pink", "brown", "gray",
] as const;
export type ConsoleSessionColour = (typeof CONSOLE_SESSION_COLOURS)[number];

/** Statuses meaning the session has a running process or is about to: `starting` is one whose agent
 * is still being launched, so it has no terminal to attach to yet. The complement of
 * `DORMANT_STATUSES`. */
export const LIVE_STATUSES: ReadonlySet<SessionStatus> = new Set(["starting", "working", "waiting_user", "idle"]);

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
  default_agent: Agent;
  /** The account each agent's sessions opened in this console read, by id; absent means that
   * agent's default account — the state of pinning nothing. A session resolves its own: the one
   * chosen for it, else this, else the default. */
  claude_account_id?: string | null;
  codex_account_id?: string | null;
  grok_account_id?: string | null;
  /** The model and reasoning effort a console session of this console is launched with, in the
   * agent's own spelling; one pair per agent, and a session reads only its own agent's, unless the
   * session overrides it. Absent means the agent's own default. Read when a console session is opened
   * or resumed, so one already running keeps what it started with; project sessions never read
   * them. */
  claude_console_session_model?: string | null;
  claude_console_session_effort?: string | null;
  codex_console_session_model?: string | null;
  codex_console_session_effort?: string | null;
  grok_console_session_model?: string | null;
  grok_console_session_effort?: string | null;
  /** A custom avatar as an `image/*` `data:` URL of at most 256 KiB; unset shows the default glyph. */
  icon?: string | null;
  /** The user's own text added after the console session's built-in instructions; unset adds
   * nothing. */
  console_session_prompt?: string | null;
  created_at: number;
}

/** What a project says about one agent's account: one state more than a console's reference, which
 * is an account or the agent's default account. */
export type ProjectAccount =
  /** Take the console's reference for the agent. What a project that never set one says, and what
   * a deleted account leaves behind. */
  | { kind: "inherit" }
  /** The agent's default account, the state of pinning nothing, even where the console references
   * a named account. */
  | { kind: "default" }
  /** The account with this id, which belongs to the agent the setting is for. */
  | { kind: "account"; id: string };

/** The three per-agent account reference fields of a console, as named on the wire. */
export type AccountField = "claude_account_id" | "codex_account_id" | "grok_account_id";

/** The per-agent model and reasoning effort fields of a console (its console sessions' launch
 * settings), as named on the wire. */
export type ConsoleModelField = "claude_console_session_model" | "codex_console_session_model" | "grok_console_session_model";
export type ConsoleEffortField =
  | "claude_console_session_effort"
  | "codex_console_session_effort"
  | "grok_console_session_effort";

/** The same pair of fields on a project (its sessions' launch settings). */
export type ProjectModelField = "claude_session_model" | "codex_session_model" | "grok_session_model";
export type ProjectEffortField = "claude_session_effort" | "codex_session_effort" | "grok_session_effort";

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
  /** The account each agent's sessions opened in this project read; always present, with
   * `{ kind: "inherit" }` taking the console's reference. A session resolves its own: the one
   * chosen for it, else this (unless it inherits), else the console's, else the agent's default
   * account. */
  claude_account_id: ProjectAccount;
  codex_account_id: ProjectAccount;
  grok_account_id: ProjectAccount;
  /** The model and reasoning effort a session of this project is launched with, in the agent's own
   * spelling; one pair per agent, and a session reads only its own agent's unless it overrides it. Absent means the
   * agent's own default: there is no console-level fallback. Read when a session is opened or
   * resumed, so one already running keeps what it started with. */
  claude_session_model?: string | null;
  claude_session_effort?: string | null;
  codex_session_model?: string | null;
  codex_session_effort?: string | null;
  grok_session_model?: string | null;
  grok_session_effort?: string | null;
  /** The user has agreed that Octoboard may press any agent's own trust confirmation for this
   * project's directory without asking: the project form of the one permission every agent shares.
   * Only ever set by `confirm_trust` with `remember`, once a confirmation has been pressed. */
  trust_consent: boolean;
  /** The user pinned this project to the top of its console's project list. */
  pinned: boolean;
  /** The user's free-form labels for this project, used to filter the project list. There is no tag
   * registry: the tags in use are the distinct ones across projects. Stored trimmed, non-empty and
   * without case-insensitive duplicates, in the order the user gave them. */
  tags: string[];
}

/** The agent's recorded cumulative usage. Cache counts are subsets of input_tokens. */
export interface SessionUsage {
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens?: number;
  cache_write_input_tokens?: number;
  total_tokens: number;
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
  terminal_title?: string | null;
  status: SessionStatus;
  has_conversation: boolean;
  /** The console session this session is bound to, or absent for one outside the orchestration.
   * Set when the session is created and never changed afterwards; a console session is never
   * bound, so this is always absent for one of those. Reports are routed by this field. */
  bound_to?: string | null;
  /** A console session's badge colour, assigned on creation and changed afterwards only by
   * `set_session_colour`. Absent for a project session, which carries no colour of its own. */
  colour?: ConsoleSessionColour | null;
  /** A console session's place in its console's history, one past the highest ordinal ever used
   * there. Never reused after archiving or deletion. Absent for a project session. */
  ordinal?: number | null;
  /** The account this session's own agent reads, by id; absent means the default account. Written
   * when the session is opened and when its account is switched. */
  account_id?: string | null;
  /** The config directory of this session's own agent that it launches with, recorded with the
   * account so a resume finds its transcript even after the account's own directory changes. */
  config_dir?: string | null;
  /** Absent follows the project's or console's current setting; null explicitly uses the
   * agent's own default. Overrides survive resuming and switching accounts. */
  model?: string | null;
  effort?: string | null;
  /** The user pinned this session to the top of its list; survives archiving and resuming. */
  pinned: boolean;
  started_at: number;
  ended_at?: number | null;
  usage?: SessionUsage | null;
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

/** `checking` while the remote is being contacted and the status read; `pulling` while the branch
 * is being fast-forwarded; `pushing` while at least one push is in flight, taking precedence over
 * a simultaneous check or pull. */
export type GitActivity = "idle" | "checking" | "pulling" | "pushing";

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

/** One model an agent offers, as `list_agent_models` reports it. */
export interface AgentModel {
  /** What is passed to the agent as its model: Claude Code's alias (`best`, `opus`, `sonnet`,
   * `haiku`), or the identifier the Codex or Grok Build catalogue lists. */
  id: string;
  /** The name to show. */
  name: string;
  /** The reasoning-effort levels to offer for the model, in the agent's own spelling: the
   * catalogue's own where the model's entry gives them, else the agent's whole measured set;
   * `[]` only for a model that takes none, and never unknown. A level the model does not
   * advertise can still be refused by the agent, or quietly lowered (Claude Code does so for a
   * level its resolved model does not support), so a listed level is one to offer, not a promise
   * that it is honoured. */
  efforts: string[];
}

/** Why `list_agent_models` has no list, as a coded message worded like an `error`. Codes:
 * `models_binary_not_found`, `models_shell_environment_failed`, `models_timed_out`,
 * `models_command_failed`, `models_unreadable_output`. */
export interface ModelsUnavailable {
  code: string;
  params: Record<string, string>;
  message: string;
}

/** The app-wide settings the daemon stores; it is a record so it can grow. */
export interface Settings {
  /** Off (the default): the periodic check still runs `git fetch` so ahead/behind stays accurate,
   * but nothing in the repository changes. On: a branch that is behind and can fast-forward is
   * also fast-forwarded. Never pushes and never merges a non-fast-forward either way. */
  auto_pull_repositories: boolean;
  /** Where a `git` association clones into when no directory is named: the one the user set, or
   * `~/Projects` expanded while none is. Always an absolute path. */
  default_clone_dir: string;
  /** Every account of every agent, application-wide. The default account of each agent is not
   * among these — it is the state of pinning nothing, not a row. */
  accounts: Account[];
}

/** An account reference of a configuration document: the account's name alone, since the agent is
 * the one whose field holds it. */
export interface AccountName {
  name: string;
}

/** A project's account reference in a configuration document; keeps the three states of
 * `ProjectAccount`, naming the account instead of carrying its id. */
export type DocumentProjectAccount = { kind: "inherit" } | { kind: "default" } | { kind: "account"; name: string };

export interface DocumentProject {
  name: string;
  /** An absolute path of the exporting machine: the one machine-local path in the document. */
  path: string;
  source: ProjectSource;
  remote_url?: string | null;
  default_agent?: Agent | null;
  pinned: boolean;
  tags: string[];
  claude_account: DocumentProjectAccount;
  codex_account: DocumentProjectAccount;
  grok_account: DocumentProjectAccount;
  claude_session_model?: string | null;
  claude_session_effort?: string | null;
  codex_session_model?: string | null;
  codex_session_effort?: string | null;
  grok_session_model?: string | null;
  grok_session_effort?: string | null;
}

export interface DocumentConsole {
  name: string;
  icon?: string | null;
  default_agent: Agent;
  /** Each agent's account by name; `null` or absent is that agent's default account. */
  claude_account?: AccountName | null;
  codex_account?: AccountName | null;
  grok_account?: AccountName | null;
  claude_console_session_model?: string | null;
  claude_console_session_effort?: string | null;
  codex_console_session_model?: string | null;
  codex_console_session_effort?: string | null;
  grok_console_session_model?: string | null;
  grok_console_session_effort?: string | null;
  /** The console's own addition to its console session's instructions; absent is none. */
  console_session_prompt?: string | null;
  projects: DocumentProject[];
}

/** The application settings a full export carries. */
export interface DocumentSettings {
  auto_pull_repositories: boolean;
}

/** What `export_configuration` answers and a user keeps as a file. The client saves it as it is
 * and sends it back unchanged to `inspect_configuration` and `import_configuration`; it is built
 * by the daemon alone. */
export interface ConfigurationDocument {
  format: "octoboard-configuration";
  version: number;
  /** Present in a full export only. */
  settings?: DocumentSettings | null;
  consoles: DocumentConsole[];
}

/** The directory a project of an imported document can be placed in. */
export type ImportChoiceKind = "exported_path" | "default_clone_dir" | "clone" | "directory";

/** The choice made for one project when applying; `directory` names the directory the user
 * pointed at, absolute or starting with `~/`. */
export type ImportChoice =
  | { kind: "exported_path" }
  | { kind: "default_clone_dir" }
  | { kind: "clone" }
  | { kind: "directory"; path: string };

/** The choice for the project at index `project` of the document's console at index `console`. */
export interface ProjectResolution {
  console: number;
  project: number;
  choice: ImportChoice;
}

/** An account reference of the document that names no account of its agent on this machine, and
 * so is bound to that agent's default account. */
export interface AccountFallback {
  agent: Agent;
  name: string;
}

/** The directory each choice would use for a project, present only for the choices its state
 * allows; `directory` is never offered, any project can be placed that way. */
export interface ImportOffers {
  exported_path: string | null;
  default_clone_dir: string | null;
  clone: string | null;
}

export interface InspectedProject {
  name: string;
  /** As the document records it. */
  path: string;
  remote_url: string | null;
  offers: ImportOffers;
  /** The choice that is preselected; none when nothing is offered. */
  default: Exclude<ImportChoiceKind, "directory"> | null;
  account_fallbacks: AccountFallback[];
}

export interface InspectedConsole {
  name: string;
  account_fallbacks: AccountFallback[];
  projects: InspectedProject[];
}

/** The application settings a document carries beside what this machine has now. */
export interface InspectedSettings {
  auto_pull_repositories: boolean;
  current_auto_pull_repositories: boolean;
}

/** An `error` reply's `code`, `params` and `message`, for something an import could not do. */
export interface ImportFailure {
  code: string;
  params: MessageParams;
  message: string;
}

export type ImportStatus = "created" | "left_out" | "failed" | "not_attempted";

export interface ImportedProject {
  name: string;
  status: ImportStatus;
  /** How the directory of a created project was obtained. */
  via: ImportChoiceKind | null;
  project: string | null;
  error: ImportFailure | null;
  account_fallbacks: AccountFallback[];
}

export interface ImportedConsole {
  name: string;
  /** `created` or `failed`. */
  status: "created" | "failed";
  console: string | null;
  error: ImportFailure | null;
  account_fallbacks: AccountFallback[];
  projects: ImportedProject[];
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
  /** A regular file's version, equal to a live read's `version` while the file stays unchanged;
   * null for anything else. */
  version: string | null;
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

/** Which of a worktree's uncommitted changes an entry is: `HEAD` against the index, the index
 * against the disk, or a file on disk Git neither tracks nor ignores. Part of a change's identity. */
export type ChangeGroup = "staged" | "unstaged" | "untracked";

/** One side of a listed change (see "Changes and comparisons" in `apps/daemon/PROTOCOL.md`). A
 * present side's `path` is a wire path relative to the project and `source` the version listed; an
 * out-of-scope side's `repository_path` is relative to the repository's root, for display only. */
export type ChangeSide =
  | { state: "present"; path: string; kind: "file" | "symlink" | "submodule"; source: ContentSource }
  | { state: "absent" }
  | { state: "out_of_scope"; repository_path: string };

export type ConflictKind =
  | "both_modified"
  | "both_added"
  | "both_deleted"
  | "added_by_us"
  | "added_by_them"
  | "deleted_by_us"
  | "deleted_by_them";

/** One entry of `project_changes` or `project_comparison`: a change between two sides, or a path
 * in conflict, which is no two-sided change and is read as a live file. A `committed` change is one
 * of a branch comparison's, its old side from the left branch's commit and its new side from the
 * right's. */
export type ChangeEntry =
  | { group: ChangeGroup | "committed"; old: ChangeSide; new: ChangeSide }
  | { group: "conflicted"; path: string; conflict: ConflictKind };

/** One local branch, as `project_branches` lists it: its name below `refs/heads/` (a wire path) and
 * the object id its reference named when it was listed — the commit at its tip, unless the branch
 * is broken (a missing object, or not a commit), which a comparison then refuses. */
export interface BranchInfo {
  name: string;
  commit: string;
  /** The session that created the branch; absent when the daemon cannot name one: no session's own
   * git reported creating it. */
  origin?: BranchOrigin;
}

/** The session a branch is attributed to having created. `title` is what that session was called
 * when the attribution was made, since `session` may name one that no longer exists (a project's
 * deletion takes its sessions with it). `source` is how the daemon told: `ref_hook`, the session's
 * own git reporting the branch's creation. `created_at` is milliseconds since the epoch. */
export interface BranchOrigin {
  source: "ref_hook";
  session: string;
  title: string;
  created_at: number;
}

/** One end of a branch comparison: the branch (a wire path) and the full id of the commit it was
 * resolved to. A read of one of the comparison's changes names both back, with the comparison's
 * `base`; the commits alone decide what is read. */
export interface ComparisonEndpoint {
  branch: string;
  commit: string;
}

/** How `read_project_change` and `read_project_comparison_change` name one side, as the listing
 * gave it. */
export type SideRef = { state: "present"; path: string } | { state: "absent" } | { state: "out_of_scope" };

/** The version of one side of a change as a diff reply reported it, which a bodies request names
 * back so the bodies are read only from the sides the patch was made from. */
export type SideVersion = { state: "present"; source: ContentSource } | { state: "absent" };

/** One side of a change as `read_project_change` read it. `file` is its body, carried only where no
 * patch shows the content: a side alone, or both sides of a binary change. `lines` is how many
 * lines the stored content of that very version holds (an unterminated last line counts, so an
 * empty side is 0); `null` when the daemon does not know: a link or a submodule, a side read from
 * the worktree disk unless its body is carried, and either side of a patch with no hunk that no
 * body is carried for (a change of mode alone, a pure rename) — a binary change's patch has no hunk
 * either, and its two sides are counted from the bodies the reply carries. The disk side is not
 * counted because a clean filter such as Git-LFS makes the count of the file on disk disagree with
 * the patch the diff was made from (see "Changes and comparisons" in `apps/daemon/PROTOCOL.md`). */
export type SideRead =
  | { state: "present"; path: string; kind: "file" | "symlink" | "submodule"; source: ContentSource; file: FileContent | null; lines: number | null }
  | { state: "absent" }
  | { state: "out_of_scope" };

/**
 * The body of a client request, without the envelope's `id`. One variant per `RequestBody` case
 * in `protocol.rs` and per `BrowseBody` case (the browse requests, whose envelope also carries
 * `slot`), tagged the same way (`type`, snake_case).
 */
export type RequestBody =
  | {
    type: "create_console";
    name: string;
    default_agent: Agent;
    /** Each agent's account, by id; absent means that agent's default account. */
    claude_account_id?: string;
    codex_account_id?: string;
    grok_account_id?: string;
    /** Each agent's model and reasoning effort for the console's console sessions; absent or
       * blank means the agent's own default. */
    claude_console_session_model?: string;
    claude_console_session_effort?: string;
    codex_console_session_model?: string;
    codex_console_session_effort?: string;
    grok_console_session_model?: string;
    grok_console_session_effort?: string;
    icon?: string;
    /** Absent or blank means no addition to the console session's instructions. */
    console_session_prompt?: string;
  }
  | {
    type: "update_console";
    console: string;
    name?: string;
    default_agent?: Agent;
    /** Each agent's account: absent leaves it alone; an explicit `null` clears it back to that
       * agent's default account. */
    claude_account_id?: string | null;
    codex_account_id?: string | null;
    grok_account_id?: string | null;
    /** Each agent's console session model and effort: absent leaves it alone; an explicit `null`
       * (or a blank string) clears it back to the agent's own default. */
    claude_console_session_model?: string | null;
    claude_console_session_effort?: string | null;
    codex_console_session_model?: string | null;
    codex_console_session_effort?: string | null;
    grok_console_session_model?: string | null;
    grok_console_session_effort?: string | null;
    /** Absent leaves the avatar alone; an explicit `null` clears it back to the default glyph. */
    icon?: string | null;
    /** Absent leaves the addition alone; an explicit `null` (or a blank string) clears it. */
    console_session_prompt?: string | null;
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
    /** Each agent's account; absent or `null` means inheriting the console's reference. */
    claude_account_id?: ProjectAccount | null;
    codex_account_id?: ProjectAccount | null;
    grok_account_id?: ProjectAccount | null;
    /** Each agent's model and reasoning effort for the project's sessions; absent or blank means
       * the agent's own default. */
    claude_session_model?: string;
    claude_session_effort?: string;
    codex_session_model?: string;
    codex_session_effort?: string;
    grok_session_model?: string;
    grok_session_effort?: string;
    /** Whether an absent `default_agent` is detected from each directory's marker files; absent
       * means true. Sent false by a client that has already shown the detection to the user. */
    detect_default_agent?: boolean;
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
    /** Each agent's account: absent leaves it alone; an explicit `null` puts it back to
       * inheriting the console's reference. */
    claude_account_id?: ProjectAccount | null;
    codex_account_id?: ProjectAccount | null;
    grok_account_id?: ProjectAccount | null;
    /** Each agent's model and effort for the project's sessions: absent leaves it alone; an
       * explicit `null` (or a blank string) clears it back to the agent's own default. */
    claude_session_model?: string | null;
    claude_session_effort?: string | null;
    codex_session_model?: string | null;
    codex_session_effort?: string | null;
    grok_session_model?: string | null;
    grok_session_effort?: string | null;
    /** Absent leaves the project's pin alone. */
    pinned?: boolean;
    /** Absent leaves the project's tags alone; a present array replaces them wholesale. */
    tags?: string[];
  }
  /** `stop_sessions` ends the project's running sessions first (as `archive_session` does) instead
   * of being refused with `project_has_running_sessions`; default false. `trash_directory` moves
   * the project's folder to the system Trash first, and keeps the project when that fails or
   * the folder is shared with another project; default false. */
  | { type: "delete_project"; project: string; stop_sessions?: boolean; trash_directory?: boolean }
  | { type: "list_dir"; path: string }
  /** Answered with `agent_detected`, or an error when `path` is not an existing directory. */
  | { type: "detect_directory_agent"; path: string }
  /** Answered with `agent_detected`, or `git_remote_unreachable` when the remote cannot be read. */
  | { type: "probe_git_remote"; remote_url: string }
  /** Answered with `agent_models`. `account` is an account id of `agent`, absent for the default
   * account; one that does not exist or belongs to another agent is refused with
   * `unknown_account`. Anything else that goes wrong is an empty list with a reason, not an error. */
  | { type: "list_agent_models"; agent: Agent; account?: string }
  /** Answered with `configuration_exported`: the document of the console `console` names, or, with
   * none, of every console and the application settings. Refused with `unknown_console` for a
   * console that does not exist. */
  | { type: "export_configuration"; console?: string }
  /** Answered with `configuration_inspected`: what importing `document` would meet on this machine.
   * Changes nothing. The document is untrusted input; refused with `configuration_not_recognised`,
   * `configuration_version_unsupported` or `configuration_unreadable`. */
  | { type: "inspect_configuration"; document: unknown }
  /** Answered with `configuration_imported` once the last project is placed, which can take as
   * long as every clone together. `document` is the one that was inspected, unchanged. A project
   * with no resolution is left out. Refused before anything is created with the codes of
   * `inspect_configuration`, `import_resolution_invalid` and `import_settings_missing`. */
  | { type: "import_configuration"; document: unknown; resolutions?: ProjectResolution[]; apply_settings?: boolean }
  /** Reads the next automatic title without reserving it; answered with `console_session_title`.
   * Refused with `unknown_console` when the console does not exist. */
  | { type: "preview_console_session_title"; console: string }
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
    /** Absent inherits the project's or console's setting; null uses the agent's own default.
       * Values are saved on this session and reused when it resumes. */
    model?: string | null;
    effort?: string | null;
    /** The console session this (project) session should report to. Absent means none: a
       * session the user opens by hand stays outside the orchestration unless they choose one.
       * Ignored for the console session itself, which is never bound. */
    bound_to?: string;
    /** The agent's own id of a conversation started outside Octoboard, which this project session
       * continues. Only a project session takes one, never together with `bound_to`. */
    resume_agent_session_id?: string;
  }
  | { type: "resume_session"; session: string }
  | { type: "archive_session"; session: string }
  /** Moves the session to another account of its own agent: its process ends, its conversation is
   * copied into that account's directory, and it is relaunched there. `account` is the target by
   * id, or `null` for the default account; it is always sent. Answered once the relaunched session
   * has stayed up, or with the reason it did not. */
  | { type: "switch_session_account"; session: string; account: string | null }
  /** Removes Octoboard's record of one session: an archived one, or one that has never been used
   * (no turn, bound to no owner, none bound to it), whose process is stopped; refused with
   * `session_not_deletable` otherwise, or while it is starting. The agent's own records of the
   * session (and of the archived sessions deleted with it) are moved to the Trash only with
   * `trash_agent_records`; default false. `only_if_unused_without_agent_records` rejects unless
   * the session remains unused and a fresh native-file inspection proves absence; guarded deletion
   * never moves native files to the Trash. */
  | { type: "delete_session"; session: string; trash_agent_records?: boolean; only_if_unused_without_agent_records?: boolean }
  /** Inspects the agent-native files of the entire supplied deletion scope; reads no transcripts. */
  | { type: "inspect_session_records"; sessions: string[] }
  /** With `project`: every archived session of that project. With `console_session`: every
   * archived session under it (bound to it, or to an archived lead session bound to it). With
   * neither: every archived console session of the console. Naming both is refused. Each removal
   * is broadcast as `session_deleted`. */
  | { type: "delete_archived_sessions"; console: string; project?: string; console_session?: string; trash_agent_records?: boolean }
  | { type: "set_session_pinned"; session: string; pinned: boolean }
  /** Refused for a project session. */
  | { type: "set_session_colour"; session: string; colour: ConsoleSessionColour }
  | { type: "send_message"; session: string; text: string }
  | { type: "rename_session"; session: string; title: string }
  | { type: "list_pages"; console_session: string }
  /** What a report panel form was submitted with. The page is named rather than the console
   * session: it is what the panel knows, and it names the console session the submission goes to.
   * Only that console session's newest page may be submitted from. */
  | { type: "submit_page"; page: string; data: unknown }
  /** The user's go-ahead to a `trust_prompt`: Octoboard may press that session's trust
   * confirmation. `remember` also records the project's consent, so a later confirmation of any
   * agent in the project is pressed without a prompt; `trust_parent_dir` records the project's
   * parent directory as trusted instead (the daemon derives it; it is the prompt's `trust_dir`),
   * covering every project under it. Either is recorded only once the confirmation was pressed. */
  | { type: "confirm_trust"; session: string; remember: boolean; trust_parent_dir?: boolean }
  /** Stops trusting a directory; projects' own consents are untouched. */
  | { type: "remove_trusted_directory"; path: string }
  /** The user's answer to a `console_session_request`: `approve` starts a console session in the
   * requesting session's console and binds that session to it; a refusal, dismissing the dialog
   * included, changes nothing. The first answer from any client wins; a later one is refused with
   * `CONSOLE_REQUEST_ANSWERED`, or `CONSOLE_REQUEST_NOT_WAITING` when the session stopped
   * waiting. An approval is answered with `session_opened` for the console session it started, a
   * refusal with `ack`. */
  | { type: "answer_console_session_request"; request_id: string; approve: boolean }
  /** Each settable field is optional: absent means leave it as it is. Answered with `ack`;
   * broadcasts `settings_updated` only when something actually changed (the trusted-folders
   * pattern). */
  | { type: "update_settings"; auto_pull_repositories?: boolean; default_clone_dir?: string }
  /** Answered with `ack` at once; the statuses arrive as `project_git_status` broadcasts as each
   * project finishes. Every project of `console` is checked, concurrently. A project whose check
   * is already in flight is not started again, so a client polling faster than the checks finish,
   * or several clients watching the same console, cannot pile work up. */
  | { type: "refresh_git_status"; console: string }
  /** Checks one project against its remote and fast-forwards it right now, ignoring the
   * one-minute floor and the `auto_pull_repositories` setting. Answered with `ack`; the status
   * follows as `project_git_status` broadcasts. */
  | { type: "pull_project_branch"; project: string }
  /** Opens the system terminal at the project's directory, on the daemon's host. Answered with
   * `ack` once the terminal has been asked to open. */
  | { type: "open_project_terminal"; project: string }
  /** Opens the project's directory in the system file manager on the daemon's host. Answered
   * with `ack` once `open` succeeds on macOS, or `xdg-open` is spawned on Linux; later Linux
   * launcher failures are not observed. */
  | { type: "open_project_directory"; project: string }
  /** Pushes the project's current branch and only that branch. Answered with `ack` once `git push`
   * has exited; a success is followed by `project_git_status` from a local re-read. */
  | { type: "push_project_branch"; project: string }
  /** Switches to an existing local branch named in canonical wire form; never fetches or pulls. */
  | { type: "switch_project_branch"; project: string; branch: string }
  /** Restores staged and unstaged tracked changes to HEAD in the project worktree, preserving
   * untracked and ignored files. Answered with `ack` after the restore finishes. */
  | { type: "restore_project_changes"; project: string }
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
   * `project_source`, `project_dir`, `project_file_matches`, `project_file`, `project_changes`, `project_change`,
   * `project_branches`, `project_comparison`, `project_comparison_change`, `project_change_bodies`
   * and `project_comparison_change_bodies`. A
   * newer browse request on this connection with the same `slot` supersedes an older one still
   * outstanding, which is answered `request_superseded`; one that finished first still gets its
   * real reply, so a reply is also discarded by its `id`. */
  | { type: "get_project_source"; project: string; slot?: string }
  | { type: "search_project_files"; project: string; query: string; slot?: string }
  | { type: "list_project_dir"; project: string; path: string; worktree?: string; slot?: string }
  | {
    type: "read_project_file";
    project: string;
    path: string;
    worktree?: string;
    from?: ReadFrom;
    slot?: string;
  }
  | { type: "list_project_changes"; project: string; worktree?: string; slot?: string }
  | {
    type: "read_project_change";
    project: string;
    worktree?: string;
    change: { group: ChangeGroup; old: SideRef; new: SideRef };
    slot?: string;
  }
  | { type: "list_project_branches"; project: string; slot?: string }
  /** Each a branch name as a wire path: the changes are what `right` has changed since it forked
   * from `left` (`git diff left...right`). Two branches with no commit in common are refused
   * (`no_merge_base`, or `no_merge_base_shallow` in a shallow clone). */
  | { type: "compare_project_branches"; project: string; left: string; right: string; slot?: string }
  | {
    type: "read_project_comparison_change";
    project: string;
    left: ComparisonEndpoint;
    right: ComparisonEndpoint;
    base: string;
    change: { old: SideRef; new: SideRef };
    slot?: string;
  }
  /** The whole bodies of a change `read_project_change` made a patch of, for expanding the lines
   * the patch collapses; `old` and `new` are the versions that reply reported for its sides. */
  | {
    type: "read_project_change_bodies";
    project: string;
    worktree?: string;
    change: { group: ChangeGroup; old: SideRef; new: SideRef };
    old: SideVersion;
    new: SideVersion;
    slot?: string;
  }
  /** As `read_project_change_bodies`, for a change of a comparison. */
  | {
    type: "read_project_comparison_change_bodies";
    project: string;
    left: ComparisonEndpoint;
    right: ComparisonEndpoint;
    base: string;
    change: { old: SideRef; new: SideRef };
    old: SideVersion;
    new: SideVersion;
    slot?: string;
  }
  | { type: "shutdown" };

/** A client request as sent on the wire: the body's fields plus an optional correlation id. */
export type Request = RequestBody & { id?: string };

export type Event =
  | { type: "session_records_inspected"; id?: string; presence: SessionRecordsPresence }
  | {
    type: "snapshot";
    hosts: Host[];
    consoles: Console[];
    projects: Project[];
    sessions: Session[];
    /** Directories whose projects Octoboard presses every agent's trust confirmation for. */
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
  | { type: "console_session_title"; id?: string; console: string; title: string }
  /** The reply to `open_session`, and to an approving `answer_console_session_request`: the session
   * that was started. The same record is also broadcast as `session_upserted`, but that broadcast
   * carries no request id, so this is the only way the caller can tell which session in the tree
   * is the one it just opened. */
  | { type: "session_opened"; id?: string; session: Session }
  /** A session of a project the user has not agreed Octoboard may press trust confirmations for is
   * at its agent's folder-trust confirmation, which asks whether `path` is trusted; `agent` is the
   * one asking. Broadcast once per confirmation, and sent again to a client right after each
   * `snapshot` (connect or lag recovery) for every one still waiting, so a client that missed it is
   * still asked; a repeat is ignored. */
  | {
    type: "trust_prompt";
    session: string;
    agent: Agent;
    project: string;
    path: string;
    /** The directory `trust_parent_dir` would trust, or null when there is none to offer (too broad
       * to trust, the home directory cannot be determined, or `path` is not absolute): the choice
       * is offered only when it is not null. */
    trust_dir: string | null;
  }
  /** An unbound project session asked for a console session (`request_console_session`) and its
   * call is waiting for the user's answer. Broadcast once, and sent again right after each
   * `snapshot`, oldest first, for every request still waiting; a repeat is ignored. `request`,
   * `summary` and `open_items` are the session's own words. */
  | {
    type: "console_session_request";
    request_id: string;
    session: string;
    console: string;
    project: string | null;
    request: string;
    summary: string;
    open_items: string[];
    /** Milliseconds since the epoch; several requests are shown oldest first. */
    requested_at: number;
  }
  /** A `console_session_request` stopped waiting — answered, timed out or withdrawn — and every
   * client closes its dialog. */
  | {
    type: "console_session_request_closed";
    request_id: string;
    reason: "approved" | "refused" | "timed_out" | "withdrawn";
  }
  | { type: "dir_listing"; id?: string; path: string; entries: DirEntry[] }
  /** The reply to `detect_directory_agent` and `probe_git_remote`: the one agent the project is set
   * up for, or null for none, several, or one known not to be installed. */
  | { type: "agent_detected"; id?: string; agent: Agent | null }
  /** The reply to `list_agent_models`; `agent` and `account` echo the request. `models` is empty
   * exactly when `unavailable` says why, and a form then falls back to the agent's own default
   * rather than failing. */
  | {
    type: "agent_models";
    id?: string;
    agent: Agent;
    account: string | null;
    models: AgentModel[];
    unavailable: ModelsUnavailable | null;
  }
  /** The reply to `export_configuration`: the document to save as a file. */
  | { type: "configuration_exported"; id?: string; document: ConfigurationDocument }
  /** The reply to `inspect_configuration`: one entry per console of the document, in its order,
   * and `settings` when the document carries any. */
  | { type: "configuration_inspected"; id?: string; settings?: InspectedSettings | null; consoles: InspectedConsole[] }
  /** The reply to `import_configuration`: one entry per console of the document, in its order.
   * A console or project that went wrong is reported in it rather than failing the request. */
  | {
    type: "configuration_imported";
    id?: string;
    settings_applied?: DocumentSettings | null;
    settings_error?: ImportFailure | null;
    consoles: ImportedConsole[];
  }
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
  /** Recursive filename matches in canonical wire path form; incomplete when bounded or unreadable. */
  | {
    type: "project_file_matches";
    id?: string;
    project: string;
    query: string;
    paths: string[];
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
  /** `head` is the commit the staged changes are against, null before the first commit;
   * `complete` is false when the list was cut at a budget. */
  | {
    type: "project_changes";
    id?: string;
    project: string;
    worktree: string | null;
    head: string | null;
    changes: ChangeEntry[];
    complete: boolean;
  }
  /** `head` is the commit a staged change was read against (null before the first commit, and for
   * the other groups); `patch` is the change's unified patch as `git` writes it, null when none is
   * made (an untracked file, a side outside the project). */
  | {
    type: "project_change";
    id?: string;
    project: string;
    worktree: string | null;
    group: ChangeGroup;
    head: string | null;
    old: SideRead;
    new: SideRead;
    patch: FileContent | null;
  }
  /** In byte order of the names; `complete` is false when the list was cut at a budget. */
  | { type: "project_branches"; id?: string; project: string; branches: BranchInfo[]; complete: boolean }
  /** `left` and `right` are the branches as asked for and the commits they were resolved to, and
   * `base` the commit they forked at (their merge base); every change in `changes` (all
   * `committed`) is between exactly `base` and `right`'s commit. */
  | {
    type: "project_comparison";
    id?: string;
    project: string;
    left: ComparisonEndpoint;
    right: ComparisonEndpoint;
    base: string;
    changes: ChangeEntry[];
    complete: boolean;
  }
  /** `left`, `right` and `base` echo the request: the old side was read from `base` and the new
   * side from `right`'s commit. */
  | {
    type: "project_comparison_change";
    id?: string;
    project: string;
    left: ComparisonEndpoint;
    right: ComparisonEndpoint;
    base: string;
    old: SideRead;
    new: SideRead;
    patch: FileContent | null;
  }
  /** Both sides of the change with their whole bodies in `file`, each present or absent. */
  | {
    type: "project_change_bodies";
    id?: string;
    project: string;
    worktree: string | null;
    group: ChangeGroup;
    old: SideRead;
    new: SideRead;
  }
  /** `left`, `right` and `base` echo the request. */
  | {
    type: "project_comparison_change_bodies";
    id?: string;
    project: string;
    left: ComparisonEndpoint;
    right: ComparisonEndpoint;
    base: string;
    old: SideRead;
    new: SideRead;
  }
  | { type: "ack"; id?: string }
  /** A failure, worded from `code` and `params` (see `daemonMessage.ts`); `message` is the English
   * text, shown for a code the client does not know. A client also branches on some codes — see
   * `ALREADY_RUNNING_CODES`, `TRUST_REFUSED_CODES`, `TRUST_NOT_WAITING`,
   * `CONSOLE_REQUEST_NOT_WAITING`, `CONSOLE_REQUEST_ANSWERED`, `REQUEST_SUPERSEDED`,
   * `SOURCE_CHANGED` and `WORKTREE_UNAVAILABLE`. */
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

/** The go-ahead was for a trust confirmation no longer waiting: nothing is wrong, so nothing is
 * shown. */
export const TRUST_NOT_WAITING = "trust_not_waiting";

/** An approval of a request for a console session whose session is no longer waiting for it
 * (withdrawn, or timed out): nothing was started, which the user is told in a toast. */
export const CONSOLE_REQUEST_NOT_WAITING = "console_request_not_waiting";

/** An answer to a request for a console session already answered, here or from another client:
 * nothing is wrong, so nothing is shown. */
export const CONSOLE_REQUEST_ANSWERED = "console_request_answered";

/** A browse request given up for a newer one in its slot, as the client asked: nothing is shown. */
export const REQUEST_SUPERSEDED = "request_superseded";

/** What was being read changed while it was read: reading it again gets a consistent copy. */
export const SOURCE_CHANGED = "source_changed";

/** The worktree a browse request named is gone: nothing is read from another one in its place. */
export const WORKTREE_UNAVAILABLE = "worktree_unavailable";

/** Client-sent text frame on `/ws/term/:session`. Binary frames on that socket are raw PTY input. */
export type TermControl = { type: "resize"; cols: number; rows: number };
