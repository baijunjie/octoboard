# 03 Nudging the owner about idle children

> Goal: an owner is told when a child of its has stayed idle after its last delivered report, so a finished child that
> never sent a qualifying report is not forgotten.
> Completion criteria: after a child has been idle for the chosen delay with its last report delivered, the owner
> receives one notice naming the child, saying whether that report was the child's own or synthesised, and suggesting
> it check the child and archive it if its work is over; no notice comes for a child that started a new turn, was
> archived, or is otherwise excluded by the idle judgement; covered by daemon tests and listed in "Notices to an owner"
> in `docs/product/reporting-to-owners.md`.

## Technical design

- [ ] A new owner notice, alongside the existing ones in "Notices to an owner", delivered the way every notice is:
      queued while the owner cannot take a message, kept for an interrupted owner (dated like the other kept notices),
      not told to an archived owner.
- [ ] The trigger uses the idle judgement of milestone 02 plus a delay (length open, see the overview).
- [ ] The notice never archives the child; archiving stays with the owner or the user.

## Implementation plan

- [ ] Start a timer when a child becomes idle after a delivered report; cancel it when the child starts a turn, is
      archived, or stops counting as idle.
- [ ] On expiry, re-check the idle judgement, then write the notice through the owner-notice path.
- [ ] Add the notice text to the owner-notice texts, naming the tool to archive with as the owner's agent shows it.
- [ ] Tests: notice after the delay for an own non-archiving report and for a synthesised report; none after a new
      instruction, an archive, or background work starting; kept-notice behaviour for an interrupted owner.
- [ ] Document the notice in "Notices to an owner" and tell owners about it in their role descriptions.

## Notes for the developer

- **Reusable capabilities**: the owner-notice texts and their delivery (queueing, keeping for an interrupted owner,
  dating kept notices, dropping a kept notice superseded by a later one); the idle judgement from milestone 02.
- **Development notes**: a kept notice of this kind should give way to any later report or notice about the same
  session, as a kept wait already does.
- **Reference docs**: `docs/product/reporting-to-owners.md` ("Notices to an owner", "Reports and notices for an
  interrupted owner"), `docs/product/message-delivery.md`.
