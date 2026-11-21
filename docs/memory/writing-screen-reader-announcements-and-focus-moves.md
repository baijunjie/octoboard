# Writing screen-reader announcements and focus moves

## Read the case rules before claiming a VoiceOver match

The rule (pick a passing case in `docs/voiceover-cases.md` before building, name it in the plan) is
`docs/conventions/ui-accessibility.md`, "General principles". Announcing covers a
`StatusAnnouncer` or `AttendedStatusAnnouncer` write's text, timing, attention or modal gating, and whether the region
is exposed; moving focus covers an automatic move, a handoff after the focused element is removed, and focus return.

- Which cases qualify, and what a case silent on an item covers, are in `docs/voiceover-cases.md`, "When a real-device
  pass is required" and "What \"matches a case\" means"; which writes a listed caller is covered for is in the preamble
  of "Existing `StatusAnnouncer` callers and the case each matches". Read them before claiming a match.
- A caller's listing there never covers a focus move or handoff the caller gains; match that to a focus case of its own.
- If no passing case can be matched, tell the user before building, then either reshape the behaviour until it equals a
  passing case, or build it and record it under "Open items" in `docs/voiceover-cases.md` as needing a listening pass.
