CREATE TABLE hosts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    ssh_config  TEXT
);
CREATE TABLE accounts (
    id         TEXT PRIMARY KEY,
    agent      TEXT NOT NULL,
    name       TEXT NOT NULL,
    config_dir TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- Whether the account reads the default account's configuration, and where that
    -- stands: a change that did not finish reads as incomplete, for the user to retry.
    share_default_config           INTEGER NOT NULL DEFAULT 0,
    share_default_config_incomplete INTEGER NOT NULL DEFAULT 0,
    share_default_config_completed INTEGER NOT NULL DEFAULT 0,
    share_default_config_backup    TEXT,
    share_default_config_dir       TEXT,
    -- 1 while the account is switched off.
    disabled                       INTEGER NOT NULL DEFAULT 0
);
-- Backstop for the rule that an account's name is required and unique within its
-- agent: `coordinator` checks this before every write that can affect a name, but
-- the invariant belongs on the table it protects, not only on the callers that happen
-- to remember to check it. `LOWER` matches `accounts::account_name_key`'s
-- case-insensitive comparison; it is ASCII-only in SQLite, which is a narrower fold
-- than `str::to_lowercase`, so this index is a backstop and not a full substitute for
-- that check. A write that trips it surfaces through `Store::account_write_error` as
-- the protocol's own `account_name_taken` rather than a raw SQLite error.
CREATE UNIQUE INDEX accounts_agent_name_unique
    ON accounts (agent, LOWER(name));
CREATE TABLE consoles (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    workdir       TEXT NOT NULL,
    default_agent TEXT NOT NULL,
    -- The agent a project session here falls back to when its project names none;
    -- NULL follows `default_agent`.
    project_session_agent TEXT,
    icon          TEXT,
    -- The user's own addition to the console session's instruction file; NULL means
    -- none.
    console_session_prompt TEXT,
    -- The ordinal a console session created in this console is given next, one past
    -- the highest ever used here. Kept on the console rather than derived from the
    -- sessions in it, because a console session that is later deleted must not free its
    -- number for reuse (see `Store::insert_console_session`).
    next_console_session_ordinal INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL
);
CREATE TABLE projects (
    id            TEXT PRIMARY KEY,
    console_id    TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    host_id       TEXT NOT NULL,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    default_agent TEXT,
    source        TEXT NOT NULL,
    remote_url    TEXT,
    trust_consent INTEGER NOT NULL DEFAULT 0,
    pinned        INTEGER NOT NULL DEFAULT 0,
    tags          TEXT NOT NULL DEFAULT '[]',
    description   TEXT,
    -- NULL follows the application-wide value; see `Store::git_workflow_settings`.
    development         TEXT,
    squash_before_delivery INTEGER,
    rebase_before_delivery INTEGER,
    delivery            TEXT,
    revert_gate         INTEGER
);
-- A path is one project of its console. `coordinator` checks this before it adds a project,
-- by what the path is on the file system; the index is the backstop for two requests racing
-- past that check, and compares the stored text. Its leading column also serves the lookup
-- of a console's projects.
CREATE UNIQUE INDEX projects_console_path_unique
    ON projects (console_id, path);
-- The account each provider's sessions in a console read, by id; a provider with no row
-- uses its default account. No `REFERENCES` on `account_id`: `Store::delete_account` removes
-- these rows itself, in the same locked step as the account's removal, rather than relying on
-- a cascade. A row exists only while it names an account. Rows naming a provider this build
-- does not register are kept and ignored.
CREATE TABLE console_providers (
    console_id TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    provider   TEXT NOT NULL,
    account_id TEXT NOT NULL,
    PRIMARY KEY (console_id, provider)
);
-- The model and reasoning effort sessions in a console are launched with, per provider because
-- each has its own model names; NULL means the provider's own default. `session_kind` says which
-- sessions the pair launches: `console` for a console session of the console, `project` for a
-- project session there when neither it nor its project names one. Read at launch, so nothing
-- is copied onto the session. A row exists only while it names a model or an effort.
CREATE TABLE console_launches (
    console_id   TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    provider     TEXT NOT NULL,
    session_kind TEXT NOT NULL CHECK (session_kind IN ('console', 'project')),
    model        TEXT,
    effort       TEXT,
    PRIMARY KEY (console_id, provider, session_kind)
);
-- What a project sets for each provider; a provider with no row inherits everything from its
-- console. `account_id` is the account the provider's sessions here read, by id; NULL with
-- `account_default` clear inherits the console's reference. The flag, set only where the id is
-- NULL, says the project pins the provider's default account instead, even where the console
-- names an account: a separate column so that no stored value in the id column can be mistaken
-- for an account id. The id is cleared by `Store::delete_account`, like the console's; the flag
-- names no account. `model` and `effort` are what a session here is launched with; NULL means
-- the provider's own default. Read at launch, so nothing is copied onto the session. A row
-- exists only while it holds a value; rows naming a provider this build does not register are
-- kept and ignored.
CREATE TABLE project_providers (
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    account_id      TEXT,
    account_default INTEGER NOT NULL DEFAULT 0,
    model           TEXT,
    effort          TEXT,
    PRIMARY KEY (project_id, provider)
);
CREATE TABLE sessions (
    id               TEXT PRIMARY KEY,
    agent            TEXT NOT NULL,
    agent_session_id TEXT,
    console_id       TEXT NOT NULL REFERENCES consoles(id) ON DELETE CASCADE,
    project_id       TEXT REFERENCES projects(id) ON DELETE CASCADE,
    host_id          TEXT NOT NULL,
    kind             TEXT NOT NULL,
    origin           TEXT NOT NULL,
    title            TEXT NOT NULL,
    terminal_title   TEXT,
    -- What the agent's own records call the session: a title it was named with, and a
    -- stand-in from its first prompt.
    record_title     TEXT,
    record_prompt    TEXT,
    context          TEXT,
    -- The model and reasoning effort the agent's records show the session running with.
    observed_model   TEXT,
    observed_effort  TEXT,
    -- The old-record model boundary of the last restart, private to the collector.
    observed_boundary TEXT,
    status           TEXT NOT NULL,
    has_conversation INTEGER NOT NULL DEFAULT 0,
    -- 1 while the session's last turn ended abnormally: cancelled, declined, stopped on a
    -- usage limit, failed, or cut short by its process ending mid-turn. Cleared by a
    -- status off the prompt and by a turn that ends normally (see `Session::paused`).
    last_turn_abnormal INTEGER NOT NULL DEFAULT 0,
    -- The console session this (project) session reports to, or NULL outside the
    -- orchestration. Always NULL for a console session itself. No `REFERENCES`, because
    -- what follows a console session is decided by the coordinator, not by the
    -- database: archiving one archives its interrupted bound sessions, and deleting an
    -- archived one deletes its archived bound sessions. An `ON DELETE CASCADE` would be
    -- wrong twice over: it would take a bound session that is not archived, and it
    -- would remove rows behind the daemon's back, so no `session_deleted` would be
    -- broadcast and every client would keep a ghost row.
    bound_to         TEXT,
    -- `coordinator` or `worker` (see `Session::role`). Written with `bound_to`: a console session,
    -- a session with no parent and one opened under a console session are always coordinators.
    role             TEXT NOT NULL,
    -- Above 0 while the session has a change of its owner it was not running to be
    -- told of (one count per change): its next launch tells it, from the binding as
    -- it then stands, and clears what it saw.
    owner_notice_pending INTEGER NOT NULL DEFAULT 0,
    -- The parting note of the owner that released the session while it was not running,
    -- told with the notice while the session is still unbound; NULL for none. Written
    -- with the release, or with the mark a failed write to the running session left,
    -- and cleared by the next change of owner and by the delivery.
    owner_notice_note TEXT,
    -- Set only for a console session (`kind = 'console'`); NULL for a project session.
    colour           TEXT,
    ordinal          INTEGER,
    -- The account this session's own agent reads, by id; NULL means the default
    -- account. Written with `config_dir` below, which is that account's directory at
    -- the time, when the session is opened and when its account is switched — see
    -- `Session::account_id`.
    account_id       TEXT,
    config_dir       TEXT,
    -- A clear override flag inherits the project's or console's current setting.
    -- A set flag with a NULL value explicitly leaves the choice to the agent.
    model            TEXT,
    effort           TEXT,
    model_override   INTEGER NOT NULL DEFAULT 0,
    effort_override  INTEGER NOT NULL DEFAULT 0,
    pinned           INTEGER NOT NULL DEFAULT 0,
    locked           INTEGER NOT NULL DEFAULT 0,
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER ,
    owner_notice_former_owner TEXT);
-- `owner_notice_former_owner` is the id of the owner that released the session while it was
-- not running, named in the notice with the parting note; NULL for none. Written and cleared
-- with `owner_notice_note`. It is the last column, spelled as a migration leaves it.
-- The lookups by the columns that other rows and the daemon's queries hold sessions by.
CREATE INDEX sessions_console_id ON sessions (console_id);
CREATE INDEX sessions_project_id ON sessions (project_id);
CREATE INDEX sessions_bound_to ON sessions (bound_to);
CREATE INDEX sessions_agent_session_id ON sessions (agent_session_id);
CREATE TRIGGER protect_locked_session_delete
BEFORE DELETE ON sessions WHEN OLD.locked = 1
BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be deleted'); END;
CREATE TRIGGER protect_locked_session_archive
BEFORE UPDATE OF status ON sessions
WHEN OLD.locked = 1 AND NEW.status = 'archived' AND OLD.status != 'archived'
BEGIN SELECT RAISE(ABORT, 'locked sessions cannot be archived'); END;
CREATE TABLE trusted_directories (
    path TEXT PRIMARY KEY
);
CREATE TABLE pages (
    id         TEXT PRIMARY KEY,
    -- The session that pushed the page, a console session or a project session that
    -- faces the user; deleting it takes its pages, and deleting a console or a project
    -- therefore takes all of theirs.
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    html       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX pages_session_id ON pages (session_id);
-- Messages for a session that was interrupted (shared information, and the reports and
-- notices for an owner), held to be written into it once it is resumed
-- (`Store::add_kept_message`), oldest first by `id`. Deleting the session takes its rows.
CREATE TABLE kept_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    text       TEXT NOT NULL
);
CREATE INDEX kept_messages_session_id ON kept_messages (session_id);
-- The session working on a branch, keyed on the branch because one session works on
-- many and a branch has one responsible session. It starts as the session whose own git
-- reported creating the branch and moves only by a transfer. No `REFERENCES` on
-- `responsible_session_id`: a project's deletion cascades its sessions away and the
-- record outlives them, which is what `responsible_title`, the title the session had
-- when it became responsible, is for. `responsible_since` is when that was, in
-- milliseconds. `repository` is the repository's canonical common directory, so a row
-- belongs to the repository and not to any project.
-- `birth_commit` and `birth_time` are the commit and the second the branch was created
-- at, which tell a branch deleted and made again under the same name from the one the
-- row was made for; `created_at` is that time in milliseconds, or when the creation was
-- reported.
-- `target_branch` is the branch the branch was created from, which the revert gate
-- holds its commits against, together with `birth_commit` as the fork point; NULL for
-- one created from something that is no branch.
CREATE TABLE branch_origins (
    repository     TEXT NOT NULL,
    branch         TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    birth_commit   TEXT NOT NULL,
    birth_time     INTEGER NOT NULL,
    responsible_session_id TEXT NOT NULL,
    responsible_since      INTEGER NOT NULL,
    responsible_title      TEXT NOT NULL,
    target_branch          TEXT,
    PRIMARY KEY (repository, branch)
);
CREATE TABLE settings (
    id                     TEXT PRIMARY KEY,
    auto_pull_repositories INTEGER NOT NULL DEFAULT 0,
    auto_close_idle_shell_terminals INTEGER NOT NULL DEFAULT 0,
    auto_delete_unresumable_archives INTEGER NOT NULL DEFAULT 1,
    auto_approve_session_requests INTEGER NOT NULL DEFAULT 0,
    auto_approve_resume_requests INTEGER NOT NULL DEFAULT 0,
    auto_approve_account_switches INTEGER NOT NULL DEFAULT 0,
    auto_approve_handoffs INTEGER NOT NULL DEFAULT 0,
    ask_questions_as_choices INTEGER NOT NULL DEFAULT 1,
    auto_select_account INTEGER NOT NULL DEFAULT 0,
    auto_switch_on_usage_limit INTEGER NOT NULL DEFAULT 0,
    sidebar_order_by_status INTEGER NOT NULL DEFAULT 1,
    auto_start_quota_windows INTEGER NOT NULL DEFAULT 0,
    -- NULL until the user sets one; see `Store::default_clone_dir`.
    default_clone_dir      TEXT,
    -- NULL until a client reports the language the user picked; see
    -- `Store::interface_language`.
    interface_language     TEXT,
    -- NULL until the list is first decided; see `Store::added_agents`.
    added_agents           TEXT,
    account_order          TEXT,
    -- NULL until the user names a default account; see `Store::default_account_names`.
    default_account_names  TEXT,
    -- NULL until a default account is turned off; see `Store::disabled_default_accounts`.
    disabled_default_accounts TEXT,
    -- NULL until the user puts a tag on an agent; see `Store::agent_tags`.
    agent_tags             TEXT,
    development         TEXT NOT NULL DEFAULT 'worktree',
    squash_before_delivery INTEGER NOT NULL DEFAULT 0,
    rebase_before_delivery INTEGER NOT NULL DEFAULT 1,
    delivery            TEXT NOT NULL DEFAULT 'ask',
    revert_gate         INTEGER NOT NULL DEFAULT 0
);
-- The user's explicit choice for a setting of a provider, as JSON (`true`, `false` or a quoted
-- choice); a setting with no row follows its default. Rows naming a provider or setting this
-- build does not declare are kept and ignored. See `Store::provider_settings`.
CREATE TABLE provider_settings (
    provider TEXT NOT NULL,
    setting  TEXT NOT NULL,
    value    TEXT NOT NULL,
    PRIMARY KEY (provider, setting)
);
