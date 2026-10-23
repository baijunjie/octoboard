# Measuring build and test speed

## Judge a speed change by CPU time and output size, measured back to back, never by wall-clock time

Applies to deciding whether a change makes a build, type check, lint or test run cheaper, on a development machine
shared with other agent sessions building and testing in their own worktrees, which is the normal state here. Wall-clock
time on such a machine follows the other sessions' load rather than the change: within a single before/after pair the
load average has moved between about 15 and 170.

- Run the before and the after back to back, at least two pairs, and record the load average with every run.
- Compare CPU time (user plus system) and, for a build, the size of what it produces; treat a difference no larger than
  the spread between repeat runs of the same side as no gain.
- A change that shows no gain by those numbers is dropped, not kept on the hope that it helps on a quieter machine.
