//! The registry of the codes the daemon sends: [`Code`] and the code modules that declare every
//! one of them. See "Coded messages" in `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use crate::providers::ProviderCode;

/// A code the daemon names a failure or a notice by on the wire. Only this module can make one, so
/// every code the daemon sends is a `pub const` of one of the code modules below ([`error_code`],
/// [`models_unavailable_code`], [`quota_unavailable_code`], [`login_failure_code`],
/// [`agent_cli_code`], [`notice_code`]): together they are the registry of the codes a client has
/// to word. A UI test (`packages/ui/src/daemonCodes.test.ts`) reads them from this file and fails
/// for a code the UI has no words or classification for, so each is a `pub const` of its own, of
/// type `Code`, made from a string literal, and a code is never made anywhere else in this file.
/// The one exception is a code only one provider raises, which its descriptor declares
/// ([`Code::of_provider`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code(&'static str);

impl Code {
    /// The wire code of a code a provider declares (`crate::providers::ProviderCode`). The only
    /// way to make a `Code` that is not a `pub const` below.
    pub const fn of_provider(code: ProviderCode) -> Self {
        Self(code.as_str())
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Serialize for Code {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl PartialEq<&str> for Code {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Code> for &str {
    fn eq(&self, other: &Code) -> bool {
        *self == other.0
    }
}

impl PartialEq<Code> for String {
    fn eq(&self, other: &Code) -> bool {
        self == other.0
    }
}

/// Error codes carried by `Event::Error`; `PROTOCOL.md` lists each with its params. A code never
/// changes its meaning or its params, because a client words the failure from them. See [`Code`]
/// for what a new code needs in the UI.
pub mod error_code {
    use super::Code;

    // -- what a client acts on, rather than just shows ---------------------------------------

    /// A request asked a provider for something it does not offer: `params` names the `agent` (its
    /// wire id, unlike every other code's `agent`, its label) and the `operation`, one of
    /// `sign_in`, `kick`, `reset_credit`, `config_sharing` and `cli_update`.
    pub const PROVIDER_UNSUPPORTED: Code = Code("provider_unsupported");
    /// A launch was asked for while one was already running or already starting for that session.
    /// The client's own double click is the ordinary cause, so it is shown as nothing at all.
    pub const SESSION_ALREADY_RUNNING: Code = Code("session_already_running");
    /// [`SESSION_ALREADY_RUNNING`], where the launch had begun but not yet registered.
    pub const SESSION_ALREADY_STARTING: Code = Code("session_already_starting");
    /// A directory to trust as a whole cannot be offered: it is the filesystem root, the user's home
    /// directory or one that contains it. Nothing was answered. The dialog the request came from
    /// stays open, because the user can still choose another way to answer, and so it does for the
    /// three codes below, which are the same refusal for other reasons.
    pub const TRUST_DIRECTORY_TOO_BROAD: Code = Code("trust_directory_too_broad");
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the project's path is not absolute.
    pub const TRUST_PATH_NOT_ABSOLUTE: Code = Code("trust_path_not_absolute");
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the home directory is unknown, so nothing can be
    /// checked against it.
    pub const TRUST_HOME_UNKNOWN: Code = Code("trust_home_unknown");
    /// [`TRUST_DIRECTORY_TOO_BROAD`], where the folder above the project at `path` could not be
    /// checked against the home directory in time, as on a mount that does not answer.
    pub const TRUST_DIRECTORY_UNREACHABLE: Code = Code("trust_directory_unreachable");
    /// A go-ahead for a trust confirmation that is no longer waiting for one: pressed already, by
    /// this or another client or in the terminal, gone with its session, or never shown. Nothing is
    /// wrong, so a client shows nothing.
    pub const TRUST_NOT_WAITING: Code = Code("trust_not_waiting");
    /// A write of a record edited through a form (`update_settings`, `update_console`,
    /// `update_project`, `update_account`, or an import's settings) was based on a version of a
    /// field that another write has moved on from, so none of it was applied. `params` names the
    /// `fields` whose versions differ (their paths, comma-separated) and their current `versions`
    /// (a JSON object of path to version), for the client to show the current values and ask again.
    pub const WRITE_CONFLICT: Code = Code("write_conflict");

    // -- for the user to read ----------------------------------------------------------------

    /// Octoboard could not press a trust confirmation it had accepted a go-ahead for, or meant to
    /// press without asking. `params` names the `agent`, and the `reason_code` param says why, one
    /// of [`trust_reason`](crate::protocol::codes::trust_reason).
    pub const TRUST_ANSWER_FAILED: Code = Code("trust_answer_failed");
    /// The confirmation of a provider that records trust per session (`Capabilities::
    /// per_session_trust`, Grok Build's) was accepted — pressed by Octoboard, or answered by the
    /// person in the terminal — but no trust entry for the folder reached the user's own store: the
    /// agent wrote none into the session's copy in time, or the user's store could not be updated.
    /// Nothing was written to the user's store and no permission was recorded. `params` names the
    /// `agent`, the user's store as `path`, the English `reason`, its `reason_code` (one of
    /// [`trust_reason`](crate::protocol::codes::trust_reason)'s carry codes) and, for some, a
    /// `detail`.
    pub const TRUST_NOT_CARRIED_OVER: Code = Code("trust_not_carried_over");
    /// A go-ahead for a console session's trust confirmation, which Octoboard presses without
    /// asking.
    pub const CONSOLE_SESSION_TRUST_NOT_ASKED: Code = Code("console_session_trust_not_asked");

    /// A request that is not valid JSON of a known shape.
    pub const UNREADABLE_REQUEST: Code = Code("unreadable_request");
    /// Anything that is not meant for the user to read in detail: an unexpected failure with no
    /// meaning of its own. The text is carried as the `detail` param.
    pub const INTERNAL_ERROR: Code = Code("internal_error");
    /// SQLite could not write Octoboard's own database, for a cause the user can fix. `params` has
    /// the `cause`: `access` (the data folder or the file is read-only, or access is denied),
    /// `disk_full` (no room to grow the file), or `other` (the file could not be opened for the
    /// write, which a full disk also causes, as SQLite cannot create its journal then); and
    /// SQLite's own text as `detail`.
    pub const STORE_WRITE_FAILED: Code = Code("store_write_failed");
    pub const UNKNOWN_CONSOLE: Code = Code("unknown_console");
    pub const UNKNOWN_PROJECT: Code = Code("unknown_project");
    pub const UNKNOWN_SESSION: Code = Code("unknown_session");
    /// No shell terminal has that id. `params` names it as `shell_terminal`.
    pub const UNKNOWN_SHELL_TERMINAL: Code = Code("unknown_shell_terminal");
    pub const UNKNOWN_PAGE: Code = Code("unknown_page");
    /// A request names two fields that exclude each other. `params` names them as `first` and
    /// `second`, as on the wire.
    pub const CONFLICTING_FIELDS: Code = Code("conflicting_fields");
    /// A request lacks a field its kind needs.
    pub const FIELD_REQUIRED: Code = Code("field_required");
    /// Project text exceeds its character limit. `params` names the `field` and `limit`.
    pub const FIELD_TOO_LONG: Code = Code("field_too_long");
    /// A model or effort the agent does not offer. `params` names the `agent`, the `field`
    /// (`model` or `effort`) and the `value`.
    pub const INVALID_LAUNCH_SETTING: Code = Code("invalid_launch_setting");
    /// A repository URL that a clone is refused from: not one of the transports Octoboard
    /// clones over, or carrying credentials, which would be stored with the project. `params`
    /// names the `url` with any credentials removed.
    pub const INVALID_REMOTE_URL: Code = Code("invalid_remote_url");
    pub const CONSOLE_HAS_RUNNING_SESSIONS: Code = Code("console_has_running_sessions");
    pub const PROJECT_HAS_RUNNING_SESSIONS: Code = Code("project_has_running_sessions");
    /// Another project's directory is the one being moved to the Trash, lies inside it, or holds
    /// it, so trashing would take that project's files too. `params` names the other project as
    /// `project`.
    pub const PROJECT_DIRECTORY_SHARED: Code = Code("project_directory_shared");
    /// The project's directory is one the Trash is never offered for: the home directory, a
    /// root of a disk, or a directory holding Octoboard's own data. The project is kept.
    pub const PROJECT_DIRECTORY_PROTECTED: Code = Code("project_directory_protected");
    /// The project's directory could not be moved to the Trash. The project is kept. The reason
    /// is carried as the `detail` param.
    pub const PROJECT_DIRECTORY_TRASH_FAILED: Code = Code("project_directory_trash_failed");
    /// A `delete_account` with `trash_directory` while a session of the account has a process or is
    /// being launched, resumed or switched: its directory holds what they run on. `params` names
    /// the `account`, its `name` and `sessions`, how many.
    pub const ACCOUNT_HAS_RUNNING_SESSIONS: Code = Code("account_has_running_sessions");
    /// A `delete_account` with `trash_directory` for a directory that is, holds or lies inside an
    /// agent's default configuration directory or another account's directory, or holds a
    /// project's folder. The account is kept.
    /// `params` has `kind` (`account`, `default_account` or `project`), the `name` of that account
    /// or project, and the `agent` of an account or default account.
    pub const ACCOUNT_DIRECTORY_SHARED: Code = Code("account_directory_shared");
    /// A `delete_account` with `trash_directory` for a directory the Trash is never offered for:
    /// the home directory, the root of a disk, or one holding Octoboard's own data. The account is
    /// kept. `params` names the `path`.
    pub const ACCOUNT_DIRECTORY_PROTECTED: Code = Code("account_directory_protected");
    /// A `delete_account` with `trash_directory` whose directory could not be moved to the Trash,
    /// or it, the home directory, Octoboard's data directory, or another account's, an agent's
    /// default or a project's path could not be looked at, or a sign-in could still write into the
    /// directory. The account is kept. `params` has the `path`, the `detail` and, as for
    /// `project_directory_trash_failed`, the `cause` where there is one: `unanswered` and
    /// `unreadable` for the directory itself, the home directory and the data directory,
    /// `other_unlooked` where whether the directory holds or overlaps another account's, an agent's
    /// default or a project's directory could not be told, and `signing_in` where the account's
    /// sign-in did not end within five seconds, or a sign-in of another account pointed at the same
    /// directory is running.
    pub const ACCOUNT_DIRECTORY_TRASH_FAILED: Code = Code("account_directory_trash_failed");
    pub const PATH_NOT_ABSOLUTE: Code = Code("path_not_absolute");
    pub const PATH_NOT_FOUND: Code = Code("path_not_found");
    pub const PATH_NOT_A_DIRECTORY: Code = Code("path_not_a_directory");
    pub const PATH_ALREADY_EXISTS: Code = Code("path_already_exists");
    pub const DIRECTORY_UNREADABLE: Code = Code("directory_unreadable");
    /// A session's or a launch's working directory cannot be reached by the daemon.
    pub const DIRECTORY_UNREACHABLE: Code = Code("directory_unreachable");
    pub const NO_REPOSITORIES_FOUND: Code = Code("no_repositories_found");
    pub const ALL_PROJECTS_ALREADY_ADDED: Code = Code("all_projects_already_added");
    pub const REPOSITORY_NAME_MISSING: Code = Code("repository_name_missing");
    pub const GIT_CLONE_FAILED: Code = Code("git_clone_failed");
    pub const GIT_REMOTE_UNREACHABLE: Code = Code("git_remote_unreachable");
    /// Also used for an account's directory, which goes through the same normalisation minus the
    /// existence check below.
    pub const CONFIG_DIR_NOT_ABSOLUTE: Code = Code("config_dir_not_absolute");
    /// A session's pinned configuration directory has gone, which refuses the launch — narrowed to
    /// a session that has a conversation on the agent's side; see `crate::adapter::pinned_config_dir`.
    pub const CONFIG_DIR_UNREACHABLE: Code = Code("config_dir_unreachable");
    /// The `format` of a configuration document is absent or names something else, so the file is
    /// not one.
    pub const CONFIGURATION_NOT_RECOGNISED: Code = Code("configuration_not_recognised");
    /// A configuration document is of a `version` this daemon does not read. `params` names the
    /// `version` and the one it `supported`.
    pub const CONFIGURATION_VERSION_UNSUPPORTED: Code = Code("configuration_version_unsupported");
    /// A configuration document could not be read. `params` names the `detail`.
    pub const CONFIGURATION_UNREADABLE: Code = Code("configuration_unreadable");
    /// A resolution of `import_configuration` is not one the document can take. `params` names the
    /// `console` and `project` it gave, as indexes, or for an account's resolution the `agent` and
    /// the `account` index within that agent's accounts of the document.
    pub const IMPORT_RESOLUTION_INVALID: Code = Code("import_resolution_invalid");
    /// `import_configuration` was asked to apply a section of the application settings
    /// (`apply_settings`) and the document does not carry it. `params` names the `section`.
    pub const IMPORT_SETTINGS_MISSING: Code = Code("import_settings_missing");
    /// `import_configuration` was asked to apply the accounts' part of an agent (`apply_agents`)
    /// and the document carries nothing for it. `params` names the `agent`.
    pub const IMPORT_AGENT_MISSING: Code = Code("import_agent_missing");
    /// `import_configuration` was asked to create a document's accounts and the document carries
    /// none.
    pub const IMPORT_ACCOUNTS_MISSING: Code = Code("import_accounts_missing");
    /// `export_configuration` was asked for nothing: no console, no application settings and no
    /// accounts.
    pub const EXPORT_EMPTY: Code = Code("export_empty");
    /// A project of an import was left uncreated because the directory choice made for it cannot be
    /// used: the document gives it no usable path or remote URL for that choice. `params` names the
    /// `choice`.
    pub const IMPORT_CHOICE_UNAVAILABLE: Code = Code("import_choice_unavailable");
    pub const ICON_NOT_AN_IMAGE: Code = Code("icon_not_an_image");
    pub const ICON_TOO_LARGE: Code = Code("icon_too_large");
    pub const SESSION_NOT_RUNNING: Code = Code("session_not_running");
    /// A `release_session` for a console session, which is never bound. `params` names the
    /// `session`.
    pub const CONSOLE_SESSION_NEVER_BOUND: Code = Code("console_session_never_bound");
    /// A `release_session` for a session that is not bound to any owner. `params` names the
    /// `session`.
    pub const SESSION_NOT_BOUND: Code = Code("session_not_bound");
    /// A `release_session` by an owner for a session bound to another owner. `params` names the
    /// `session` and the `owner` that asked.
    pub const SESSION_NOT_BOUND_TO_OWNER: Code = Code("session_not_bound_to_owner");
    /// A `release_session` found the session's binding changed between checking it and writing the
    /// release, so nothing was released. `params` names the `session`.
    pub const SESSION_BINDING_CHANGED: Code = Code("session_binding_changed");
    /// A change of a session's owner (a bind, a transfer to another owner, a release) is refused
    /// because the session itself is being started, resumed, handed off, switched to another
    /// account, having its branches transferred or having its owner changed. Nothing was
    /// changed. `params` names the `session`.
    pub const SESSION_CHANGING: Code = Code("session_changing");
    /// A change of a session's owner is refused because the owner it gains or loses the session
    /// for is itself being started, resumed, handed off, switched to another account or having its
    /// owner changed. Nothing was changed. `params` names the `session` and the `owner`.
    pub const SESSION_OWNER_CHANGING: Code = Code("session_owner_changing");
    /// A lock change raced an archive or deletion. The client may retry once it finishes.
    pub const SESSION_LOCK_BUSY: Code = Code("session_lock_busy");
    /// The operation includes a locked session. `params.session` names it.
    pub const SESSION_LOCKED: Code = Code("session_locked");
    /// Only an archived session, or one never used, can be deleted, and not while it is starting: a
    /// session that is not archived is refused once it has had a turn, is bound to an owner or has
    /// sessions bound to it.
    pub const SESSION_NOT_DELETABLE: Code = Code("session_not_deletable");
    /// A session cannot be archived while a session under it (bound to it, or to a coordinator
    /// bound to it) has a process running, whatever its status. `params` names the `count` of
    /// those sessions and their `sessions`, by title, comma separated.
    pub const SESSION_HAS_RUNNING_SESSIONS: Code = Code("session_has_running_sessions");
    pub const SESSION_WAITING_FOR_USER: Code = Code("session_waiting_for_user");
    /// An `interrupt_session` for a session that is being switched to another account, handed
    /// off, having its branches transferred, given another owner, or restarted for a change of its
    /// account's sharing. Nothing was sent. `params` names the `session`.
    pub const SESSION_BUSY: Code = Code("session_busy");
    /// A message the user sent is refused because the session's agent is showing, or may still be
    /// about to show, its folder-trust confirmation, which the message's trailing Enter would
    /// answer. `params` names the `session`.
    pub const SESSION_TRUST_PENDING: Code = Code("session_trust_pending");
    /// A message queued for the session could not be written in full, so it and the queue behind
    /// it were dropped. `params` holds `fragment: "held"` when it was written while a draft was
    /// stashed: only it was dropped, and the session holds Octoboard's later writes until the user
    /// has sent a prompt after it, so nothing is run together with the fragment.
    pub const QUEUED_MESSAGES_LOST: Code = Code("queued_messages_lost");
    /// A message for the session went into its agent's own queue, which did not confirm taking it:
    /// it may already be queued or running, and is not sent again. As the notice of the same code;
    /// `params` names the `agent`.
    pub const MESSAGE_ACCEPTANCE_UNCONFIRMED: Code = Code("message_acceptance_unconfirmed");
    /// A message for the session was refused outright by its agent's own queue, which took
    /// nothing of it: it is not sent by any route, and the messages queued after it go on. As
    /// the notice of the same code; `params` names the `agent` and gives the agent's reason as
    /// `detail`.
    pub const MESSAGE_REFUSED: Code = Code("message_refused");
    pub const PAGE_NOT_CURRENT: Code = Code("page_not_current");
    /// The page's form was already submitted: it takes one submission, whichever window sent it.
    pub const PAGE_ALREADY_SUBMITTED: Code = Code("page_already_submitted");
    pub const BINARY_NOT_FOUND: Code = Code("binary_not_found");
    pub const SHELL_ENVIRONMENT_TIMEOUT: Code = Code("shell_environment_timeout");
    /// A session's resolved agent has been determined unavailable (its binary does not resolve on
    /// the login shell's `PATH`) — never raised while that determination is still pending.
    /// `params` names `agent`.
    pub const AGENT_NOT_AVAILABLE: Code = Code("agent_not_available");
    /// A session cannot be opened on, or moved to, an account whose plan quota is used up: its
    /// last reading has a session, weekly or credit-period window at 100% or more that has not
    /// reset yet. `params` names the `agent`, the `account` (by name) and the `window` (as the
    /// quota shows it), and `resets_at` (milliseconds since the epoch) when the agent said when.
    pub const ACCOUNT_QUOTA_EXHAUSTED: Code = Code("account_quota_exhausted");
    /// A session cannot be opened on, handed off to, or switched to an account the user turned off
    /// (`set_account_disabled`). `params` names the `agent` and the `account` (by name).
    pub const ACCOUNT_DISABLED: Code = Code("account_disabled");
    /// A session cannot be opened on, handed off to, or switched to an account the last quota
    /// reading found not signed in (`quota_not_signed_in`, `quota_config_dir_missing`). A reading
    /// that failed for another reason, a network error say, does not refuse. `params` names the
    /// `agent` and the `account` (by name).
    pub const ACCOUNT_NOT_SIGNED_IN: Code = Code("account_not_signed_in");
    /// A session's resolved agent is not on `Settings.added_agents`, so a new session or a handoff
    /// to it is refused, as is an `update_agent_cli` for it; never raised while that list is
    /// undecided. `params` names `agent`.
    pub const AGENT_NOT_ADDED: Code = Code("agent_not_added");
    /// An `add_agent` or an `update_agent_cli` asked for an agent whose binary does not resolve on
    /// the login shell's `PATH` (for an `add_agent`, one determined not installed). `params` names
    /// `agent`.
    pub const AGENT_NOT_INSTALLED: Code = Code("agent_not_installed");
    /// An `add_agent` asked for an agent that is already added. `params` names `agent`.
    pub const AGENT_ALREADY_ADDED: Code = Code("agent_already_added");
    /// An `add_agent` asked before availability is determined, which would leave the daemon unable
    /// to tell an installed agent from a missing one. Once it is determined it is no longer refused
    /// for this reason; a run whose snapshot failed refuses until the next start. `params` names
    /// `agent`.
    pub const AGENT_AVAILABILITY_PENDING: Code = Code("agent_availability_pending");
    /// A `reorder_accounts` did not name every account of the agent and its default exactly once.
    pub const ACCOUNT_ORDER_INVALID: Code = Code("account_order_invalid");
    /// A `reorder_agents` named agents that are not exactly those on the list, each once. No `params`.
    pub const AGENT_ORDER_INVALID: Code = Code("agent_order_invalid");
    /// A `reorder_agents` or `remove_agent` asked before the list is decided, which waits for availability. No
    /// `params`.
    pub const ADDED_AGENTS_UNDECIDED: Code = Code("added_agents_undecided");
    pub const UNKNOWN_ACCOUNT: Code = Code("unknown_account");
    /// A `list_all_processes` waited for a count another request was computing, it did not finish
    /// in time, and no counts had been noted to answer with. Nothing was counted; the next request
    /// asks again. No `params`.
    pub const PROCESS_COUNT_TIMED_OUT: Code = Code("process_count_timed_out");
    /// A `keep_processes`, or an `archive_session` with `keep_process_ids`, named a tree with a
    /// process attached to its agent's own console, which ends with the session whatever keeps it
    /// (`ProcessTree.unkeepable_reason` `agent_console`). Nothing was kept, and the archive did not
    /// go ahead. `params` names the process's `pid` and `name`.
    pub const PROCESS_SHARES_AGENT_CONSOLE: Code = Code("process_shares_agent_console");
    /// A `use_reset_credit` for an account whose use of a credit is already running. Nothing
    /// was sent. `params` names `agent`.
    pub const RESET_CREDIT_IN_PROGRESS: Code = Code("reset_credit_in_progress");
    /// The provider did not answer a `use_reset_credit` in time. The credit may or may not have
    /// been used; the quota is read again. `params` names `agent`.
    pub const RESET_CREDIT_TIMED_OUT: Code = Code("reset_credit_timed_out");
    /// The provider refused a `use_reset_credit` for want of a sign-in. The quota is read again.
    /// `params` names the `agent` and the `account` (by name).
    pub const RESET_CREDIT_NOT_SIGNED_IN: Code = Code("reset_credit_not_signed_in");
    /// A `use_reset_credit` could not be carried out: the provider's CLI could not be run, ended
    /// without answering, refused the request or answered with something that could not be read.
    /// The quota is read again. `params` names `agent` and `detail`.
    pub const RESET_CREDIT_FAILED: Code = Code("reset_credit_failed");
    /// A `start_account_login` for an account that is being signed in already. `params` names
    /// `agent`.
    pub const LOGIN_ALREADY_RUNNING: Code = Code("login_already_running");
    /// An `update_agent_cli` for an agent the daemon knows no command to update: its CLI was
    /// installed in a way it does not recognise (`install_method` `unknown`), or the tool that
    /// updates a recognised install (`npm`, `brew`) is not on the shell's `PATH`. Nothing was
    /// run. `params` names `agent`.
    pub const AGENT_CLI_UPDATE_UNSUPPORTED: Code = Code("agent_cli_update_unsupported");
    /// An `update_agent_cli` for an agent whose CLI is being updated already. Nothing was run.
    /// `params` names `agent`.
    pub const AGENT_CLI_UPDATE_RUNNING: Code = Code("agent_cli_update_running");
    /// A `check_daemon_update`, `update_daemon` or `restart_daemon` to a daemon that does not
    /// update itself: a build without the feature, a daemon that follows the desktop application,
    /// or a host there is no release for. Nothing was done.
    pub const DAEMON_UPDATE_UNAVAILABLE: Code = Code("daemon_update_unavailable");
    /// An `update_daemon` while an update is being downloaded or installed, by this daemon or by
    /// `octoboardd update`, or a `restart_daemon` while this daemon downloads one. Nothing was
    /// started.
    pub const DAEMON_UPDATE_RUNNING: Code = Code("daemon_update_running");
    /// An `update_daemon` once an update has replaced the executable: the daemon has to restart
    /// before it can update again. Nothing was started.
    pub const DAEMON_UPDATE_INSTALLED: Code = Code("daemon_update_installed");
    /// An `update_daemon` when the last check found no newer release, or none was made. Nothing
    /// was started.
    pub const DAEMON_UPDATE_NOTHING_TO_INSTALL: Code = Code("daemon_update_nothing_to_install");
    /// An `update_daemon` for a daemon whose user may not replace its executable. Nothing was
    /// downloaded. `params` names the `path` of the executable.
    pub const DAEMON_UPDATE_NOT_WRITABLE: Code = Code("daemon_update_not_writable");
    /// A `cancel_account_login` for an account that is not being signed in. `params` names
    /// `agent`.
    pub const LOGIN_NOT_RUNNING: Code = Code("login_not_running");
    /// A `sign_in_account_mcp_server` while a sign-out of the same server of the account is
    /// running, or the reverse. Nothing was run. `params` names `agent` and `server`.
    pub const MCP_SERVER_AUTH_BUSY: Code = Code("mcp_server_auth_busy");
    /// A `cancel_account_mcp_sign_in` for a server that is not being signed in to for the account.
    /// `params` names `agent` and `server`.
    pub const MCP_SIGN_IN_NOT_RUNNING: Code = Code("mcp_sign_in_not_running");
    /// A `set_account_sharing` for the default account, which is the configuration the other
    /// accounts share, or turning sharing on for an account whose directory resolves to the
    /// default account's. `params` names the `account` and its `name` when one was named.
    pub const DEFAULT_ACCOUNT_NOT_SHAREABLE: Code = Code("default_account_not_shareable");
    /// A `set_account_sharing` turning sharing on, on a host that cannot make the file links it
    /// needs (`snapshot.host.file_links` is false). No `params`; nothing was recorded.
    pub const SHARING_UNAVAILABLE_ON_HOST: Code = Code("sharing_unavailable_on_host");
    /// An account's change of sharing did not complete — asked for by `set_account_sharing`, or
    /// turning sharing off before `delete_account` or `update_account` pointing it at another
    /// directory, which it then refuses. Nothing is rolled back: the account is left marked
    /// `share_default_config_incomplete`. `params` names the `account`, its `name`, the `detail`,
    /// `items`: a JSON array of `{item, left, cause, detail, sessions?}`, each item of the
    /// account's directory that was not brought in line, what it was left as (`unchanged`,
    /// `backed_up`, `partly_moved`, `linked`, `absent`) and why, as a `SharingCause` and in the
    /// system's own words; and `cause`, the cause the items share (`other` when they differ), or
    /// why the change could not start when there are none. A `cause` of `default_moved` means the
    /// agent's default directory is no longer the one the links lead into or sharing was started
    /// into: turning sharing off, then on again, shares the new one. An entry about a conversation
    /// turning sharing off could not take back names the agent's id of it as `item` and the
    /// sessions recording it as `sessions`.
    pub const ACCOUNT_SHARING_FAILED: Code = Code("account_sharing_failed");
    /// A `delete_account`, or an `update_account` pointing it at another directory, for an account
    /// that shares the default configuration, or whose last change of sharing is incomplete,
    /// while sessions of it run or start or it is being signed in: sharing has to be turned off
    /// first, and cannot be now. Nothing was changed. `params` names the `account` and its `name`.
    pub const ACCOUNT_STILL_SHARING: Code = Code("account_still_sharing");
    /// A `set_account_sharing` while sessions of the account work under its directory (as
    /// `AccountInUse::sessions` counts them), or it is being signed in: the change would move
    /// files they hold. Also when one of its idle sessions turned working before the change ended
    /// it. Nothing was stopped or changed. `params` names the `account`, its `name`, `sessions`,
    /// how many of its sessions work, and `signing_in`, `true` or `false`.
    pub const ACCOUNT_IN_USE: Code = Code("account_in_use");
    /// A change of the account's sharing is running: creating, launching or resuming a session of
    /// it, switching a session to or from it, a second `set_account_sharing` for it, and removing
    /// or repointing it, are refused until the change ends. `params` names the `account` and its
    /// `name`.
    pub const ACCOUNT_SHARING_IN_PROGRESS: Code = Code("account_sharing_in_progress");
    /// A `start_account_login` for an account whose `delete_account` with `trash_directory` is
    /// running: a sign-in would write into the directory after it was ended and before it moves.
    /// Nothing was started. `params` names the `account` and its `name`.
    pub const ACCOUNT_BEING_REMOVED: Code = Code("account_being_removed");
    /// A `set_account_sharing`, or a `delete_account` or `update_account` repointing an account,
    /// ended one of the account's idle sessions for the change and did not see its process gone
    /// after a generous wait: the change did not start, nothing was recorded, and the sessions it
    /// ended are resumed. `params` names the `account`, its `name` and the `session`.
    pub const SHARING_SESSION_DID_NOT_STOP: Code = Code("sharing_session_did_not_stop");
    /// A switch was asked to move a session to the account it is on already.
    pub const SESSION_ALREADY_ON_ACCOUNT: Code = Code("session_already_on_account");
    /// A switch was asked for an archived session. Reopening one is a resume's job, and a switch
    /// must not do it as a side effect.
    pub const SESSION_ARCHIVED: Code = Code("session_archived");
    /// A handoff, or a transfer of a session's branches, was asked for a session in the middle of a
    /// turn, or still starting. `params` names the `session`.
    pub const SESSION_WORKING: Code = Code("session_working");
    /// A handoff was asked to the agent the session already runs on. `params` names the `session`
    /// and the `agent`.
    pub const HANDOFF_TO_SAME_AGENT: Code = Code("handoff_to_same_agent");
    /// A handoff moved the work to the receiving session, but archiving the original failed.
    /// `params` names the `receiving_session` and nests the archive's own failure as `reason`,
    /// `reason_code` and that failure's params (its own `session` kept), as a `launch_failed` does.
    pub const HANDOFF_ARCHIVE_FAILED: Code = Code("handoff_archive_failed");
    /// A handoff started the receiving session, but moving the place to it failed (the binding, the
    /// owned sessions or the pinned mark), and it is left running. `params` names the
    /// `receiving_session` and nests the failure as `HANDOFF_ARCHIVE_FAILED` does.
    pub const HANDOFF_MOVE_FAILED: Code = Code("handoff_move_failed");
    /// A switch found no conversation record of the session in the account it is on. `params`
    /// names `agent` and the `path` of that account's directory.
    pub const CONVERSATION_NOT_FOUND: Code = Code("conversation_not_found");
    /// An `open_session` asked to resume a conversation found no record of it in the
    /// session's account directory. `params` names `agent`, the `path` of that directory and the
    /// `id` asked for.
    pub const CONVERSATION_TO_RESUME_NOT_FOUND: Code = Code("conversation_to_resume_not_found");
    /// Reopening a session was refused because its agent no longer has the conversation record to
    /// resume, so the launch could only fail. The session is left as it was. `params` names
    /// `agent` and `session`.
    pub const CONVERSATION_UNRESUMABLE: Code = Code("conversation_unresumable");
    /// An `open_session` asked to resume a conversation named an id that is not a UUID, which
    /// every agent's conversation ids are. `params` names the `id`.
    pub const RESUME_ID_INVALID: Code = Code("resume_id_invalid");
    /// An `open_session` asked to resume a conversation for a console session.
    pub const RESUME_FOR_CONSOLE_SESSION: Code = Code("resume_for_console_session");
    /// The conversation an `open_session` asked to resume is already recorded by a session
    /// outside the project (of another project, or a console session). `params` names the `id`.
    pub const CONVERSATION_OUTSIDE_PROJECT: Code = Code("conversation_outside_project");
    /// The conversation an `open_session` asked to resume is already recorded by a session of the
    /// same project, which resuming or reopening continues instead. `params` names the `id` and
    /// that `session`, by id.
    pub const CONVERSATION_IN_SESSION: Code = Code("conversation_in_session");
    /// Copying the conversation into the target account's directory did not complete. `params`
    /// names the target `path` and the `detail`.
    pub const RELOCATION_FAILED: Code = Code("relocation_failed");
    /// A switch ended the session's process and it was still not seen gone after a generous wait,
    /// so the switch stopped there: nothing was copied or recorded, and the process may still be
    /// running. `params` names the `session`.
    pub const SESSION_DID_NOT_STOP: Code = Code("session_did_not_stop");
    /// A switch relaunched the session and its process ended at once, so it is back on the
    /// account it had. `params` names the `session`.
    pub const SWITCH_DID_NOT_COME_UP: Code = Code("switch_did_not_come_up");
    /// A change of a running session's model ended its process to restart it and still did not see
    /// it gone after a generous wait, so the change stopped there: the session's model and effort
    /// were not changed, and the process may still be running. `params` names the `session`.
    pub const MODEL_CHANGE_DID_NOT_STOP: Code = Code("model_change_did_not_stop");
    /// A change of a session's model relaunched it and its process ended at once, so the session
    /// keeps the model and effort it had and is left interrupted. `params` names the `session`.
    pub const MODEL_CHANGE_DID_NOT_COME_UP: Code = Code("model_change_did_not_come_up");
    /// The daemon has begun to shut down, so no session is launched, resumed, switched or handed
    /// off, and no claim on one is granted; in a planned stop, also nothing that would change or
    /// end a terminal or remove what one works in (an archive, a deletion, a removal of a project
    /// or a console, a restart, an explicit stop of processes, a shell terminal created or closed,
    /// an interrupt, a trust answer), once the queued messages are saved for the next daemon no
    /// message, and once a terminal is handed over nothing typed into it, which is dropped (told
    /// once per terminal connection, with no params). The request may be made again once the
    /// daemon is back.
    pub const DAEMON_SHUTTING_DOWN: Code = Code("daemon_shutting_down");
    /// A planned stop or restart was asked for (`planned_stop`, `restart_daemon`) of a daemon a
    /// service manager runs, while a process holder that runs sessions or shell terminals is
    /// inside the daemon's own systemd service, having no unit of its own: the daemon would exit
    /// and the service's stop would end that holder with everything it runs, which a planned stop
    /// promises to keep. Nothing is stopped. Has no params; the ordinary stop (`shutdown`) is not
    /// refused.
    pub const HOLDERS_IN_SERVICE: Code = Code("holders_in_service");
    /// A planned restart was asked for (`restart_daemon`) while an update of an agent's CLI runs,
    /// which the restart would cut off mid-install, possibly leaving that CLI broken. Nothing is
    /// restarted; it can be asked for again once the update has ended. Has no params.
    pub const AGENT_UPDATE_RUNNING: Code = Code("agent_update_running");
    /// A requested account name collides with an existing one of the same agent, trimmed and
    /// compared ignoring letter case — the default account's name takes part. `params` names
    /// `agent` and the `name` of the account it collides with.
    pub const ACCOUNT_NAME_TAKEN: Code = Code("account_name_taken");
    /// A requested account config directory, once normalised, equals that of another account of
    /// the same agent — the default account's resolved directory takes part. `params` names
    /// `agent` and the `name` of the account using it (the default account by its name, which is
    /// [`DEFAULT_ACCOUNT_NAME`](crate::protocol::DEFAULT_ACCOUNT_NAME) unless the user named it).
    pub const CONFIG_DIR_TAKEN: Code = Code("config_dir_taken");
    /// An approval of a `session_request` whose session is no longer waiting for it — the request
    /// was withdrawn or timed out — so its request was not carried out. `params` names the
    /// `session`.
    pub const SESSION_REQUEST_NOT_WAITING: Code = Code("session_request_not_waiting");
    /// An answer to a `session_request` that has already been answered, from this client or
    /// another, or that the daemon does not know. Nothing changed, and nothing is wrong.
    pub const SESSION_REQUEST_ANSWERED: Code = Code("session_request_answered");

    // -- browse requests ---------------------------------------------------------------------

    pub const INVALID_PATH: Code = Code("invalid_path");
    /// The directory a read is scoped to — the project's own, or its scope in another worktree —
    /// does not exist or is not a directory.
    pub const SOURCE_UNAVAILABLE: Code = Code("source_unavailable");
    pub const FILE_NOT_FOUND: Code = Code("file_not_found");
    pub const PERMISSION_DENIED: Code = Code("permission_denied");
    pub const OUTSIDE_SCOPE: Code = Code("outside_scope");
    /// `open_path_link` found a path it will not open from a link, or `reveal_path_link` one it will not show, or either could not look at it; its `cause` says why, and
    /// `detail` carries the system's text for an `unreadable` one.
    pub const PATH_LINK_REFUSED: Code = Code("path_link_refused");
    pub const UNSUPPORTED_FILE_TYPE: Code = Code("unsupported_file_type");
    /// What was being read changed while it was read; reading again gets a consistent copy.
    pub const SOURCE_CHANGED: Code = Code("source_changed");
    pub const LIMIT_EXCEEDED: Code = Code("limit_exceeded");
    pub const NOT_A_GIT_REPOSITORY: Code = Code("not_a_git_repository");
    pub const GIT_UNAVAILABLE: Code = Code("git_unavailable");
    pub const WORKTREE_UNAVAILABLE: Code = Code("worktree_unavailable");
    pub const INVALID_BRANCH_NAME: Code = Code("invalid_branch_name");
    pub const UNKNOWN_BRANCH: Code = Code("unknown_branch");
    pub const INVALID_COMMIT: Code = Code("invalid_commit");
    pub const UNKNOWN_COMMIT: Code = Code("unknown_commit");
    /// A comparison of two branches whose histories share no commit, so there is no point they
    /// forked at to compare from. `params` names `left` and `right`, the two branches.
    pub const NO_MERGE_BASE: Code = Code("no_merge_base");
    /// As [`NO_MERGE_BASE`], in a shallow clone: the history that would show a common commit may
    /// simply not be there.
    pub const NO_MERGE_BASE_SHALLOW: Code = Code("no_merge_base_shallow");
    pub const GIT_FAILED: Code = Code("git_failed");
    /// The system terminal could not be opened at a project's directory. `params` names `detail`.
    pub const TERMINAL_UNAVAILABLE: Code = Code("terminal_unavailable");
    /// A terminal's connection ended before input someone may have typed was written into it.
    /// `params` carries the `detail` of the write that failed, when one did.
    pub const TERMINAL_INPUT_LOST: Code = Code("terminal_input_lost");
    /// The system file manager could not open a project directory, or show a path for `reveal_path_link`. `params` names `detail`.
    pub const DIRECTORY_UNAVAILABLE: Code = Code("directory_unavailable");
    pub const PATH_OPEN_FAILED: Code = Code("path_open_failed");
    /// A push was asked for while `HEAD` is detached. Nothing was pushed.
    pub const NOT_ON_A_BRANCH: Code = Code("not_on_a_branch");
    /// A push was asked for on a branch that has no commit yet. Nothing was pushed.
    pub const NO_COMMITS_TO_PUSH: Code = Code("no_commits_to_push");
    /// A branch with no upstream, and neither `origin` nor exactly one remote to send it to.
    /// Nothing was pushed.
    pub const NO_PUSH_REMOTE: Code = Code("no_push_remote");
    /// The branch's upstream does not name a remote of this repository. Nothing was pushed.
    pub const UPSTREAM_NOT_A_REMOTE: Code = Code("upstream_not_a_remote");
    /// The project already has a push in flight, which also runs the repository's `pre-push`
    /// hook; a second one is refused rather than started alongside. Nothing was pushed.
    pub const PUSH_IN_PROGRESS: Code = Code("push_in_progress");
    /// `git push` failed, or the branch name cannot be placed in a refspec. `params` names
    /// `detail`, `git`'s own message when it ran.
    pub const GIT_PUSH_FAILED: Code = Code("git_push_failed");
    /// Switching to a local branch failed. `params` names `detail`, Git's message when it ran.
    pub const GIT_SWITCH_FAILED: Code = Code("git_switch_failed");
    pub const RESTORE_NOT_ON_BRANCH: Code = Code("restore_not_on_branch");
    pub const NO_COMMITS_TO_RESTORE: Code = Code("no_commits_to_restore");
    pub const GIT_OPERATION_IN_PROGRESS: Code = Code("git_operation_in_progress");
    /// Restoring tracked changes failed. `params` names `detail`, Git's message when it ran.
    pub const GIT_RESTORE_FAILED: Code = Code("git_restore_failed");
    /// Broadcast after an archive, a deletion or a project's removal the user asked for, when a
    /// branch or worktree it was to remove is still there. The operation itself stands. `params`
    /// names `count` and `items`, a JSON list with one `{kind, name, branch, state, uncommitted,
    /// detail}` per item left.
    pub const SESSION_BRANCHES_NOT_REMOVED: Code = Code("session_branches_not_removed");
    /// Broadcast after a project's removal with its folder moved to the Trash, when what goes along
    /// with the folder could not be determined: the worktrees outside it were left, and the
    /// branches too if the repository outlives the folder (otherwise they went with it).
    /// The removal itself stands. `params` names the removed project's `name`, as it is no longer
    /// on record, and `detail`, the reason.
    pub const PROJECT_BRANCHES_NOT_DETERMINED: Code = Code("project_branches_not_determined");
    /// A `read_project_change` whose change names no side its group can read.
    pub const INVALID_CHANGE: Code = Code("invalid_change");
    /// A body asked of a side on the disk whose path has a clean filter in effect: the patch was
    /// made from that filter's output, which cannot be read without writing an object. `params`
    /// names the `path` and the `filter` driver.
    pub const CLEAN_FILTERED: Code = Code("clean_filtered");
    /// A browse request was given up because a newer one took its slot. A client shows nothing.
    pub const REQUEST_SUPERSEDED: Code = Code("request_superseded");
}

/// Codes carried by `ModelsUnavailable`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod models_unavailable_code {
    use super::Code;

    /// The agent's binary does not resolve on the login shell's `PATH`. `params` names `binary`.
    pub const BINARY_NOT_FOUND: Code = Code("models_binary_not_found");
    /// The login-shell snapshot the command runs in could not be taken. `params` names `detail`.
    pub const SHELL_ENVIRONMENT_FAILED: Code = Code("models_shell_environment_failed");
    /// The command did not finish in time. `params` names `agent`.
    pub const TIMED_OUT: Code = Code("models_timed_out");
    /// The command could not be run, or exited non-zero. `params` names `agent` and `detail`.
    pub const COMMAND_FAILED: Code = Code("models_command_failed");
    /// The command succeeded but what it printed holds no model that could be read. `params`
    /// names `agent`.
    pub const UNREADABLE_OUTPUT: Code = Code("models_unreadable_output");
}

/// Codes carried by `QuotaUnavailable`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod quota_unavailable_code {
    use super::Code;

    /// The agent's binary does not resolve on the login shell's `PATH`. `params` names `binary`.
    pub const BINARY_NOT_FOUND: Code = Code("quota_binary_not_found");
    /// The login-shell snapshot the command runs in could not be taken. `params` names `detail`.
    pub const SHELL_ENVIRONMENT_FAILED: Code = Code("quota_shell_environment_failed");
    /// The command did not reply in time. `params` names `agent`.
    pub const TIMED_OUT: Code = Code("quota_timed_out");
    /// The command could not be run, ended without replying, or the reply was an error. `params`
    /// names `agent` and `detail`.
    pub const COMMAND_FAILED: Code = Code("quota_command_failed");
    /// The command replied but the reply is not in a shape that can be read. `params` names
    /// `agent`.
    pub const UNREADABLE_REPLY: Code = Code("quota_unreadable_reply");
    /// The command replied and the account has no quota windows (an API key rather than a
    /// subscription, say). `params` names `agent`.
    pub const NO_WINDOWS: Code = Code("quota_no_windows");
    /// The account's configuration directory does not exist (yet), so there is nothing to read;
    /// nothing was run. `params` names `agent` and `dir`. A state, not a failure: logging the
    /// account in creates the directory.
    pub const CONFIG_DIR_MISSING: Code = Code("quota_config_dir_missing");
    /// The account's directory exists but the agent is not signed in there. `params` names `agent`.
    /// A state, not a failure.
    pub const NOT_SIGNED_IN: Code = Code("quota_not_signed_in");
}

/// Codes carried by `LoginFailure`; `PROTOCOL.md` lists each with its params. Like an error code,
/// each needs a `daemon.<code>` message in the UI's catalogs.
pub mod login_failure_code {
    use super::Code;

    /// The CLI could not be run, ended without signing in, or reported an error. `params` names
    /// `agent` and `detail`.
    pub const FAILED: Code = Code("login_failed");
    /// The CLI reported success but the agent still says it is not signed in. `params` names
    /// `agent`.
    pub const NOT_CONFIRMED: Code = Code("login_not_confirmed");
}

/// Codes carried by an `AgentCliStatus` (`latest_error`) and an `AgentCliUpdate` (`error`);
/// `PROTOCOL.md` lists each. Shown where the status is, so each needs only a `daemon.<code>` message
/// in the UI's catalogs, not a remedy.
pub mod agent_cli_code {
    use super::Code;

    /// Looking up the latest version failed: the registry or the CLI's own check could not be
    /// reached, did not answer in time or reported an error. Carried with the English text as the
    /// status's `latest_detail`.
    pub const LATEST_FAILED: Code = Code("agent_cli_latest_failed");
    /// The lookup answered but what it said holds no version that could be read.
    pub const LATEST_UNREADABLE: Code = Code("agent_cli_latest_unreadable");
    /// The update command could not be run, ended with a failure or printed an error; the last line
    /// it printed is the update's `detail`.
    pub const UPDATE_FAILED: Code = Code("agent_cli_update_failed");
    /// The update command did not finish in time and was stopped. It may have changed the install
    /// before that.
    pub const UPDATE_TIMED_OUT: Code = Code("agent_cli_update_timed_out");
}

/// Codes carried by a `DaemonUpdateCheck` and a `DaemonUpdateInstall` (`error`); `PROTOCOL.md` lists
/// each. Shown where the status is, so each needs only a `daemon.<code>` message in the UI's
/// catalogs, not a remedy.
pub mod daemon_update_code {
    use super::Code;

    /// The check could not read the release manifest: the host could not be reached, answered with
    /// a status other than "not found", sent something that is not a manifest, or has no release
    /// for this machine. Carried with the English text as the check's `detail`.
    pub const CHECK_FAILED: Code = Code("daemon_update_check_failed");
    /// The archive could not be downloaded or written to the disk. The executable was not touched.
    pub const DOWNLOAD_FAILED: Code = Code("daemon_update_download_failed");
    /// The archive is not what the release published: its SHA-256 or its signature does not
    /// check, it names another file, or it does not hold the daemon. The executable was not
    /// touched.
    pub const UNVERIFIED: Code = Code("daemon_update_unverified");
    /// The new executable could not be put in place. The one that runs, and the file at its path,
    /// are as they were.
    pub const REPLACE_FAILED: Code = Code("daemon_update_replace_failed");
}

/// The `reason_code` param of a failed press of a trust confirmation, in the `error` and in the
/// `session_notice` alike; `PROTOCOL.md` lists each. A client that does not know one shows the
/// English `reason` instead. A new reason also needs a `daemon.trust_reason.<code>` message in the
/// UI's catalogs.
pub mod trust_reason {
    pub const SCREEN_GONE: &str = "screen_gone";
    pub const CURSOR_NOT_AT_START: &str = "cursor_not_at_start";
    pub const CURSOR_DID_NOT_MOVE: &str = "cursor_did_not_move";
    pub const TERMINAL_NOT_SETTLED: &str = "terminal_not_settled";
    pub const CURSOR_MOVED_AWAY: &str = "cursor_moved_away";
    pub const SCREEN_NOT_DISMISSED: &str = "screen_not_dismissed";
    pub const SCREEN_REDRAWN: &str = "screen_redrawn";
    pub const INPUT_TOUCHED: &str = "input_touched";
    pub const TERMINAL_WRITE_FAILED: &str = "terminal_write_failed";
    // Of `trust_not_carried_over`: why the trust entry a provider recorded per session did not
    // reach the user's store.
    pub const NOT_RECORDED: &str = "not_recorded";
    pub const STORE_LOCKED: &str = "store_locked";
    pub const STORE_UNREADABLE: &str = "store_unreadable";
    pub const STORE_IO_FAILED: &str = "store_io_failed";
}

/// Codes carried by `Event::SessionNotice`; `PROTOCOL.md` lists each with its params. Like an error
/// code, each needs a `daemon.<code>` message in the UI's catalogs.
pub mod notice_code {
    use super::Code;

    pub const LAUNCH_FAILED: Code = Code("launch_failed");
    pub const QUEUED_MESSAGES_DROPPED: Code = Code("queued_messages_dropped");
    pub const HANDOFF_MESSAGES_DROPPED: Code = Code("handoff_messages_dropped");
    pub const SHARING_MESSAGES_DROPPED: Code = Code("sharing_messages_dropped");
    pub const TRUST_ANSWER_FAILED: Code = Code("trust_answer_failed");
    pub const TRUST_NOT_CARRIED_OVER: Code = Code("trust_not_carried_over");
    /// A message written into the session was not sent by its agent and is waiting in the input
    /// box; nothing more is written into the session until a prompt is sent. `params` names the
    /// `agent`.
    pub const MESSAGE_NOT_SENT: Code = Code("message_not_sent");
    /// The session's agent did not confirm accepting a message written into its own queue. The
    /// message may already be queued, and is not sent again. `params` names the `agent`.
    pub const MESSAGE_ACCEPTANCE_UNCONFIRMED: Code = Code("message_acceptance_unconfirmed");
    /// The session's agent refused a message written into its own queue outright: it was not
    /// sent, and later messages go on. `params` names the `agent` and gives its reason as
    /// `detail`.
    pub const MESSAGE_REFUSED: Code = Code("message_refused");
    /// A message written into the session was thrown away by a dialog its agent raised as the
    /// message went in; it is back at the front of the queue and is written again once the dialog
    /// has been answered. `params` names the `agent`.
    pub const MESSAGE_DISCARDED_BY_DIALOG: Code = Code("message_discarded_by_dialog");
    /// Octoboard switched the session to another account of its agent by itself, because its turn
    /// stopped on the usage limit of the account it ran on. `params` names the `agent`, the account
    /// it was `from`, the `account` it is on now and the `session`.
    pub const ACCOUNT_SWITCHED_AUTOMATICALLY: Code = Code("account_switched_automatically");
    /// Octoboard tried to switch the session to another account by itself, the switch failed after
    /// the session's process was ended, and bringing it back up on the account it had failed too,
    /// so it is left interrupted there. `params` are those of `account_switched_automatically`
    /// (`agent`, `from`, `account`, `session`, `from_id?`, `account_id?`, naming the account it was
    /// on and the one it was to be switched to), with why it could not be brought back up beside
    /// them as a `launch_failed` carries its own failure: `reason`, `reason_code` and the failure's
    /// own params.
    pub const ACCOUNT_SWITCH_AUTOMATICALLY_FAILED: Code =
        Code("account_switch_automatically_failed");
    /// A message written into the session went to a view of its agent's background agents (a
    /// subagent, or a new background session) rather than the conversation, and is not written
    /// again. `params` names the `agent`.
    pub const MESSAGE_DIVERTED_BY_VIEW: Code = Code("message_diverted_by_view");
    /// A message Octoboard wrote into the session was complete but the agent did not confirm sending
    /// it, and the one retry of its submit key failed to write. The message is not written again;
    /// the input box may still hold it. `params` names the `agent` and the failed write's English
    /// text as `detail`.
    pub const SUBMIT_RETRY_FAILED: Code = Code("submit_retry_failed");
    /// Before the session launched, some of its sharing account's links to the default
    /// configuration could not be repaired; the session launched all the same. `params` names
    /// the `account`, its `name`, `cause` and `items`, as `account_sharing_failed`'s.
    pub const ACCOUNT_SHARING_REPAIR_FAILED: Code = Code("account_sharing_repair_failed");
    /// The session was idle, so a change of its account's sharing ended its process, and it was not
    /// resumed afterwards: resuming it failed and it is left interrupted, to be resumed by hand, or
    /// (`still_running` `true`) its process was never seen to end and it was not relaunched.
    /// `params` names the `account`, its `name`, the `detail` and `still_running`, `true` or
    /// `false`.
    pub const SHARING_RESUME_FAILED: Code = Code("sharing_resume_failed");
    /// The session's sharing account holds a thread registry database the shared configuration
    /// does not cover (for Codex, a newer schema generation's), so its threads are kept apart from
    /// the other accounts'. `params` names the `account`, its `name` and the database's `path`.
    pub const UNSHARED_STATE_DATABASE: Code = Code("unshared_state_database");
    /// The session's agent did not confirm, within the protection window, stashing the draft in its
    /// input box before a message was written, so the message went into the input as it was and
    /// may have been sent with the draft. `params` names the `agent`.
    pub const DRAFT_SAVE_UNCONFIRMED: Code = Code("draft_save_unconfirmed");
    /// A message went in while the draft was stashed, but the agent did not confirm restoring the
    /// draft in time: keys typed meanwhile are released into the input as it is, and the message is
    /// not sent again. `params` names the `agent`.
    pub const DRAFT_RESTORE_UNCONFIRMED: Code = Code("draft_restore_unconfirmed");
    /// Keys typed while a draft was stashed have not all been replayed yet: a dialog is up or the
    /// terminal is not taking them. They are replayed once it does. `params` names the `agent`.
    pub const TYPING_HELD: Code = Code("typing_held");
    /// Only part of a message written while a draft was stashed reached the input. It is not sent
    /// again. Keys typed while it was written are dropped, unreplayed, when the user next types, and
    /// their keys go straight in from then on; Octoboard writes nothing more into the session, and
    /// keeps what is queued behind the message, until a prompt is submitted after that. `params`
    /// names the `agent`.
    pub const TYPING_BLOCKED: Code = Code("typing_blocked");
    /// The session's process ended before the draft's restoration was confirmed, so the keys typed
    /// meanwhile are not replayed into a resumed session. `params` names the `agent` and the
    /// `bytes` held back.
    pub const TYPING_LOST: Code = Code("typing_lost");
}
