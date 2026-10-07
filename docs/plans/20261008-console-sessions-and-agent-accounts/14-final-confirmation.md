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
