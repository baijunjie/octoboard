# Full-width punctuation under a CJK input method

> Goal: typing a full-width punctuation mark with a CJK input method puts it into the terminal on the first key press,
> as it does in a native macOS application.
> Completion criteria: with a Chinese input method active, pressing the key for `？` once inserts `？` into the agent's
> input line; the same holds for the other full-width marks the input method emits without a candidate window, and
> composed CJK *text* (which already works) is unaffected.

## Current status

**The workaround is implemented and user-verified on xterm 5.5.0.** On 2026-10-08, the user confirmed that they had
tested the workaround and it works. This records the user's overall confirmation, not a separate result for every regression
case listed below.

The immediate usability fix is accepted. This plan stays open only to replace the workaround with an upstream fix
and remove the dependency on xterm's private state. That is low-priority maintenance; it does not promise a further
improvement in normal punctuation input over the working workaround.

## Problem and current solution

Without the workaround, `xterm.js` 5.5.0 in WKWebView drops the first full-width punctuation mark that a Chinese input
method commits directly, without a candidate window. A second press works; composed CJK text is unaffected.
The library sets `_keyDownSeen` on every keydown, including a bare `Shift`, while WebKit can deliver the punctuation's
`input` before its own `keydown`. The already-set flag then prevents xterm's input fallback from forwarding the mark.

`packages/ui/src/terminal/TerminalController.ts` carries the workaround in
`keepModifiersFromArmingKeyDownSeen`, called first from the terminal's custom key handler. A modifier-only keydown
restores the flag to the state left by preceding keys. The implementation was added in commit `ad660f2` on 2026-10-05.

`packages/ui/package.json` declares `@xterm/xterm` `^6.0.0` and `@xterm/addon-fit` `^0.11.0`;
`pnpm-lock.yaml` currently pins them to 6.0.0 and 0.11.0. The 6.0.0 source and published package still expose the
private field and call the custom handler after arming it on keydown and clearing it on keyup, so the workaround is
retained. This dependency upgrade does not contain the upstream punctuation fix. The user confirmation above
predates the upgrade and is not a separate manual IME acceptance result for 6.0.0.

Upgrade checks passed: workspace typechecking, the UI test suite and production build, plus a focused WebKit check
of the real terminal controller for mounting, fitting and resizing, output, ASCII input, `Ctrl+C`, and the modifier
and rollover flag states. The synthetic key events check integration with the new library, not a real input method.

## Design constraints for the replacement

- Keep xterm's default composition handling. Do not introduce a bespoke composition handler or synthesize composition
  events to solve this defect.
- Keep the working workaround until a replacement passes verification in the real application. Because it accesses
  a private field, recheck it on every xterm upgrade, even when the upgrade is for another reason.
- Preserve the distinction between restoring the preceding flag state and unconditionally clearing it. The workaround
  follows [#6054](https://github.com/xtermjs/xterm.js/pull/6054)'s approach of not arming the flag on a modifier;
  [#6200](https://github.com/xtermjs/xterm.js/pull/6200)'s proposed assignment also clears it when another key left it
  set. Our source analysis suggests that difference reaches the key-rollover case; do not assume the two are
  equivalent.
- [#6045](https://github.com/xtermjs/xterm.js/issues/6045) warns that relaxing this gate can cause duplicated input
  during fast typing. The current workaround deliberately leaves the flag armed while a preceding non-modifier key
  is still down. A shifted mark can therefore still be dropped during rollover according to the source analysis;
  this is not an observed result from the user's confirmation, nor a claim that the workaround fixes all IME issues.

## Upstream tracking

Checked on 2026-10-08: [#6054](https://github.com/xtermjs/xterm.js/pull/6054) and
[#6200](https://github.com/xtermjs/xterm.js/pull/6200) are both open and unmerged. Resume the replacement work when an
upstream fix is available in a version the project can adopt; a merged PR alone is not proof that the installed
package contains the fix.

The original investigation and reporting do not need to be repeated:

- [#6144](https://github.com/xtermjs/xterm.js/issues/6144) already describes the same macOS WKWebView / Chinese Pinyin
  first-character drop. Plain Safari reproduction in [#5374](https://github.com/xtermjs/xterm.js/issues/5374) and the
  library guard explain why changing the web engine is not the chosen remedy; the original engine-comparison step
  is superseded by these findings and the working workaround.
- Source research on 2026-10-05 found the same guard and unconditional arming in 5.5.0, 6.0.0 and the then-current
  master, with no terminal option that fixes this path. Turning on `screenReaderMode` disables the input fallback
  rather than fixing it. These are dated findings, not a claim about future releases.
- The version range and reproduction evidence were posted, with the user's authorization, to
  [#6144](https://github.com/xtermjs/xterm.js/issues/6144#issuecomment-5986424483) and
  [#6200](https://github.com/xtermjs/xterm.js/pull/6200#issuecomment-5986424653) on 2026-10-05. Reporting is complete;
  opening a duplicate issue or waiting for a reply is not an outstanding local implementation step. Those comments
  reported the defect, not successful testing of either upstream patch.

## Remaining work

- [x] Implement the local workaround and obtain the user's confirmation that it works (2026-10-08).
- [ ] Adopt an upstream release that fixes modifier-only arming of `_keyDownSeen`, inspecting its actual implementation
      and the compatible addon version rather than relying only on the release number or PR status.
- [ ] Remove `keepModifiersFromArmingKeyDownSeen` and its invocation as part of that replacement, and verify the
      replacement without the local workaround in the real desktop application using a Chinese input method:
      `？`, `！` and `（` on the first press; normal Pinyin composition; no duplicated characters when typing `，？`
      quickly; `Ctrl+C` still interrupts the agent; and releasing `Shift` between two presses of `？` does not lose
      the first. Check rollover explicitly rather than assuming the upstream patch resolves it.
- [ ] Update the terminal's maintained documentation to match the replacement and close this plan once verification
      passes and the workaround is removed.

## Notes for developers

Input-method verification requires a person using the real input method. Scripted key injection is not evidence
that this path works. The regression cases above are acceptance checks for the future replacement, not a renewed
request to validate the accepted workaround.

The terminal requirements are in "The terminal" in `docs/product/sessions.md`; the keyboard and WebView pitfalls
are in "Known pitfalls of the Tauri / Rust approach" in `docs/architecture.md`.
