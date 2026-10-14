# Verifying behaviour in the desktop app

## Keep a verification as narrow as the change

The user has objected to drawn-out testing. Verify in the app only the behaviour this change altered, in a short,
targeted run — the states, languages or screens the change actually touches, not a tour of the rest or a matrix of
every combination — and leave what a unit test or the type checker already covers to them.

A fallback written for an environment this machine does not have is settled by reading the code, not by finding that
environment: a browser too old for an event the code works around, a platform the build does not target. The user has
said as much — a compatibility detail of that kind wants its logic confirmed, not a device. What a real run is for is
what only the real thing can answer: the shell's own dialogs, a volume that is really unmounted, how long something
really takes, whether a window really paints.

Changing a shared component's markup, layout or timing widens what the change touches to every surface already
using it, so those surfaces are inside this narrow scope rather than outside it: making `MarkedPath`'s root a `span`
for one new caller collapsed the path boxes in the Settings rows, and neither the type checker, the unit tests nor a
gallery pass over the new caller's own surface showed it. Settle such a change by putting each existing call site
side by side against the merge base — a second gallery server run from a branchless temporary worktree at that
commit (the `git worktree add --detach` case in the "Creating" section of
`.claude/skills/git-worktree/SKILL.md`) — rather than judging the new rendering on its own.

Where what the change alters is the data a fixture answers with rather than any markup, settle it on the replies instead
of on renderings: dump every scenario's reply for the affected requests to JSON at the merge base and on the branch and
diff the two dumps. That names which field of which reply moved, which no screenshot comparison can, and it costs one
scripted run rather than two servers and a capture pass. Where renderings do have to be compared, no scenario is
byte-identical between two runs, and how far from identical is a property of that scenario rather than of the capture
pass: an animation moves a few dozen pixels in one small region (the session-status glyph), while a floating HeroUI
overlay lands a couple of dozen CSS pixels away from one render to the next, and a lazily loaded renderer may not have
painted at all when the capture is taken — either of those moves tens of thousands of pixels, more than a real change to
the same scenario does. So measure that floor before reading any diff as a regression: capture the whole set twice at
the same commit, and count a scenario as changed only where its branch-against-base diff stands above its own
same-commit diff.

## Look at a UI state in the gallery before staging it through a daemon

To see how the UI renders in a given state — connection loss, a session status, a dialog, the archive, report pages,
toasts, a narrow window, a language or the right-to-left layout — open it in the UI state gallery
(`packages/ui/README.md`, section "The UI state gallery") rather than building the state up against a running daemon.
Where no scenario shows the state, do not write one yourself: hand it to the `agent-gallery` skill, which dispatches
the subagent that owns the gallery and the criteria for what a scenario is worth carrying (`CLAUDE.md`, section
"Dispatching subagents"). The gallery is the real `App` over a fixture daemon in the Vite dev server, so it settles
rendering, layout and copy and nothing more: it has no real daemon timing, protocol round trips or agent terminal, and
it is not the app's WKWebView. A pointer interaction or a WebKit rendering question is still checked with the gallery
page loaded in Playwright's `webkit`, a paint defect still in the real window, and behaviour that depends on the
daemon, the shell or how the window starts up still in the real app.

For batch checks that scenarios still run, follow `packages/ui/README.md`, section "Sweeping the gallery", for the
scope and when a run is needed. A page with no errors does not establish that its mocks supply what the current
components need: verify the fields, data conditions and contracts behind a new UI state against the code. Use the
browser where the actual appearance or interaction needs checking, within the change's scope.

How the bundle loads what it fetches at run time — lazy chunks, a worker, WebAssembly, assets — and anything a
build-only Vite plugin changes are not settled there either: the gallery runs Vite's unbundled dev modules from
`http://localhost`, while the app loads the built `dist/` from `tauri://localhost`. Check those in a packaged build
(`pnpm build:app`). For UI the app does not render yet, build that from a temporary detached worktree (the
`git worktree add --detach` case in the "Creating" section of `.claude/skills/git-worktree/SKILL.md`) whose `main.tsx`
renders the gallery's scenarios in place of the app.

The gallery's "File viewer" group hands `FileViewer` a fixed subject instead of driving the app, so it carries none of
what the project browser supplies while running — above all the loader for a change's whole bodies, which is the only
thing that makes the renderer offer to expand the lines a diff collapsed. A diff's expansion is therefore never
exercised there, however right the scenario looks: verify it in the "Project pane: Git" group, whose fixture change
supplies those bodies, and when handing a new change presentation to the `agent-gallery` skill, say that it has to be
shown in that group.

That group also stands in for the app around the viewer, in both directions, and only running its pages shows a
mismatch. The contexts the app would provide are wired for that branch alone in `packages/ui/src/gallery/frame.tsx`, so
a viewer component that starts calling a hook needing one — anything that reaches the daemon, a toast included — throws
on mount in every scenario of the group, with the type check and the unit tests still green. The other way round, its
subjects are literals standing in for what the project browser's own hooks build, so a field those hooks fill only
under a condition (a `ViewerSubject`'s `source`, which only the change reader sets, and only for a comparison) is left
unset in every fixture that does not meet that condition, read off the hook that builds the subject rather than off
what the header happens to draw. So open the group's pages for any change to the viewer or to what it depends on.

## Only `zh-Hans` shows translated copy; the other 15 languages render English

Applies to any check whose subject is translated text: reading a screen under another language, a gallery scenario's
step that names a control, a test that asserts on what a surface says. Only `en` and `zh-Hans` have catalogs
(`CATALOGS` in `packages/ui/src/i18n/catalog.ts`; `docs/product/language.md`, section "What is translated so far"), so
each of the other 15 offered languages renders English word for word. Anything resting on English copy — a name written
out as English text, a label that never reached the catalog, a layout that only fits English — passes under every one of
them, so a sweep across languages is no broader than a run in English. Make such a check `zh-Hans`, and expect no
language to show text longer than English's, since none of the longer-worded ones is translated. Text direction is not a
catalog's (`textDirection` in `packages/ui/src/i18n/languages.ts`), so `ar` still settles mirroring.

## A fixture daemon answers as fully as the real one, or the gallery shows a state no daemon sends

Applies whenever a daemon reply gains a field the UI draws from, and whenever a gallery scenario still shows the old
behaviour after a fix. The fixture daemon (`packages/ui/src/gallery/fixtureDaemon.ts`, and `fixtureGit.ts` for a
project's Git mode) answers every scenario from what that scenario's fixture happens to hold, so a field the fixture
cannot supply goes out as the real daemon's "not known" value and the UI draws the fallback for it. A complete fix can
therefore leave every scenario showing what it showed before, with nothing in the type checker or the unit tests to say
which side is wrong: the viewer's bar for the lines past a diff's last hunk went on claiming more might follow because
no fixture change carried both sides' whole text for the new line count to be taken from.

So a reply that gains such a field is taught to the fixture together with the condition the real daemon fills it under,
and every scenario's fixture is given what that condition needs — the scenarios whose fixture answers with an error
included, since they answer the reply as well, and an unfilled field there is a state the real daemon never sends. Take
that condition from the daemon's own code and tests — never from an enumeration of cases in a doc or a plan, and never
by importing a helper out of the UI's own source that happens to compute the same thing: the fixture stands on the
daemon's side of the protocol, so it repeats the rule inside `packages/ui/src/gallery/` rather than depending on UI code
that is free to change for the UI's reasons. An enumeration can lag the rule it enumerates, and the first implementation
of the refusal below, built on one, was wrong for the case the rule covers and the list did not (a binary change, whose
patch has no hunk and whose two sides are nonetheless both counted, from the bodies the reply carries).

Hold that condition in a signature before holding it at run time. A repository's change fixtures are written through a
factory per kind of change rather than as literals (`packages/ui/README.md`, section "The UI state gallery"), so a field
the reply gains goes first into the parameters of the factories whose kind the real daemon fills it for: an unfilled
field is a legal value of the reply, so nothing but a parameter that has to be passed stops a fixture being written that
cannot be answered faithfully. Leave to the run-time refusal below only what no signature can settle — a field whose
condition is a property of the data rather than of the kind of thing the fixture is.

Do not hold that condition by remembering it. Where the real daemon would fill the field and the fixture carries nothing
to fill it from, the fixture daemon refuses the request rather than answering with the field unfilled, and the refusal
is painted over the page as well as rejected to the caller, because the window swallows a failed read into its own
error line and the scenario would otherwise read as merely broken. Such a refusal is deliberately not a daemon error:
scenarios answer requests with a daemon error on purpose and those are legitimate UI states under test, so a gap in the
gallery's own data must never be dressed as one. Extend that refusal to each new field of this kind, and have the guard
test over the fixture daemon's replies read every scenario's reply — including the ones that carry no such field — so a
refusal fails the test run too and not only the scenario someone opens by hand. And before reading a scenario as a fix
that did not work, check that its fixture can produce the input the fix reads at all. A handler for a request no
scenario ever sends falls outside all of this — no scenario renders it and no guard test reads it — so write one only
once a scenario makes that request, and delete one left unused rather than keeping it for completeness; until then it
is an unfaithful reply nothing can catch, waiting for whoever adds that scenario first.

A fixture field says what a reply costs, never whether it is given. Whether the daemon answers a request at all follows
from the change's own data — its group, its sides, whether making its patch already failed — so the fixture daemon reads
it from there, and the daemon error it refuses with is one the real daemon sends for that data. An optional field saying
how a reply is answered (the delay before it, the error in place of it) is carried by exactly the scenarios that need
something special, which makes its presence look like the condition for answering at all: gating on `expand` refused a
deletion, a binary change and a change past the patch budget with an `invalid_change` the real daemon never sends there.
A factory parameter likewise carries the content a reply needs, not the right to ask for it.

## A final capture does not verify earlier transient states

Applies when visually verifying gallery interactions. A capture taken after the scenario's steps shows only the state
they leave on screen: a second hover displaces the first tooltip, even when both steps' text assertions pass. Keep those
assertions, but do not count an earlier transient state as visually checked from the final capture.

Where several such states need visual inspection, reach and capture each on the retained page during verification.
Scenario selection follows `packages/ui/README.md`, section "The UI state gallery"; needing several captures does not
require several navigation entries.

## A scenario's steps run in every language, so no matcher spells out the catalog's own punctuation

Applies to the matchers a gallery scenario's steps find a control by name with (`ui.press`, `ui.hover*`,
`ui.focusControl` in `packages/ui/src/gallery/`). The page renders every scenario in every offered language, so a
matcher written against the English catalog finds nothing in another one and the scenario fails there — loudly, but
only in the language that hits it, which the default run never reaches. Matching a rail avatar as the console's name
followed by `", "` broke in `zh-Hans`, whose catalog writes the full-width `，`, while the English capture was clean.

- Where the matcher can be built inside a step, which has `ui.t`, take the text from the catalog message: format it
  with a sentinel in place of every placeholder and split on that, as `Ui.session` does in `interact.ts`.
- A matcher defined at module level has no `t`, so it accepts the name alone or the name followed by a punctuation
  character (`/^\p{P}/u`), never a literal separator. That still tells the avatar of a console named `Operations`
  from a row for `Operations hub`, where the next character is a space.
- Check a new or changed matcher by rendering its scenario in a language other than English, not only in the default
  one; and when a matcher changes, check the other scenarios that use it in that language too.

## Settle what `git` writes against real `git` in a throwaway repository

Applies to `git` output written out by hand rather than taken from a run of `git`: a patch in a gallery change fixture
or a viewer fixture, an expected patch in a viewer test. Nothing here can see such a literal being wrong — the type
check sees a string, a unit test compares it against itself, and the gallery renders whatever it says — so it is worth
no more than the reading of whoever wrote it. Build the case in a throwaway repository, run the command the daemon runs
for it, and take the output from there.

Two premises had stood in these fixtures as literals for a long time, and both were wrong: that `git` writes zeros in a
patch's `index` line for a side it read from the worktree disk (it hashes the file; it is `git diff --raw` that writes
zeros for that side), and that a rename whose content is unchanged has an empty patch (`git` writes the rename's headers
with no hunk; only a change at one path whose two sides are equal is empty).

## Stage a defect's states in the order it was reported, not in whatever order is easy to reach

The order the states are brought up in decides the outcome on its own often enough — whether an element was already
mounted when an overlay opened, what held focus when a region appeared, which of two things observed the other — that
a run reaching the same final state by another route proves nothing about the route that was reported. A gallery
scenario here raised the connection banner and then opened a dialog over it, where the defect had been filed the other
way round; every measurement passed, and the fix it passed still failed in the packaged app. So when reproducing a
defect or verifying its fix, put the steps in the order the report gives, and where a scenario cannot reach the state
that way, extend it until it can rather than reporting the pass the other order gives.

## Check the page in WebKit with real pointer input, and with long and CJK text

The app renders in WKWebView, so a pass in Chrome says nothing: a HeroUI tag's remove button that ignored the mouse in
WebKit passed a scripted click in Chrome. Run a browser check of the page in Playwright's `webkit` and drive the pointer
through `page.mouse` rather than a click dispatched from script. Playwright is a pinned devDependency of `packages/ui`,
so `pnpm install` is all such a check needs from the repository; the dependency downloads no browser of its own, and the
browser is fetched explicitly with `pnpm exec playwright install webkit`, run from `packages/ui` — from the workspace
root `pnpm exec playwright` resolves to a machine-global Playwright or to nothing, and would silently fetch the wrong
revision. Headless WebKit still is not the app for how things paint (it has no GPU compositing), so a rendering defect
is judged in the real window. Where a change puts text in a constrained layout, include a long text and a CJK one:
overflow defects there showed only with long text. Judge a colour or a dimming from computed styles or full-resolution
pixels, not from a downscaled screenshot. A packaged build offers no DOM to measure, so settle a geometry question there
from ink extents read off a capture at the display's native resolution, allowing for its scale factor (a Retina display
gives 2 image pixels per CSS pixel), and settle a question of overlap or reachability with a real click at the measured
point, since a capture only ever shows what is on top. Colours read off such a capture carry the display's own profile,
so a contrast ratio measured from one differs from the ratio the tokens give by a few tenths, either way: it is good for
confirming which side of a threshold a pair lands on, and never for correcting a token-derived ratio recorded in the
stylesheet or the accessibility standard.

A scrollbar's own footprint is one layout question that check cannot answer, however much else it settles: Playwright's
WebKit draws overlay scrollbars only — headless and headed, at `scrollbar-width` `auto` or `thin`, and even with macOS
set to show scroll bars always — so a scroller over content larger than its box on both axes reports
`offsetWidth - clientWidth` and `offsetHeight - clientHeight` of 0 there. Anything about something sitting under,
beside, or dimmed by a scrollbar that takes space therefore shows nothing wrong in that check, and has to be measured
in real Safari on the gallery page, or in the app. That wants the system set to show scroll bars always
(`defaults write -g AppleShowScrollBars -string Always`), which is honoured — but only by a browser started
afterwards: a Safari that was already running when the setting changed keeps drawing overlay scrollbars and reads as
no defect. Read the old value before writing it and put it back afterwards, it being the user's own machine-wide
setting.

## Rule out a locked screen before reading anything off a window capture

A locked screen and a sleeping display look identical from the outside: under either one every application reports
zero accessibility windows and `screencapture` returns bare wallpaper, so the app reads as having rendered nothing at
all and a whole verification pass can be spent chasing that. `caffeinate -d` prevents the display sleeping but neither
prevents nor reverses a lock, and `caffeinate -u` does not bring the session back. Tell the two apart with
`ioreg -n Root -d1 -r | grep CGSSessionScreenIsLocked` before concluding anything from a capture; unlocking needs the
user, so ask.

A capture aimed at the app's own window is no safer, and misleads where a full-screen one at least looks obviously
wrong: under lock `screencapture -l <CGWindowID>` comes back with a plausible window image rather than wallpaper,
because the suspended WebContent process leaves its last frame standing — two captures minutes apart were
pixel-identical across a run whose window state had changed throughout. So make the `ioreg` check before every
capture, not only when one comes back empty.

## Get a diagnosis out of a blank window by rendering it into the DOM

The window's own pixels are the only readable output channel: devtools can only be opened by the keystroke injection
that needs Accessibility permission, and the native window title is set by the Rust shell and does not follow
`document.title`. So install a `window.onerror` / `unhandledrejection` handler that writes the error and its stack
straight into the document body, and read it off a screenshot. The React error boundary does not remove the need for
this — it catches render-time throws only, while a throw from a library's own scheduled work, or from module-level
code running before React mounts, still leaves a silent blank window.

## Bisect every terminal symptom against the daemon before blaming the agent

When the app shows a terminal symptom — a keystroke doing nothing, input dying after a session switch, a pane looking
stale — send the same bytes straight to the daemon's terminal WebSocket and compare before concluding anything. On
screen a UI defect and ordinary agent behaviour are indistinguishable: "Ctrl+C is being swallowed above the terminal",
"the UI rebound the read path on a session switch but not the write path" and "this agent simply ignores Esc in this
state" all look exactly alike, and only the side-by-side comparison tells them apart. Do this before filing a symptom
against an agent or against `xterm.js`.

## Watch for a report page's blocked navigation on the window, not inside the frame

What stops a page in the panel from navigating its frame somewhere external is the *embedder's* `frame-src`, so WebKit
reports the refusal to the embedder: the `securitypolicyviolation` event fires on the window's own document and the
frame's document never sees it. Instrument the window — a listener there is both the only way to observe a page
attempting to leave and a standing health signal for the window's own policy.

## A network-level probe of this window needs its positive control from outside the app

While the window ships a restrictive CSP (`default-src 'none'`), every channel a probe would normally use as its
positive control — a `fetch`, an `<img>`, an `<iframe>`, a WebSocket — is refused before any name is resolved, so a
capture that comes back empty says nothing about whether the app can reach the network. Generate the control outside
the app: Safari's lookups leave through the same WebKit networking path.

## A scripted GUI probe can only confirm a positive, never a negative

A key combination that fails to arrive may be the probe's fault rather than the app's: through `osascript` / System
Events, `keystroke "<letter>" using control down` can be dropped silently where the same combination sent as
`key code <n> using control down` arrives. `cliclick`'s key presses (`kp:`) do not reach this app's window at all,
where the same key sent as `key code` moves it immediately. Prefer `osascript` with `key code`, and never conclude
"the app swallows this key" from a scripted probe without confirming by hand.

Its clicks (`c:`) are dropped often enough on this window's HeroUI controls that a press has to be set up and then
checked: move the pointer onto the control (`m:`) and leave it there before pressing, and confirm from a capture that
the press took effect before taking the next step. An in-app dropdown of the page's own (not a native `<select>`) does
open by pointer, but is reliably chosen from only with the keyboard.

macOS mangles typed text of its own accord as well: with automatic correction and capitalization on (the default),
text sent character by character into a packaged app's search field arrived as `Note` for `note` and `ran k` for
`rank`, and the field's first Escape then reverted the correction instead of clearing the field. So type a probe's
text as `key code` presses with a short delay between them, and read a garbled value, or an Escape that did not
clear, as the correction rather than as a defect in the field.

The page's HeroUI controls depend on input modality and timing in ways scripted input easily misses, in the app and
in a plain browser alike. A tooltip opens on focus only after a real Tab key press (`element.focus()` or a click
leaves it closed), and on hover only once HeroUI's `--tooltip-delay` (1500 ms in its default theme) has passed with
the pointer resting on the control — unless another tooltip closed less than 500 ms ago, react-aria's warm-up being
one flag shared by every tooltip in the window, in which case it opens at once, so a tooltip that came up instantly
by hand says nothing about its own delay and a check of what one waits starts from a window where none has been
shown. A toast's close button takes pointer events only while the toast is hovered. So a tooltip that did not
appear, or a control a script could not press, is not a finding until real input reproduces it.
A Tab walk, on the other hand, needs no change to the system's keyboard-navigation setting: unlike Safari by default,
this app's WKWebView moves Tab onto buttons whether `AppleKeyboardUIMode` is set or not, so a button that Tab skips
is not explained by that setting.

A mark inside a row moves as the pointer arrives, so never send the pointer to a point fixed before that row is
hovered, whether that is coordinates measured off a capture of it un-hovered or the mark's own box read out of the page
in a browser check. The rows of the sidebar, of focus mode, of the change list and of the avatar field keep their
controls (a "+", an actions menu) at zero width until the row is hovered, holds focus or is selected; revealing them
widens the row's end by some 50 px and shifts every mark before them — an activity marker, a git badge — that far
towards the start. A pointer aimed at the resting position therefore lands on a revealed control and answers with its
tooltip, which reads exactly like the mark having no tooltip of its own. Approach through the row and then onto the
mark in a second move, as a user does, or re-measure the target while the pointer already rests in the row.

Two further limits on macOS: driving the real app this way requires Accessibility permission granted to the host
application of whatever runs the script, and native `<select>` popups cannot be driven through the accessibility tree
at all — to make a control scriptable, build it from something other than a native `<select>`, or drive it by hand.

Where a check needs a real client against a real daemon but not the native window, the built UI is one: serve
`packages/ui/dist` on a local port and load it with `?port=<daemon port>` (the "Pointing it at a daemon" section of
`packages/ui/README.md`) in a headless browser driven over the DevTools protocol, with real mouse and key events. The
daemon treats it as any other client, so every daemon-side outcome and everything the agent in a session sees is
exercised, and no screen-recording or accessibility grant is needed for it; what it leaves out is the Tauri shell's own
event path, which carries no daemon traffic anyway (the "External interfaces" section of `apps/desktop/README.md`).
That is enough to confirm what a dialog does. It is not the app's WebKit, so it settles behaviour and never rendering.

So where a check has to run in the real app, keep the GUI out of the setup: create the consoles, projects and sessions a
verification needs by sending the daemon's own protocol requests, and drive only the behaviour under test through the
window. A session's status can be set up the same way, without an agent prompt or a model turn: POST
`{"hook_event_name":"PermissionRequest"}` to the daemon's `/hook/<session>` to raise its hand, and a later
`UserPromptSubmit` lowers it again. Trusted folders are the exception: the protocol adds one only as the answer to a
live agent's trust confirmation, so seed them by inserting rows into the `trusted_directories` table of
`$HOME/.octoboard/octoboard.db` while the app is stopped.

Keystrokes go to whichever application is frontmost, and a click at screen coordinates to whichever window is on top
at that point, not to Octoboard. Activate it and confirm it is the frontmost process immediately before every
keystroke and every click: a `Cmd+Q` or `Cmd+W` that lands on another application closes the user's own work. One check
does not cover a sequence — another application came forward between two steps of one, and the click and the Enter
after it edited a file open in the user's editor — and a capture taken after the input is the only way to know where it
landed, so take one between steps rather than reading the next step's result as proof of the last.

It cuts the other way too: whatever is typed while Octoboard sits frontmost lands in it. Left frontmost on an agent's
sign-in screen, stray keystrokes completed a real Codex sign-in through the default browser, which was already
signed in to the account. So hand focus back after each GUI step, and never leave the window frontmost on a sign-in
screen.

## Prove that no key reached the terminal against an agent that echoes every byte

When a verification has to show that keys the UI handles itself — in a pane, a viewer, a dialog over the terminal —
never reach the selected session, run that session on a stand-in agent CLI that writes back everything it reads, not
on a real agent. A real agent shows nothing for many keys (an arrow, Escape or Enter at an empty prompt), so a key
that leaked looks the same as one that did not, and the check passes whatever the UI does; against the echo, any leak
shows up in the terminal.

## Input-method and reduced-motion checks need the user's hands

Injected keystrokes bypass macOS input methods entirely, so a scripted CJK composition test passes without ever
exercising the IME and proves nothing. Ask the user to type it and report what they saw.

It cuts the other way too: scripted typing can instead be fed *through* whatever input source is active —
`cliclick t:` composes through a pinyin IME rather than typing the literal text — so any probe that types has to
switch the input source to a non-IME one (ABC) first and put it back afterwards.

Reduce motion cannot be switched on from a shell either: `defaults write com.apple.universalaccess reduceMotion` is
refused, and `defaults write com.apple.Accessibility ReduceMotionEnabled` succeeds but WebKit does not read it — the
app keeps animating after a relaunch, which reads as the page ignoring `prefers-reduced-motion`. For a check in the
app, ask the user to turn on System Settings → Accessibility → Display → Reduce motion, and off again afterwards.

## `SIGSTOP` on the WebContent process simulates a hung page, but only roughly

`kill -STOP` on the app's WebContent process freezes the page without a debug build. It is not the same state as a
real hang, though: a double `Cmd+Q` did not escape it, while scripted AppleEvent quits did. Do not read a `Cmd+Q`
that fails to quit under `SIGSTOP` as a verdict on the wedged-window escape hatch.

Every WebKit application's page process has the same name (`com.apple.WebKit.WebContent`), so find Octoboard's own
from `launchctl print pid/<app pid>`, never by name — stopping the wrong one freezes the user's browser.
