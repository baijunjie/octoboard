# 02 Final confirmation

> Goal: confirm against the real agent CLIs what milestone 01 verified only by replaying captured terminal output.
> Completion criteria: every item below has been run against a live Claude Code, Codex and Grok Build through
> Octoboard, and any defect found has been fixed.

Run each in a fresh test git repository that holds an `AGENTS.md` (Grok Build asks only when the folder has
trust-gated content), with no Octoboard permission recorded for it. Back up `~/.codex/config.toml`,
`~/.grok/trusted_folders.toml` and Claude Code's configuration first, and restore them afterwards.

- [ ] A project session of each agent stops at that agent's own trust confirmation, and Octoboard's dialog names the
      agent and shows the caution for it (and, for Codex, the repository-root note).
- [ ] "Trust and continue" presses the confirmation and records `trust_consent`; a later session of each of the other
      two agents in that project is pressed without asking.
- [ ] "Trust parent folder" does the same for another project under that folder.
- [ ] A later launch of the agent that was pressed does not ask again: Claude Code and Codex record the trust in their
      own configuration (Codex for the repository root), and Grok Build's entry is in the user's own
      `trusted_folders.toml`, not only in the session's copy.
- [ ] Answering Grok Build's confirmation in the terminal also carries its entry into the user's store, and records no
      Octoboard permission.
- [ ] A message a console session sends to a Codex or Grok Build session waiting at its confirmation is held, not
      written into the confirmation, and is delivered once the confirmation is answered.
- [ ] Codex launched with `check_for_update_on_startup=false` shows no update prompt before its trust confirmation
      when an update is available.
- [ ] Grok Build's own writes to `trusted_folders.toml` and Octoboard's carry exclude each other through the `flock`
      Octoboard takes on `trusted_folders.toml.lock` (Grok Build's locking scheme was inferred, not confirmed).
