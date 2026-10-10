# Final confirmation

> Goal: the checks that only a person can make are made, and anything they turn up is fixed here.
> Completion criteria: both items below are confirmed in the packaged app, or fixed and then confirmed; nothing is
> left beyond this milestone.

## Carried from milestone 02 (expanding collapsed lines in the viewer)

Everything else about the expansion was verified in the packaged app. These two cannot be driven by an agent at
all — they need a person at the machine:

- [ ] A screen reader (VoiceOver) on the separators: their names, the loading and failure announcements, and the
  controls marked unavailable while a read is out.
- [ ] A real input method typing into the viewer while a diff is shown, confirming composition does not reach the
  separator controls' key handling.

## Notes for the developer

**Development notes**

- Every check that an agent can drive was handed to milestone 05 instead, since the only thing in its way was the
  machine's screen locking.

**Reference docs**

- `docs/memory/building-and-launching-the-app-for-verification.md`, `docs/memory/verifying-the-desktop-ui.md`
