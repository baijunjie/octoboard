# Shared auto-trust permission

## Problem

Octoboard asks before it answers a trust confirmation only for Claude Code. The project-level record of that
permission is `claude_trust_consent`. A trusted parent directory is stored separately and is also consulted only
for Claude Code. The trust dialog and the trusted-folders settings describe only Claude Code. The dialog does not
say that agreeing is kept for later launches, or that the permission recorded in Octoboard is what allows a later
confirmation to be pressed without asking.

Codex and Grok are marked trusted for a single launch without asking. Codex is given a per-launch project-trust
override that is not saved. Grok is given a per-session home whose copy of the trust store has the project written
in as trusted; that copy is discarded with the session, so the decision does not apply to a later launch. Neither
path presses a confirmation the agent itself shows, so neither agent records the trust in the user's own
configuration.

Claude Code already follows the intended path. The user agrees in Octoboard's dialog, Octoboard presses Claude
Code's own confirmation, and Claude Code records the trust. Later launches of that Claude Code configuration do
not ask again.

## Outline

- One auto-trust permission in Octoboard, shared by Claude Code, Codex, and Grok. It has the two forms it has
  today: one project, or one parent directory.
- That permission means Octoboard presses the agent's own trust confirmation without asking. It is not itself the
  agent's trust record.
- Octoboard does not make a trust decision on an agent's behalf, and does not pre-seed one into a per-session
  copy. Pressing the agent's confirmation is what makes the agent record the trust, so later launches of that agent
  are trusted. Grok is the one case where the agent's own write cannot reach the user's store; there Octoboard
  carries the entry Grok wrote over to the user's store, unchanged.
- Codex is no longer marked trusted for one launch. Grok's session copy is no longer pre-seeded with trust.
- `claude_trust_consent` is renamed `trust_consent`.
- The trust dialog names the agent that is asking and states what agreeing does. The trusted-folders settings
  state the same behavior.

## Key design decisions

- **The agent records the trust; Octoboard does not decide it.** Agreeing lets Octoboard press the confirmation
  that agent is showing. Claude Code and Codex each write their own configuration, which is what makes a later
  launch of that agent already trusted. A per-session copy is not a place trust may be pre-seeded: it does not
  survive the session.
- **One shared permission, two forms.** `trust_consent` on a project, and a trusted parent directory, are the
  same kind of permission and apply to every brand. Either one is enough to press a trust confirmation without
  asking, whichever brand shows it. Agreeing for one brand therefore covers a later confirmation from either of
  the other two. One press does not write the other brands' configurations; each brand records its own trust when
  its own confirmation is pressed.
- **`trust_consent` replaces `claude_trust_consent`.** The stored meaning is the project form of this permission.
  It is set only when "Trust and continue" has been pressed and that press succeeded. Editing a project does not
  change it. Removing the project removes it.
- **A trusted parent directory is the other form.** "Trust parent folder" records the parent directory instead of
  the project, and only after the press succeeded. It covers every project whose directory is that folder or lies
  under it, including projects added later, for every brand. The rules for which parent may be offered stay as
  they are: not the filesystem root, not the home directory, and not a directory that contains the home directory.
  Removing a folder from settings stops auto-pressing for a project that has no `trust_consent` and lies under no
  other trusted folder. It does not erase a trust decision an agent has already recorded.
- **No permission, no press.** A project session with neither form of permission waits. Nothing is pressed until
  the user agrees. "Not now", Escape, the close button, and a click outside the dialog press nothing and record
  nothing. A press that fails, or a confirmation that is no longer waiting, records nothing.
- **A console session is pressed at once, without recording a permission.** Its working directory is the
  console's own and holds only the instruction file Octoboard wrote there. This holds for every brand.
- **Codex's per-launch project-trust override is removed.** Its own folder-trust confirmation then appears, and
  pressing that confirmation is what makes Codex record the trust. The separate bypass of the review Codex raises
  for Octoboard's own hooks stays. That review is not folder trust: the hook command is a different path every
  session, so one confirmation cannot be remembered. The bypass does not change the sandbox or the approval
  policy.
- **Grok's session copy is not pre-seeded with trust; Octoboard carries Grok's own entry to the user's store.**
  Grok's confirmation then appears. Pressing it (`y` or Enter) is Grok recording the decision. Grok 1.0.50 keeps its
  store at `GROK_HOME/trusted_folders.toml` and saves by replacing the file, so with the per-session home its write
  lands only in the session copy, and a symlink to the user's file is replaced rather than followed; no setting
  moves the store. So once Grok has written its entry into the session copy, Octoboard merges exactly that entry,
  unchanged, into the user's own `trusted_folders.toml`, leaving every other entry as it is. It does so both after
  its own successful press and after the person answers Grok's confirmation in the terminal themselves; the user
  chose the latter so that Grok behaves like Claude Code and Codex, whose own answer in the terminal is recorded in
  their own configuration too. An answer in the terminal records no permission of Octoboard's. Octoboard writes only
  an entry Grok itself wrote; it never writes one Grok did not.
- **The dialog states the behavior.** It names the agent that is asking. It says that agreeing presses that
  agent's own confirmation, and that the agent records the trust so its later launches do not ask. It says that
  Octoboard records one permission shared by every brand, so a later trust confirmation for this project is
  pressed without asking. When "Trust parent folder" is offered, it says that choice records that folder instead,
  and covers every project under it, including ones added later, for every brand. The caution still says that
  trusting lets the project's own permission rules and hooks take effect, and it says that only for the agent
  that is asking: Claude Code's `.claude/settings.json` may pre-approve tool permissions; Grok loads the
  project's instructions, hooks, and MCP servers; Codex's sandbox and approval policy are not changed by this
  confirmation.
- **Settings state the same behavior.** The trusted-folders section describes the list as the shared parent-directory
  permission: under those folders, every brand's trust confirmation is pressed without asking, and each agent
  records its own trust. A folder is still trusted from a session's trust dialog, and removing one leaves project
  permissions and trust an agent already recorded as they are.

## Milestones

- 01 shared-trust-permission (closed)
- [02 Final confirmation](02-final-confirmation.md)
