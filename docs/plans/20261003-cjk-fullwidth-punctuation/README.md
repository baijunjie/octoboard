# Full-width punctuation under a CJK input method

> Goal: typing a full-width punctuation mark with a CJK input method puts it into the terminal on the first key press,
> as it does in a native macOS application.
> Completion criteria: with a Chinese input method active, pressing the key for `？` once inserts `？` into the agent's
> input line; the same holds for the other full-width marks the input method emits without a candidate window, and
> composed CJK *text* (which already works) is unaffected.

Low priority. This is a usability defect in one input path, not a blocker for anything else, and the terminal is usable
in the meantime by pressing the key twice or by switching the input method to English.

## Problem

Found during technical validation, in a throwaway `xterm.js` terminal inside WKWebView.

With a Chinese input method active, a full-width punctuation mark such as `？` — which the input method emits directly,
with no candidate window — **needs the key pressed twice**. The first press is swallowed. Composed CJK text, which goes
through candidate conversion, is not affected and lands correctly on the first try. Native macOS applications take the
punctuation on the first press, so the behaviour is the terminal's, not the input method's.

Everything below the composition layer was ruled out during validation: plain shifted ASCII symbols typed into the
window reach the PTY correctly, the same symbols sent straight to the daemon arrive intact, and the UI installs no
custom key handler. What remains is `xterm.js` 5.5.0's own composition handling in WKWebView.

## Key design decisions

- **Keep the framework's default composition handling; do not hand-roll one.** Writing a bespoke composition handler
  means owning input-method behaviour for every language and every macOS release, which is a far larger liability than
  the defect it would fix. Settled with the user.
- It follows that the fix has to come from one of: a newer `xterm.js` whose composition handling resolves it, a
  configuration option on the terminal, or an upstream fix. Which of the three applies is the open question below.

## Upstream findings (2026-10-05)

Desk research on the npm registry, the `xtermjs/xterm.js` repository and the WebKit bug tracker — not observed in
Octoboard itself — turned up enough to close part of the open question above; what follows is this round's lasting
conclusion.

The defect is already filed upstream, in our exact environment, as
[**xtermjs/xterm.js#6144**](https://github.com/xtermjs/xterm.js/issues/6144) (established) — macOS WKWebView via
Tauri 2, Apple Simplified Chinese (Pinyin), full-width `？`/`！`/`（` dropped on the first press and accepted from
the second on. It does not need a new report.

Of the three branches opened by "Key design decisions" above, two are now closed (established): no released
`xterm.js` version fixes it — the guard responsible is byte-identical in 5.5.0, 6.0.0 and current `master` — and no
terminal configuration option fixes it either, since `ITerminalOptions` has no composition- or IME-related option
in any released version or the current beta. Only "an upstream fix" remains open.

The root cause is a library bug, provoked by a WebKit event-ordering quirk (established). `xterm.js`'s
`_inputEvent` gates a direct IME commit on `_keyDownSeen`, a flag armed by every `keydown` — including a bare
`Shift` — and cleared only on `keyup`; the guard itself, in our pinned 5.5.0, is
`ev.data && ev.inputType === 'insertText' && (!ev.composed || !this._keyDownSeen) && !…screenReaderMode` at
`src/browser/Terminal.ts:1176`. On WebKit, the `input` for a Shift-held full-width punctuation commit arrives
*before* its own `keydown`, so the first press is read as already handled and dropped; the character's own `keyup`
disarms the flag in time for the second press to pass. The WebKit ordering quirk is real, but the drop is the
library's: the guard's ordering assumption alone explains the symptom, and #5374 reproduces the identical symptom
in plain Safari with no input method involved at all, which is the strongest evidence the fault is the library's
rather than the engine's. #5374 and #5894 each found Chromium hosts unaffected, though that is not universal — a
different reporter on #5887 found the same class of drop inside an Electron (Chromium) host — which if anything
strengthens rather than weakens the case against the library, since it shows the quirk is not tied to one engine.
Swapping the web engine would only mask the bug, not fix it.

The fix is known upstream, as [**#6200**](https://github.com/xtermjs/xterm.js/pull/6200) and
[**#6054**](https://github.com/xtermjs/xterm.js/pull/6054) (established): both make the same change — do not arm
`_keyDownSeen` for modifier-only keydowns — reusing the `wasModifierKeyOnlyEvent` helper that is already present in
our pinned 5.5.0. One caveat applies to any fix here: [#6045](https://github.com/xtermjs/xterm.js/issues/6045)
warns that relaxing this gate in isolation can turn dropped characters into *duplicated* ones on key rollover (fast
typing), so verifying a fix means checking for duplication as well as for the drop being gone.

## Plan

- [ ] Reproduce on the real desktop application once it exists, and confirm the defect is not an artefact of the
      throwaway validation prototype
- [ ] Localise it first: the bisection done during validation ruled out everything *below* the composition layer, but it
      did not separate the terminal library from the web engine. Run the same terminal build under another engine and
      compare, so the next step is not chosen on an assumption
- [ ] Establish whether a newer `xterm.js` than 5.5.0 fixes it, and whether any terminal option changes the behaviour
- [ ] If neither does, report it upstream with the reproduction, and record here what the upstream outcome was
- [ ] Once a fix exists, take it and verify against the completion criteria above — including that composed CJK text
      still works

## Mid-task handoff

This round did desk research only — the npm registry, the `xtermjs/xterm.js` repository (issues, pull requests,
release tags and their sources) and the WebKit bug tracker — nothing below was observed in Octoboard itself, so no
plan checkbox above is ticked. Most of them need a person at a Mac with a Chinese input method running the real
application, since scripted key injection bypasses macOS input methods and would prove nothing either way; the
exception is the plan item asking whether a newer `xterm.js` or a terminal option fixes it, which desk research
already answers from the sources above — it stays unticked here only because that answer has not been checked
against Octoboard's own build, not because it needs a person. Claims below keep the labels **established** (read
directly from a source), **plausible** (consistent with the evidence but not confirmed) or **searched, nothing
found**.

**Versions in use (established).** `app/package.json` declares `@xterm/xterm` `^5.5.0` and `@xterm/addon-fit`
`^0.10.0`, and `app/package-lock.json` pins them to exactly 5.5.0 and 0.10.0 — the only two `xterm.js` packages in
the project. The project is already on the renamed `@xterm/*` scope; the unscoped `xterm` package stopped at 5.3.0
and is deprecated on npm, and the scoped line begins at 5.4.0 (2024-03-01), so 5.5.0 (2024-04-05) is a scoped-era
release. Latest releases as of 2026-10-05: `@xterm/xterm` 6.0.0 and `@xterm/addon-fit` 0.11.0, both published
2025-12-22, with `beta` dist-tag `6.1.0-beta.304` (2026-08-30) — one major version behind, nothing released between
5.5.0 and 6.0.0.

**The guard, read from the published sources (established).** Delivery of a direct IME commit depends on a
fallback in `_inputEvent`, guarded by
`ev.data && ev.inputType === 'insertText' && (!ev.composed || !this._keyDownSeen) && !…screenReaderMode`.
`ev.composed` is always `true` for a browser-dispatched `input` event, so the guard reduces to `!_keyDownSeen`. The
guard line is byte-identical in all three places checked — 5.5.0 (`src/browser/Terminal.ts:1176`), 6.0.0
(`src/browser/CoreBrowserTerminal.ts:1196`) and current `master` (`CoreBrowserTerminal.ts:1033`) — and `_keyDown`
still sets `_keyDownSeen = true` unconditionally in all three. The IME fixes that *did* merge after 5.5.0 (#5698,
#5747, #5759, #5762) are all about composition *rendering and preedit placement*, not about this delivery path.

**Terminal configuration options: there is none (established).** `ITerminalOptions` was enumerated in the 5.5.0,
6.0.0 and `6.1.0-beta.304` typings — the beta is the one line worth checking separately, since it is the first to
add keyboard-adjacent options (`quirks: ITerminalQuirks`, and `vtExtensions: IVtExtensions` carrying
`kittyKeyboard` and `win32InputMode`), none of which existed in 5.5.0 or 6.0.0. None of the three touches
composition or IME behaviour. The only option that reaches the code path at all is `screenReaderMode`, and it
reaches it the wrong way round: the guard requires `!screenReaderMode`, so enabling it *disables* the `input`-event
fallback these characters depend on, which can only make the symptom worse.

**WebKit bug tracker (established, one plausible point).** [WebKit bug 25119, "IME modifies the DOM before
keydown"](https://bugs.webkit.org/show_bug.cgi?id=25119) describes exactly the inversion our symptom depends on,
filed 2009, still `NEW`, last activity 2017. [WebKit bug
165004](https://bugs.webkit.org/show_bug.cgi?id=165004) ("The event order of keydown/keyup events and composition
events are wrong on macOS") was resolved as a duplicate of [bug
311717](https://bugs.webkit.org/show_bug.cgi?id=311717), "Fix a regression and turn on correct composition event
ordering by default", `RESOLVED FIXED` (landed 2026-04, backported to `safari-7624-branch`) — but that fix is about
*composition* event ordering, and our characters produce no composition events at all, so it does not cover this
path; #6200's trace was captured on macOS 26 (the PR itself gives only the OS version; it opened 2026-10-02, so
roughly October 2026) and still reproduces, which suggests the shipped WebKit fix does not help here —
**plausible**, since we cannot confirm which WebKit build that reporter ran.

**PR state and merge cadence (established, one plausible point).** [**#6200**
"Don't arm the `_keyDownSeen` gate on modifier-only
keydowns"](https://github.com/xtermjs/xterm.js/pull/6200), opened 2026-10-02, unmerged, is the closest write-up to
ours of anything upstream: it names `？` via Shift+/ with Chinese Pinyin, reproduces in both Safari and
WKWebView/Tauri, cites #6144, #5887 and #5374 as the same mechanism, carries a test for the regular path, and is
explicit about what it does *not* claim to fix. [**#6054** "Don't set `_keyDownSeen` for pure modifier
keydowns"](https://github.com/xtermjs/xterm.js/pull/6054), opened 2026-07-16, unmerged, is the same fix, found via
Traditional Chinese (Zhuyin) full-width `「`, verified by the author against a real Tauri/WKWebView app through
`pnpm patch`. Both sit at `mergeable_state: blocked`; #6054's only comment is its author asking a maintainer to
approve the first-contributor CI workflow, in July, unanswered. For context on the odds: the repository has ~90
open pull requests, and the most recent merge of any kind is 2026-08-30 (#5879, an image-storage change unrelated
to input handling). Whether the keyboard/IME input path specifically has gone untouched for longer is less
clear-cut: the helper textarea did pick up an `autocomplete="off"` attribute in August (#6057), though nothing
nearer the `_keyDownSeen` guard itself — **plausible that the input path proper is stale since March 2026, not
established**. A maintainer did say "thanks for reporting, looking into it" on #5887 on 2026-08-25. One caveat the
PRs themselves raise: [#6045](https://github.com/xtermjs/xterm.js/issues/6045) warns that relaxing this gate in
isolation can turn dropped characters into *duplicated* ones in the fast-typing/key-rollover case, because
`CompositionHelper`'s deferred textarea diff has its own ordering assumption; #6200 deliberately scopes itself to
modifier-only keydowns to stay clear of that, and claims the rollover case is untouched — **plausible, not
verified by us**, and the thing worth watching when the patch is tested.

**Siblings and related-but-different reports — judged, not our defect.** [#5887](
https://github.com/xtermjs/xterm.js/issues/5887) (second character lost under rapid typing when an IME reports
`keyCode=229`) and [#6045](https://github.com/xtermjs/xterm.js/issues/6045) hit the *same guard* with a different
trigger — key rollover rather than a held modifier — so they are siblings, not our symptom, though #5887's thread
is not purely that: it also carries `ayii0111`'s held-modifier Zhuyin/Tauri report (2026-07-16) that became #6054,
which is why #6200 cites #5887 alongside #6144 and #5374 as the same mechanism and calls it "the held-modifier
variant" rather than treating it as unrelated.
[#5835](https://github.com/xtermjs/xterm.js/issues/5835) / [#5836](https://github.com/xtermjs/xterm.js/pull/5836)
(iOS Chinese IME punctuation dropped) and [#5614](https://github.com/xtermjs/xterm.js/pull/5614) (iOS Safari
Chinese punctuation) are the same family on the same engine but on iOS, and both propose different, broader fixes.
[#5894](https://github.com/xtermjs/xterm.js/issues/5894), against `6.1.0-beta.220` (WKWebView dead-key
cancellation), and
[#6065](https://github.com/xtermjs/xterm.js/issues/6065) / [#6066](https://github.com/xtermjs/xterm.js/pull/6066)
(`Ctrl+<letter>` and `Escape` dropped under a CJK IME) are different inputs through neighbouring code — #6066 is
worth remembering separately, because it describes `Ctrl+C` being swallowed while a CJK IME is active, which is
adjacent to the `Ctrl+C` workaround already carried. [#5374](https://github.com/xtermjs/xterm.js/issues/5374)
reproduces the *same* "first press swallowed, second press works" shape in plain Safari 18.5 on a Japanese keyboard
layout with no input method involved at all (Shift+3 for `#`), which is the strongest evidence that the fault is
the library's rather than the engine's. One cautionary precedent: [#6084](
https://github.com/xtermjs/xterm.js/issues/6084) was a confident WKWebView/Tauri CJK-input report that its author
retracted — the real cause was a missing `LANG`/`LC_CTYPE` for a process launched from Finder — a reminder of what
else can masquerade as an IME bug in this exact stack. Searched for and found nothing: no issue or PR upstream
specific to Octoboard, and no WebKit bug describing the `input`-before-`keydown` inversion for *direct,
composition-free* commits other than the 2009 bug 25119.

**Recommendation for the remaining steps (for whoever resumes, plan unchanged).** The "report it upstream" step
looks unnecessary — #6144 already carries our symptom, our environment and a root cause, and #6200/#6054 already
carry the fix; commenting on #6200 with a confirmation from our own build adds more than a new issue would (acted
on the same day — see "What was reported upstream" below). The "localise it first" engine comparison now buys
little: #5374 reproduces the same shape in plain Safari with no input method at all, and the guard that explains it
is in the library, not the engine — the Electron sighting on #5887 only reinforces that. The highest-value hands-on
step is to apply
#6200's one-line change to our pinned 5.5.0 and check, in the real application: `？` on the first press, composed
CJK text still correct, and — per #6045's warning — no duplicated characters when typing fast. Whether to carry a
patched dependency while the PR waits is the user's call.

**What was reported upstream, and what came back (2026-10-05).** Rather than filing a new issue, our data was
added to the two existing threads, with the user's authorisation: [#6144 comment](
https://github.com/xtermjs/xterm.js/issues/6144#issuecomment-5986424483) — that 5.5.0 is affected as well as
6.0.0, so this is not a 6.0.0 regression; the guard quoted from our installed 5.5.0 bundle; and what we ruled out
below the composition layer, so nobody repeats that bisection. [#6200 comment](
https://github.com/xtermjs/xterm.js/pull/6200#issuecomment-5986424653) — the same version-range point against the
PR that fixes it, stating plainly that we have not tested the patch, and asking whether a reporter can do anything
to unblock its CI approval. Both comments say explicitly that we confirm the symptom and the version range, not
the fix. **No upstream response yet** — that is the outcome still to be recorded against the plan's "report it
upstream" step, which is why that step stays unticked.

## Notes for developers

- **Development notes**: verifying this needs a person. Scripted key injection bypasses macOS input methods entirely,
  so an automated check would prove nothing about this path either way.
- **Reference docs**: the terminal requirements this sits under are in `docs/mvp.md` section 3, and the sibling
  keyboard and WebView pitfalls — including this defect's own entry — are in its "Known pitfalls of the Tauri / Rust
  approach" section.
