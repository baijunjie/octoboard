# Agent behavior settings

Settings' **Agent behavior** section (see "Agent behavior" in `docs/product/settings.md`): what a session may do
without asking the user first, and how it asks the user. A paragraph under the heading says as much, above six rows:
**Automatically approve session requests**, **Automatically approve resume requests**, **Automatically approve account
switches**, **Automatically approve handoffs**, **Ask questions as choices**, then **Keep sessions to their own
project**. The section is the same in every
client. Each switch is kept by the daemon, so every window shows the same, and besides its row only an import asked to
apply a configuration document's Agent behavior section changes it (see "The application settings an export carries"
in `docs/product/settings-in-a-configuration.md`); turning one on or off leaves the others as they are. How the
switches save is in "How switches save" in `docs/product/settings.md`. What an automatic approval does, what turning
one of the four approval switches on changes, and why agents cannot change any of them are in "Approving requests
automatically" in `docs/product/session-requests.md`.

## Automatically approve session requests

A switch, **off to begin with**: whether a session's `request_session` call, its request for a session that is
nobody's, is approved without asking the user. While the switch is off the row's description says what turning it on
will do, and while it is on it says what is happening now: a session can start new sessions without asking first.

## Automatically approve resume requests

A switch, **off to begin with**: whether a session's `request_resume_session` call, its request to resume a
conversation, is approved without asking the user (see `docs/product/requests-to-resume-a-conversation.md`). While the
switch is off the row's description says what turning it on will do, and while it is on it says what is happening now:
a session can reopen archived sessions or resume interrupted sessions and existing conversations without asking first.

## Automatically approve account switches

A switch, **off to begin with**: whether an owner's `switch_session_account` call, which switches the account of one of
its own sessions, is made without asking the user (see "The user's approval of an owner's switch" in
`docs/product/switching-session-accounts.md`). While the switch is off the row's description says that, once it is on,
a session can do so without asking first; while it is on, it says that a session can do so without asking first. The
Switch account the user makes from the window is not affected, and neither is Octoboard's own switch at the usage limit
(see "Automatically switch accounts at the usage limit" in `docs/product/automatic-account-switch.md`).

## Automatically approve handoffs

A switch, **off to begin with**: whether an owner's `handoff_session` call, which hands one of its own sessions off to a
session of another agent, is made without asking the user (see "The user's approval of an owner's handoff" in
`docs/product/handing-off-sessions.md`). While the switch is off the row's description says that, once it is on, a
session can do so without asking first; while it is on, it says that a session can do so without asking first. The Hand
off the user makes from the window is not affected.

## Ask questions as choices

A switch, **on to begin with**: whether the instructions Octoboard gives a session ask it to put a question that needs
the user's decision as options the user picks from, through the agent's own tool for asking, rather than listing the
options in its reply (what a session is told is in "The guidance on asking the user" in
`docs/product/launching-agents.md`). While the switch is on the row's description says so, and while it is off, that
once it is on sessions are asked to do so; either way it says that a change reaches sessions started afterwards. The
daemon stores it as `ask_questions_as_choices`. A session already running keeps what it was told when it launched; when
a change reaches a console session is in "The console session's instruction file" in
`docs/product/launching-agents.md`. An agent can neither read nor change the switch through the settings tools (see
"Who reads and changes what" in `docs/product/agent-settings-tools.md`).

## Keep sessions to their own project

A switch, **off to begin with**: whether a project session that edits or commits in another registered project is
stopped and asked to hand that work to a session of that project, the cross-project guard (see
`docs/product/keeping-sessions-to-their-own-project.md`). While the switch is off the row's description says what
turning it on will do; while it is on it says what is happening now, and that edits are checked in sessions started
afterwards. The daemon stores it as `keep_sessions_in_own_project`. Its git check follows the switch at once, in running
sessions too; its edit check is set up at a session's launch, so a change reaches it at the session's next launch (see
"Which sessions have it" in `docs/product/keeping-sessions-to-their-own-project.md`). An agent can neither read nor
change the switch through the settings tools (see "Who reads and changes what" in
`docs/product/agent-settings-tools.md`).
