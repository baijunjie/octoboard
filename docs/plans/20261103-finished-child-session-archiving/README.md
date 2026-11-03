# Finished Child Session Archiving Development Plan

## Problem and approach

A child session is archived automatically only by its own `report` with `status: done` and no open items (see
"Automatic archiving" in `docs/product/reporting-to-owners.md`). Three gaps leave finished child sessions running, with
nothing telling their owner:

- **Finished, but no report.** A session that finished its work (merged, cleaned up) and ended its turn without calling
  `report` gets only a synthesised report. A synthesised report never archives the session, and the owner gets no later
  nudge. One session did this twice in a single coordinator's run.
- **Items handed to the owner block the archive.** `report` has one `open_items` list. A reporter whose remaining items
  are the owner's to do (for example "needs local testing by the parent") has nothing left to do itself, yet any open
  item keeps it running. In the same run a `done` report whose open items were only local testing and checks left
  for the parent kept its reporter running.
- **No signal of children awaiting archive.** Once a child's last report is delivered and it sits idle, nothing in
  `list_sessions` or elsewhere tells the owner that it is finished and waiting to be archived rather than waiting for
  instructions.

The approach: let a report name items handed to the owner separately from its own unfinished items, so only the latter
block automatic archiving; and give the owner a signal for children that are idle after their last delivered report —
a flag on the session records and a notice written to the owner.

## Key design decisions

- **Items handed to the owner do not block automatic archiving.** A report gets a second list, separate from
  `open_items`, for items that are the owner's to carry out. A `done` report whose only items are in that list archives
  the reporter as a `done` report with no open items does today. `open_items` keeps its meaning: work the reporter has
  not finished.
- **The `report` tool's shape is part of the frozen MCP contract.** "The MCP server as a contract" in
  `apps/daemon/PROTOCOL.md` freezes the server key, tool names and call shape. The new list is an optional argument:
  existing callers, which never pass it, behave exactly as today. The change still updates the tool's description, the
  `report` row of `docs/product/reporting-to-owners.md`, and wherever `PROTOCOL.md` documents the tool, and the plan
  treats it as a contract addition to be stated explicitly, not as an internal change.
- **The owner is nudged about idle children rather than the child being archived for it.** A session that ended
  without a qualifying report stays as it is; Octoboard only tells the owner. Archiving stays the owner's (or the
  user's) decision, as the rule that a synthesised report never archives already holds.
- **The nudge covers both a child that never reported and one whose report did not archive it.** The trigger is the
  same: the child has been idle with its last report (its own or synthesised) delivered to the owner.
- **The signal can be pushed, read, or both.** A notice written to the owner, and a flag in the session records the
  owner reads (`list_sessions`, `get_session`) telling a child idle after its last delivered report apart from one
  awaiting instructions. The plan carries both as separate milestones, so either can be dropped (see "Open").

## Milestones

- [01 Items handed to the owner in a report](01-items-handed-to-the-owner.md)
- [02 Marking children idle after their last report](02-idle-after-report-flag.md)
- [03 Nudging the owner about idle children](03-owner-nudge.md)

## Open

- Whether both the record flag (milestone 02) and the nudge (milestone 03) are built, or only one: the direction
  names a nudge "or" a `list_sessions` flag. Neither milestone depends on the other's behaviour beyond the shared
  idle judgement.
- The name of the new `report` argument (for example `owner_items` or `handed_to_owner`) and how the delivered report
  presents the two lists to the owner.
- Whether a `needs_decision` or `failed` report with only owner items changes anything (the direction above only covers
  `done`).
- Whether the unfinished-session refusal for a coordinator's completion report applies unchanged when its items are
  only owner items (the inference is yes: its own children are still its work).
- How long a child must be idle before the owner is nudged, whether the nudge repeats, and whether the owner can
  silence it for a child it is deliberately keeping.
- Which facts make a child count as idle for the flag and the nudge: at its prompt, `paused` false, no
  `background_tasks`, no session under it running, nothing queued for it — and whether a session waiting for the user
  is excluded (it already has its own notice).
- The record field's name and shape (a boolean, or the time the last report was delivered).
