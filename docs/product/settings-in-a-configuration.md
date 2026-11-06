# Settings in a configuration

Which of Octoboard's settings a configuration document carries, in which of its regions, and what an import does with
them. The document, its regions and the import as a whole are in `docs/product/configuration-export-and-import.md`,
whose terms this doc uses; how the export dialog and the import's review and result present the settings is in
`docs/product/configuration-export-and-import-dialogs.md`.

## Which settings travel

The **application settings** region carries every setting of Settings' **General**, **Git**, **Agent behavior** and
**Keyboard shortcuts** sections (see `docs/product/settings.md`). The **accounts** region carries every setting of its
**Agent accounts** section: the accounts themselves (see "The accounts" in
`docs/product/configuration-export-and-import.md`) and the settings that apply to all of an agent's accounts or to
every agent (see "The agent account settings an export carries" below). The one setting of those sections left out is
an account's sharing of the default configuration (see "What is not configuration and never travels" in
`docs/product/configuration-export-and-import.md`). Nothing of the Trusted folders, Permissions and Notifications
sections travels, nor does a choice a client keeps elsewhere, such as the update channel (see "The Updates and Update
channel rows" in `docs/product/in-app-updates.md`) or a project's focus mode (see `docs/product/focus-mode.md`).

## The application settings an export carries

Each under its own key, grouped here as Settings groups them. The import review notes each switch, and the line for
turning one on says what then follows, in that setting's own words, as given below. A settings key the importing
Octoboard does not know, such as `reply_in_interface_language`, is ignored. The settings only a client holds are in a
document only when a client saved it (see "The settings a client holds" below); every other one is in every document
that has application settings.

**General**

- **Appearance**, under `appearance`: `light`, `dark` or `system` (see `docs/product/appearance.md`).
- **Language**, under `interface_language`: the tag of the language the exporting client shows (see
  `docs/product/language.md`). The client writes it in place of the language the daemon holds; a document the daemon
  builds for another program carries the daemon's held language, when it holds one (see "The language the daemon
  holds" in `docs/product/language.md`).
- **Markdown style**, under `markdown_style`: `tailwind`, `github`, `newsprint` or `latex` (see
  `docs/product/markdown-documents.md`).
- **Order the sidebar by status** (see that section in `docs/product/settings.md`), under `sidebar_order_by_status`.
  The review's line for turning it on says that sessions and projects in the sidebar then move as their status changes.
- **Automatically delete archived sessions whose conversation is gone** (see "Deleting archived sessions whose
  conversation is gone" in `docs/product/deleting-sessions.md`), under `auto_delete_unresumable_archives`.
- **Close idle terminals when closing the panel** (see that section in `docs/product/settings.md`), under
  `auto_close_idle_shell_terminals`. The review's line for turning it on says that closing the Terminal panel closes
  idle terminals and keeps terminals with running programs.
- **Keep running in the background**, under `run_in_background`, `true` or `false`, carried only by a client that has
  the setting — the macOS application (see "Closing the window" in `docs/product/application-lifecycle.md`).

**Git**

- **Default clone directory** (see that section in `docs/product/settings.md`), under `default_clone_dir`: as `~/`
  followed by its path under the exporting machine's home directory when it lies there, and as the absolute path
  otherwise, as an account's directory is written (see "The accounts" in
  `docs/product/configuration-export-and-import.md`). It is written whatever the choice about project paths.
- **Automatically pull repositories** (see "Automatically pulling repositories" in
  `docs/product/project-git-status.md`), under `auto_pull_repositories`.
- **The application-wide git workflow settings** (see "The five settings" in `docs/product/git-workflow-settings.md`),
  under `git_workflow`, an object holding all five of `development`, `squash_before_delivery`, `rebase_before_delivery`,
  `delivery` and `revert_gate`. A settings region without `git_workflow`, with `null` there, or with an object that
  leaves out any of the five cannot be read, and the import is refused (see "What is refused, and what is reported
  instead" in `docs/product/configuration-export-and-import.md`). Applying the document's settings sets all five to the
  document's values. The import review and its result each have one line for all five (see "The import review" in
  `docs/product/configuration-export-and-import-dialogs.md` and "The import result" in
  `docs/product/configuration-import-progress-and-result.md`).

**Agent behavior**

- **Automatically approve session requests** (see "Approving requests automatically" in
  `docs/product/session-requests.md`), under `auto_approve_session_requests`. The review's line for turning it on says
  that a session's request to start a session that reports to the user is then approved without asking first.
- **Automatically approve resume requests** (see the same section), under `auto_approve_resume_requests`. The review's
  line for turning it on says that a session's request to reopen or resume a session, or to resume a conversation in a
  new session, is then approved without asking first.
- **Automatically approve account switches** (see the same section), under `auto_approve_account_switches`. The review's
  line for turning it on says that a session's request to switch the account of one of its sessions is then approved
  without asking first.
- **Automatically approve handoffs** (see the same section), under `auto_approve_handoffs`. The review's line for
  turning it on says that a session's request to hand one of its sessions off to another agent is then approved without
  asking first.
- **Ask questions as choices** (see that section in `docs/product/settings.md`), under `ask_questions_as_choices`. The
  review's line for turning it on says that Octoboard then asks sessions started afterwards to put a question that needs
  the user's decision as options to pick from.

**Keyboard shortcuts**

- **The keyboard shortcuts of the exporting client**, under `keyboard_shortcuts`: only the shortcuts it has changed
  from their defaults, and `{}` when it has changed none. Their form is in "The shortcuts as data" in
  `docs/product/keyboard-shortcuts.md`. A document without them leaves the importing client's shortcuts as they are
  when its settings are applied; one with `{}` puts them back to their defaults.

A document whose export did not include the application settings carries none of them, whichever consoles it holds.

## The agent account settings an export carries

Under `agent_settings` in the accounts region, in every document that has the accounts region, whichever consoles it
holds:

- **Automatically select an account** (see that section in `docs/product/settings.md`), under `auto_select_account`.
  The review's line for turning it on says that a new session then uses the account whose weekly limit resets soonest.
- **Automatically switch accounts at the usage limit** (see that section in `docs/product/settings.md`), under
  `auto_switch_on_usage_limit`. The review's line for turning it on says that a session that stops at its account's
  usage limit is then switched to another account with usage left and carries on.
- **Start usage windows automatically** (see that section in `docs/product/settings.md`), under
  `auto_start_quota_windows`. The review's line for turning it on says that a usage window that has not started
  counting is then started with one small request, which counts a little toward the account's usage limits.
- **Provider settings** (see "Provider settings" in `docs/product/per-agent-settings.md`), under `provider_settings`,
  an object keyed by agent, each holding an object keyed by setting id with the user's explicit choice: `true` or
  `false` for a switch, the choice's text for a choice setting. Only the settings their provider declares as carried
  are in it — today every provider setting: Claude Code's and Codex's Permission approval and Codex's and Grok Build's
  Claude compatibility, `{"claude": {"approval_mode": "bypass"}, "codex": {"claude_compatibility": true}}` say — and of
  those only the ones the user made a choice for: a setting never chosen for is absent, so on the importing machine it
  keeps following that machine's default — an unset Codex Claude compatibility there follows whether Claude Code is
  added there, an unset Permission approval follows each account's own configuration — and an agent with nothing
  carried is absent, an export where no choice was made carrying `{}`. An entry naming an agent or a setting this build
  does not declare as carried, or a value the setting does not take, is skipped rather than refused, in the inspection
  as in the apply, so a hand-edited document cannot set a provider setting the build does not carry.
- **Agent tags** (see "Agent tags" in `docs/product/per-agent-settings.md`), under `agent_tags`, an object keyed by
  agent (`claude`, `codex`, `grok`) holding each agent's tags as a list. It carries every agent that has tags, a removed
  agent's included; an agent without tags is absent, and an export where no agent has tags carries `{}`. An agent this
  build does not know is skipped rather than refused.

## The settings a client holds

The appearance, the Markdown style, keeping running in the background and the keyboard shortcuts are each client's
own, kept in that client rather than by the daemon, and the language a client shows is its own too. So the client adds
them to the document it saves whenever that document carries the application settings — the appearance, the Markdown
style, the language it shows, its changed shortcuts and, where it has the setting, keeping running in the background —
and the daemon builds none of them but the language and reads none of them. A document with application settings
asked of the daemon by any other program therefore has none of them but the daemon's held language.

On import they are applied by the client that runs the import, to itself alone, and only once the daemon reports the
application settings applied, from what the daemon passes back as applied; when the settings could not be applied,
neither are these. Other clients — another browser, or the desktop app — keep their own. A value the importing client
does not know, or a setting its platform does not have (keeping running in the background, in a browser), is ignored
and leaves the client's own as it is.

## Application settings are left alone unless they are asked for

A document's application settings are applied **only when the import is explicitly asked to apply them**, and are not
applied by default, since applying them changes how the daemon's host behaves rather than only adding to what it
holds: automatically pulling repositories makes the daemon fast-forward the user's own repositories by itself,
automatically deleting archived sessions makes it delete sessions by itself, the four automatic approvals let agents'
requests, account switches and handoffs through without asking the user, asking questions as choices changes what
sessions are told at launch, and the default clone directory decides where later clones go. The inspection says both
what the document sets and what is set now, so what applying would change can be shown before anything is done.
Asking to apply the settings of a document that carries none is refused rather than quietly ignored, so a request
never does less than it was told to.

The default clone directory is applied first, before anything is created (see "The default clone directory on import"
below); the rest of the settings are applied last, after the consoles and projects, through the same path as a change
made in Settings, and the result of the import reports what was applied. So:

- an import that turns automatic pulling on runs the same immediate fast-forward pass turning it on by hand does (see
  "Turning the switch on" in `docs/product/project-git-status.md`);
- one that turns the automatic deletion on deletes nothing until the next scheduled check, as the switch does;
- one that changes closing idle terminals with the panel closes nothing at once and applies to later panel closes;
- one that turns an automatic approval on or off changes no request already waiting;
- one that turns asking questions as choices on or off changes no session already running;
- one that changes the language sets the language the daemon holds for the user, when this build offers the
  document's language; a language it does not offer is skipped, and the rest of the settings still apply. A language
  applied also switches the client that runs the import to it, as that client applies the other settings it holds (see
  "The settings a client holds" above); other windows keep showing their own.

### The default clone directory on import

The inspection gives the document's default clone directory as it comes to on the daemon's host (`~/` expanded) beside
the host's own. One that is neither absolute nor starts with `~/` is left out of the import rather than refusing the
settings. An import that applies the settings can be told to **keep the host's own** instead; then the document's is
not applied, and everything else of the settings is.

Applied, the document's directory is written **before any account, console or project is created**, so a project the
same import places under the default clone directory or clones (see "Where an imported project goes" in
`docs/product/configuration-export-and-import.md`) goes under the directory in effect after that choice. The inspection
therefore answers each project's offers under both directories, so the review can follow the choice. A directory that
cannot be written is a failure of the application settings as a whole: the rest of them are not applied. Once written,
it stays written when the rest of the settings then cannot be applied, and the result says it was set.

### The keyboard shortcuts on import

The keyboard shortcuts follow the same choice, and are applied by the client that runs the import as the other
settings it holds are (see "The settings a client holds" above). They **replace** that client's shortcuts: every
command the document does not name goes back to its default. The document's shortcuts are judged as one set, under the
same rules as a shortcut the user records, and an entry that cannot be used on the importing client is dropped and
counted (see "The shortcuts as data" in `docs/product/keyboard-shortcuts.md`).

## The agent account settings are applied with the accounts

The agent account settings are applied **whenever the accounts are imported**, whatever the import says about the
application settings, and right after the accounts — after the agents are added and the accounts created, before any
console is (see "Importing accounts" in `docs/product/configuration-export-and-import.md`) — through the same path as a
change made in Settings. The inspection says what the document sets and what is set now, for the provider settings the
value in effect here of every setting a document can carry (`null` for a choice setting left unset), so what applying
would change can be shown first. Applying records each provider choice the document carries as the importing machine's
own, setting by setting, and leaves a setting it does not carry as it is; it replaces the tags of each agent the
document lists with the document's, normalised as the Tags row's are, and leaves an agent it does not list as it is.

- An import that turns automatic account selection on or off changes no session already open.
- One that turns automatic switching at the usage limit on or off changes no session.
- One that turns the starting of usage windows on looks over the quota readings already held at once (see "When an
  account is kicked" in `docs/product/starting-usage-windows.md`).
- One that changes an agent's Claude compatibility or Permission approval changes no session already running; the
  value is read when a session next starts or resumes. A Permission approval of **Approve everything** is applied
  without the confirmation the Permission approval row asks for (see "Approval modes" in
  `docs/product/per-agent-settings.md`); the review names the choice before anything is done.

Agent account settings that could not be applied are reported in the accounts' result, and the import stands. A
document of the first version carried these settings among its application settings; reading it moves them to its
accounts region, so they too are applied with the accounts (see "Versions" in
`docs/product/configuration-export-and-import.md`).
