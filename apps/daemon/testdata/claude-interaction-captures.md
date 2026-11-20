# Claude Code prompt and composer captures

Captured on 2026-11-20 from Claude Code 2.1.328 running interactively in a 120x40 PTY, from a throwaway home in manual
permission mode (`permissions.defaultMode: "default"`) against a local stand-in for the Messages API that scripted each
tool call. Each file is the raw output from the launch on; a state is the screen after replaying a prefix of it into the
daemon's own screen (`TerminalSnapshot::new(40, 120)`, read with `visible_text_undimmed`). The tests slice the files at
the offsets below.
The probe directory's machine-local path is rewritten in every file to `/private/tmp/probe-dir/...`, segment by
segment at the same length, so the byte offsets and the screen columns are those of the capture.

## `claude_interaction_prompts.bin` (280041 bytes)

Permission prompts and `AskUserQuestion` questions. Offsets are the end of the state.

| offset | state |
|---|---|
| 17860 | Bash `touch` prompt: Yes / always allow access to the directory / switch to auto mode / No; footer `Esc to cancel · Tab to amend` |
| 18172 | the same after Down: highlight on row 2, footer without `Tab to amend` |
| 18622 | after three Downs: highlight on the last row `No`, footer with `Tab to amend` |
| 18750 | Tab on the last row: its text field is open (empty, reads `No,`), footer without Tab |
| 19282 | text pasted into that field (`No, typed-part pasted-part`) |
| 22796 | the declined prompt gone: the input box and mode line back |
| 60801 / 60872 | Tab on the Yes row: field open (`Yes,`), then with text |
| 107289 / 107860 | Write prompt (`Create file`), then with the `No` field open |
| 127132 | Edit prompt (`Edit file`) |
| 142424 / 148490 | WebFetch prompt (`Fetch`; no footer, no amend), then with the highlight on row 3 |
| 166285 | Bash prompt whose second row is a rule field, `don't ask again for: python3 *` |
| 183796 / 184101 | the same for `curl *`, then with the rule edited to `curl -s` |
| 207990 | `AskUserQuestion`, one single-select question (`☐ Colour`, `Type something.`, `Chat about this`) |
| 226190 / 226406 | the `Type something.` row focused (empty, reads `3.`), then with text pasted |
| 241708 | after Esc: `Declined to answer Claude's questions`, box back |
| 244837 / 244953 | one multi-select (`←  ☐ Fruits  ✔ Submit  →`, `[ ]` boxes, unnumbered `Submit`), then digit 1 pressed |
| 245551 / 245628 | text typed in its last row (checks it), then Down onto the `Submit` row |
| 246464 / 250003 | `Review your answers` (`1. Submit answers` / `2. Cancel`), then submitted, box back |
| 252986 | three questions, first tab; chips `←  ☐ Colour  ☐ Fruits  ☐ Size  ✔ Submit  →` |
| 255523 | Submit tab reached with nothing answered: `⚠ Some questions are unanswered` |
| 259382 / 260347 | second tab (multi-select, `Next` row, chip `☒ Colour`), third tab |
| 261654 / 265336 | review of three answers, then submitted, box back |

What the plain text shows that the raw rendering hides: the placeholder of an open field is faint, so an empty
`No, and tell Claude what to do differently` field reads `No,` and the focused empty `Type something.` row reads `3.`.

## `claude_interaction_composer.bin` (116051 bytes)

The composer. Offsets are the end of the state.

| offset | state |
|---|---|
| 17922 | `/help` pasted: the completion popup above the box, `/help` highlighted |
| 24127 | `/he` pasted: popup with `/help` highlighted (Enter ran `/help`) |
| 31947 | `/help` typed: popup with `/help` highlighted |
| 37584 | `/probecmd foo bar` pasted: no popup (a space ends it) |
| 44794 | `/probec` pasted: popup with the one custom command `/probecmd` |
| 69410 | `/probe` pasted: popup with `/probecmd` highlighted (Enter ran `/probecmd`, not `/probe`) |
| 73351 | the same after the custom command loaded: `/probe` highlighted above `/probecmd` |
| 116051 | `compare [Image #8] and [Image #9]` in the box, two pasted image paths |
