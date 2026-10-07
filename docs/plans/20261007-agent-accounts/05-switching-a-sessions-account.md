# Switching a session's account

> Goal: a session can be moved to another account of the same agent from its action menu, continuing its
> conversation under the new login.
> Completion criteria: switching a session that has a conversation relaunches it under the new account and the
> conversation continues; switching one that has none opens a fresh conversation under the new account; a switch
> whose relocation or relaunch fails leaves the session as it was and says why, and a relaunch that does not come up
> is reported as a failed switch rather than as a success. The menu entry is present exactly when the session's agent
> has more than one account, for a hub as well as a project session, and is translated in every offered language.
> The product docs no longer claim that a session keeps the directory it was opened with, that the user's own agent
> configuration is never written to, or that Octoboard never copies an agent's conversation history.

What the relocation has to move, per agent, and the fact that all three support it, are in
[the relocation findings](agent-relocation-findings.md).

## Technical design

- [ ] **The menu.** The session's action menu gains a Switch account entry with a submenu listing that agent's
      accounts, the session's current account marked as current and not actionable. An account that has since been
      removed is still marked as the current one, named by the directory the session recorded. The last entry of the
      submenu opens Settings at the Agent accounts section.
- [ ] **A hub is switched the same way**, from the Hub row's menu, which is where a hub's own actions live; its
      accounts are those of the console's hub agent.
- [ ] **When the entry appears**: only when the session's agent has more than one account. With one account the
      entry is hidden rather than shown and disabled — there is nothing the user could do with it.
- [ ] **What a switch does**, in order: end the session's process the way archiving does, relocate the session's
      conversation into the target account's directory, record the new account and its directory on the session, and
      relaunch it the way a resume does.
- [ ] A switch is refused, with the reason named, when the relocation does not complete and when the session is
      being launched or resumed at that moment. Nothing is recorded by a refused switch and the session stays on the
      account it had. A target directory that does not exist is not itself a refusal: it is created as part of
      relocating the conversation into it. A Grok target that is not an initialized Grok home is refused by the
      launch rule milestone 1 added.
- [ ] A session with no conversation on the agent's side has nothing to relocate; the switch records the new account
      and relaunches, which opens a fresh conversation, exactly as a resume of such a session does today.
- [ ] **The default account is a switch target like any other**, and counts towards the more-than-one test. Since it
      pins nothing, the directory its conversation is relocated into is the one it resolves to at that moment
      (milestone 2); resolve it once for the switch and use that same value for both the copy and the relaunch, so
      the two cannot disagree.
- [ ] The relocation copies rather than moves: the conversation stays in the account it came from as well. A switch
      that fails halfway therefore leaves the original intact, and switching back needs no second copy.
- [ ] **How a switch confirms it resumed**, the same way for all three agents: from the session's own behaviour as
      Octoboard already observes it to know that a session has a conversation. Each agent does also refuse a resume
      it cannot find, with a message of its own, but reading those messages would be the raw-output matching this
      plan rejects for usage limits, and for Grok it would not work anyway — it checks the login before it looks for
      the session. A relaunch that **does not come up** is therefore reported as a failed switch rather than as a
      success, and the session is left on the account it had. A relaunch that comes up into an empty conversation is
      not covered, and cannot be: that is the trap the relocation findings name, and no agent produces it today.
- [ ] The session's config directory stops being immutable and becomes writable through this one path only. Opening a
      session and resuming one keep taking the value they take today.
- [ ] The switch is the user's action. Nothing switches a session on its own, and no usage signal is read.

## Implementation plan

- [ ] Add a protocol request to switch a session's account, and the write path on the session record that it needs.
- [ ] Carry out a switch as the sequence above, on the same path a resume runs on, so that everything Octoboard
      injects is reassembled and the existing launch refusals still apply.
- [ ] Perform the relocation by copying the conversation record at the same path relative to the config directory it
      occupies in the account it came from, creating the intermediate directories. Treat an incomplete relocation as
      a failure of the switch rather than relaunching anyway.
- [ ] Report the outcome on the session the way a resume's failure is reported.
- [ ] Add the menu entry and its submenu, and add every string to the message catalogues of all offered languages.
- [ ] Update the product docs, in `docs/product/launching-agents.md`:
      - reword the promise that the user's own agent configuration is never written to, so that it states what a
        switch writes: one conversation record of one session, copied into the target account's directory, with no
        settings file of the agent's touched;
      - reword "Agent session data", which says Octoboard never copies an agent's conversation history, for the same
        reason.
- [ ] Update the product docs on a session's account being able to change at all, which contradicts a rule stated in
      two places — "A session keeps the directory it was opened with" in `docs/product/consoles-and-projects.md`, and
      the relaunch bullet of "Archiving, interruption and resuming" in `docs/product/sessions.md`. Say there, too,
      that a switch carries the session into the target account's whole setup rather than only its login: a setting
      the user keeps in one account's directory does not follow the session into another's.
- [ ] Update `docs/product/sidebar.md`: it lists what a session row's action menu offers — Pin or Unpin, Rename,
      Archive — and this milestone adds an entry to it, and to the Hub row's menu.
- [ ] Write the per-agent relocation facts into `docs/agent-cli-reference.md`, in the form that doc already uses,
      extending its version header to the versions measured.

## Notes for the developer

**Reusable capabilities**

- Archiving and resuming already do the two halves of a switch: ending a process gracefully, with the grace period
  the agents' shutdown hooks depend on, and relaunching with the full injection reassembled. A switch is those two
  with a relocation and a record update between them, not a new launch path.
- The existing launch refusals — a vanished config directory, a Grok home that is not initialized, a resume of an
  already-running session — apply unchanged and should not be reimplemented.
- The session action menu and its confirmation patterns already exist for archiving.
- Whether a session has a conversation on the agent's side is already tracked, since a resume depends on it.

**Development notes**

- A session's role and its agent stay fixed for its lifetime; a switch stays within one agent and must not offer
  another agent's accounts.
- The daemon observes a process's exit directly and decides from that whether a session is interrupted or archived.
  A switch has to leave the session in a state that logic reads correctly while the process is down.
- Octoboard installs nothing into a project and does not write the user's agent configuration. The relocation is the
  single, narrow exception this milestone introduces; keep it to the one conversation record and say so in the docs.

**Reference docs**

- `docs/product/sessions.md` — "Archiving, interruption and resuming".
- `docs/product/launching-agents.md` — "What Octoboard never modifies" and "Agent session data", the two promises
  being reworded.
- `docs/agent-cli-reference.md` — where the per-agent facts go.
- `docs/memory/writing-daemon-code.md`.
