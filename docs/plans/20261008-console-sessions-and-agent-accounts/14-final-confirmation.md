# Final confirmation

> Goal: every check this topic could not run while it was being built has been run, against the real
> application, and what it turned up has been fixed.
> Completion criteria: each item below is confirmed in the running application, and any defect a
> confirmation turns up is fixed here rather than handed on.

The end point of the topic. Items arrive here from a closed milestone when the implementation is
complete, only the verification is missing, and nothing that follows depends on its result. Nothing
is left beyond this milestone.

## From milestone 02 — the binding, and the end of the one-live rule

- [ ] Two console sessions live in one console at the same time, each **reachable and selectable** in
      the running window. The daemon side is covered by tests; the application side is not, and the
      sidebar it is reached through is rebuilt by milestone 03, whose own verification covers the
      same ground.
- [ ] A database written by an older build is **moved aside** and the daemon comes up on a fresh one,
      observed on a real `~/.octoboard` rather than in a unit test. Check that the application loads
      rather than failing to start, and that the superseded file is where
      `docs/product/application-lifecycle.md` says it is.
- [ ] The console session instruction file survives **two opens of the same console at once** without
      an agent reading a truncated file. The write is atomic by construction (write to a temporary
      file in the same directory, then rename); what is unverified is the behaviour under real
      concurrent opens.

## From milestone 03 — the sidebar's console sessions

- [ ] A console with three console sessions **lists all three**, and a new one can be **created, selected and
      renamed** from the section, end to end against a real daemon. Covered at the unit and gallery level only.
- [ ] Each console session's **colour** reads as distinct on screen, in both light and dark, and the six
      palette entries hold their measured 3:1 against the surfaces a row sits on. The ratios are computed, not
      observed.
- [ ] The binding badge's **tooltip** appears on pointer hover and names the owner. It has no keyboard route — a
      known, kept gap for a sighted keyboard-only user, recorded in a comment on `BindingBadge.tsx` — so this
      confirms the hover case only, not a keyboard one.
- [ ] **Keyboard navigation** through the new console sessions section: reaching the section, its rows, its
      create action and its menu, with focus visible throughout.
- [ ] The section's **empty state** reads correctly in a console with no console session.
- [ ] The **gallery scenarios** run clean in a browser. There is no scenario runner in the repository —
      `pnpm gallery` opens an interactive dev server — so every count a scenario asserts has only been
      re-derived by hand against the fixtures. `archive-many` and `archive-console-sessions` are the two this
      topic perturbed.

## From milestone 04 — accounts: storage and protocol

- [ ] The **console dialog still works unchanged** for a user, round-tripped through a live daemon: creating
      and editing a console's per-agent directory field, and a session actually launching with what was
      saved. Covered at the protocol and unit level only. The dialog becomes a picker in milestone 07, so
      this is worth confirming before that replaces it.

## From milestone 05 — agent availability and the default account

- [ ] On a real machine with at least one agent actually installed, the daemon's one-time determination
      **lands within a couple of seconds of starting**, without the application's own start-up appearing to
      wait on it — the non-blocking shape (`tokio::task::spawn_blocking` fired and not awaited before the
      server starts serving) is verified by reading the code and by the unit tests exercising the pure
      `determine` function and the no-op failure path; the actual wall-clock behaviour of a real ten-second
      shell snapshot racing a real application start has not been timed.
- [ ] An agent installed **only under a version manager or in `~/.local/bin`** (not on a plain `PATH`) is
      found as available through the login-shell snapshot, the way a launch already finds it — this
      mirrors an existing, tested capability (`env_shell::resolve_binary`) but the availability path
      itself has only been exercised against a synthetic `PATH` in a unit test, never a real shell profile.
- [ ] The **session dialog**, the **console dialog**'s two agent rows and the **sidebar**'s console-sessions
      empty state render the unavailable-agent styling and the install prompt correctly on screen, in both
      light and dark, and in a right-to-left language — covered by `pnpm typecheck` and by unit tests that
      assert the underlying data (`isDisabled`, the rendered text) but never by looking at the rendered
      HeroUI `Select` popover or `EmptyPanel` itself.
- [ ] On a machine with **no supported agent installed at all**, every one of the three places this
      milestone refuses — the session dialog, the console sessions section's own action, and a console
      session's `start_session` tool called from a real agent conversation — is confirmed end to end
      against a live daemon, including that the install prompt's wording reads naturally in context in
      each of the three.
- [ ] The **zh-Hans** catalog's new strings (`agents.installPrompt`, `agents.notInstalled`,
      `daemon.agent_not_available`) are confirmed to read naturally in the running application; only
      placeholder parity with the English source is checked automatically (`catalog.test.ts`), not the
      translation's sense or register.
- [ ] The **disabled picker option** as it is actually rendered: an unavailable agent is visible, reaches
      keyboard traversal (`disabledBehavior="selection"`, verified from the react-aria sources only), is
      announced as disabled, and cannot be chosen.
- [ ] The sidebar's **install-prompt banner above interrupted console sessions** reads well in light, dark
      and right-to-left.

## From milestone 06 — the Agent accounts section in Settings

- [ ] **Where keyboard focus lands around a removal**: confirming it leaves focus on the Agent accounts
      tab, and cancelling the confirmation leaves focus on the Remove button that opened it. The two paths
      are told apart by a counter bumped only on the confirm path, since react-aria's own focus restore
      and the section's refocus race in the same commit; the timing is reasoned from the library's
      sources, not observed.
- [ ] **Dialogs over dialogs.** The add and edit form, the directory picker opened from it, and the remove
      confirmation all open over the Settings modal: Escape closes only the topmost one and Tab stays
      inside it.
- [ ] **The section as it renders** — the group headers, the default account's two-line description, a long
      directory cut from its start with the full path as its tooltip, a long or right-to-left account name
      clipping with a fade and showing its full name as a tooltip, and the Edit and Remove buttons. The
      `settings-accounts` gallery scenario shows all three availability states and a long path.
- [ ] The section and its forms under a **right-to-left language**.
- [ ] An account added, renamed, repointed or removed in one window **appears in another open window**.
- [ ] **A name collision is reported in place in every case**: against another account of that agent,
      against the default account's name in the current language, and against the literal `Default` the
      daemon reserves as its comparison key — the last two confirmed under Chinese, where the two names
      differ.
- [ ] A `~/…` directory and a **directory that does not exist yet** are both accepted, and the agent
      creates the directory on its first run under that account.

## From milestone 07 — choosing an account where an agent is chosen

- [ ] **The grouped control open**: the group headings carry the agent's icon and name, and "(not installed)"
      where that applies; an unavailable agent's entries stay reachable by arrow keys, are announced as
      disabled, and refuse Enter. The headings are a HeroUI `Header` that has never been seen rendered, and
      their contrast on the popover surface is computed rather than observed.
- [ ] **The grouped control closed**: the trigger shows the agent's icon and "Agent (Account)", and a long
      account name truncates rather than pushing the control out of shape. The same in zh-Hans.
- [ ] **Opening a session by keyboard alone** through the grouped control: into it, across the groups, onto an
      entry, and submitted.
- [ ] **On a machine where one agent is genuinely not installed**, the session dialog opens on a selectable
      entry and the new-console dialog opens its two agent pickers on a selectable agent; a console already
      stored against that agent still keeps it when edited.
- [ ] **The account is the one that is actually used**: a session opened on a non-default account launches with
      that account's directory, and one opened on the default account launches with nothing pinned — the case
      that has to behave exactly as it did before this change.
- [ ] **The account is named where the plan says it is**: "Claude Code (Work)" on a focus-mode session card, on
      an archive view row, and in the description a session row gives assistive technology. After the account
      is removed in Settings, an existing session is named by the directory it recorded.
- [ ] **Bidi**: under a right-to-left language the "Agent (Account)" run keeps the account isolated, on the
      focus-mode card, the archive row and the dialog's trigger — checked with a Latin-letter name and with a
      path. Both catalogues are left-to-right today, so nothing shows this short of switching language.
- [ ] **A clipped focus-mode card line** fades at its end, shows the full text as a tooltip, and runs its
      marquee on hover in a narrow sidebar.

## From milestone 08 — switching a session's account

The relocation itself is measured fact (the findings this topic carried), and the implementation is covered by
unit tests against synthetic directories. What is missing is a real agent and a real window.

**Needs an agent actually installed**

- [ ] **A full switch for each of the three agents**, on a session that has had a turn: the conversation continues
      under the target account, and the record is found where the findings say it is. Nothing has run this end to
      end.
- [ ] **The "did not come up" signal against real agents.** An agent that cannot find the record has to exit inside
      the four-second settle window for a failed switch to be reported as one. Grok checks its login before it looks
      for the session. A switch to a Claude Code or Codex account that is not logged in may stay up at a login
      prompt and count as a success — that is expected, not a failure.
- [ ] Whether **Codex creates its home directory** when the target does not exist yet; the findings leave this
      unestablished.
- [ ] **A console session's own switch**, and a switch of a session that is mid-turn.
- [ ] **A switch back to the account it came from**, which replaces the older copy there.
- [ ] A **Grok target that is an initialized home**, and that the launch refusal still catches one that is not.
- [ ] **Two sessions switched into the same account in quick succession** both end up with their record in place and
      no staging directory left — the case the per-copy staging path exists for.
- [ ] A **failed relaunch leaves no stale staging directory**, and the directory is gone after a successful switch.

**Needs a window**

- [ ] **The submenu**: the current account marked and inert, the separator, and Manage accounts… opening Settings on
      the Agent accounts section. By keyboard as well as by pointer. The entry is absent when the agent has one
      account, and the zh-Hans wording reads naturally in place.
- [ ] **The wait.** For the five to eight seconds a switch takes, keyboard focus stays on the Switch button, Escape
      and Tab keep working inside the dialog, the button shows its pending state and reads "Switching account…" in
      both languages, and Escape or a click outside does **not** dismiss the dialog.
- [ ] **The terminal during the switch** reads "Resuming session…" throughout, with no Resume button appearing
      between the process dying and the relaunch — the button that would otherwise invite a resume in the middle of
      a switch.
- [ ] **A failure** shows inline in the dialog, and only falls back to a toast if the dialog is somehow gone.
- [ ] The **no-conversation wording** appears for a session nobody has typed into, rather than the one promising the
      conversation continues.
- [ ] A **removed-account entry** renders its path left-to-right with a tooltip when clipped.
- [ ] The terminal **reattaches** to the new process across the Interrupted-to-Idle transition.

