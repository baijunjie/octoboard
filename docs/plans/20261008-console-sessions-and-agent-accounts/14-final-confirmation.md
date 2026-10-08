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

## From milestone 05 — agent availability and the default account

- [ ] The one-time determination's real **wall-clock cost** against a real login shell, and that the window
      starts serving clients before it lands.
- [ ] An agent installed only under a **version manager** is found, since it resolves through the login-shell
      snapshot and not through the daemon's own environment.
- [ ] The three refusal surfaces end to end against a live daemon **with no agent installed at all**: the
      session dialog's disabled submit, the console sessions section's create action, and a console session's
      own start-session tool.
- [ ] The rendered **disabled picker option** — that an unavailable agent is visible, reaches keyboard
      traversal (`disabledBehavior="selection"`, verified from the react-aria sources only), is announced as
      disabled, and cannot be chosen.
- [ ] The sidebar's **install-prompt banner above interrupted console sessions** reads well in light, dark
      and right-to-left.
- [ ] The new zh-Hans strings read naturally in place.
