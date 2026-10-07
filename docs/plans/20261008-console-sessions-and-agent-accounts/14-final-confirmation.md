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
