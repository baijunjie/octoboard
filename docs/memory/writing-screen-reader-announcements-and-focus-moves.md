# Writing screen-reader announcements and focus moves

## Build an announcement or a focus move to a passing VoiceOver case, chosen before writing it

This applies before writing or changing code that announces or moves focus on its own. Announcing covers a
`StatusAnnouncer` or `AttendedStatusAnnouncer` write: its text, when it is written, attention or modal gating, and
whether the region is exposed. Moving focus covers an automatic move, a handoff after the focused element is removed,
and focus return. First find in `docs/voiceover-cases.md` a passing case whose behaviour the new code can equal on every
item of its section "What \"matches a case\" means". Then build to that case and name it in the plan. A caller that
matches a passing case needs no new real-device listening pass (`docs/ui-standard.md`, section "Reusing VoiceOver
verification"). Code built first and compared afterwards usually matches nothing.

- A passing status case and a passing focus case do not add up to a recording of a status written around that focus
  move. The timing relation is an item of its own, and no case heard the pair together.
- Which cases qualify, and what a case that is silent on an item covers, are set in `docs/voiceover-cases.md`, sections
  "When a real-device pass is required" and "What \"matches a case\" means"; which writes a listed caller is covered for
  is set in the preamble of section "Existing `StatusAnnouncer` callers and the case each matches". Read them before
  claiming a match. A caller's listing there never covers a focus move or handoff the caller gains; match that to a
  focus case of its own.
- If no passing case can be matched, tell the user before building. Then either reshape the behaviour until it equals a
  passing case, or build it and record it under "Open items" in `docs/voiceover-cases.md` as needing a listening pass.
