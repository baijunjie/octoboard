# Verifying behaviour in the desktop app

## Keep a verification as narrow as the change

The user has objected to drawn-out testing. Verify in the app only the behaviour this change altered, in a short,
targeted run — the states, languages or screens the change actually touches, not a tour of the rest or a matrix of
every combination — and leave what a unit test or the type checker already covers to them.

Changing a shared component's markup or layout widens what the change touches to every surface already using it, so
those surfaces are inside this narrow scope rather than outside it: making `MarkedPath`'s root a `span` for one new
caller collapsed the path boxes in the Settings rows, and neither the type checker, the unit tests nor a gallery pass
over the new caller's own surface showed it. Settle such a change by putting each existing call site side by side
against the merge base — a second gallery server run from a branchless temporary worktree at that commit (the
`git worktree add --detach` case in the "Creating" section of `.claude/skills/git-worktree/SKILL.md`) — rather than
judging the new rendering on its own.

## Look at a UI state in the gallery before staging it through a daemon

To see how the UI renders in a given state — connection loss, a session status, a dialog, the archive, report pages,
toasts, a narrow window, a language or the right-to-left layout — open it in the UI state gallery
(`packages/ui/README.md`, section "The UI state gallery") rather than building the state up against a running daemon;
if the state has no scenario yet, add one and look at that. The gallery is the real `App` over a fixture daemon in
the Vite dev server, so it settles rendering, layout and copy and nothing more: it has no real daemon timing, protocol
round trips or agent terminal, and it is not the app's WKWebView. A pointer interaction or a WebKit rendering question
is still checked with the gallery page loaded in Playwright's `webkit`, a paint defect still in the real window, and
behaviour that depends on the daemon, the shell or how the window starts up still in the real app.

How the bundle loads what it fetches at run time — lazy chunks, a worker, WebAssembly, assets — and anything a
build-only Vite plugin changes are not settled there either: the gallery runs Vite's unbundled dev modules from
`http://localhost`, while the app loads the built `dist/` from `tauri://localhost`. Check those in a packaged build
(`pnpm build:app`). For UI the app does not render yet, build that from a temporary detached worktree (the
`git worktree add --detach` case in the "Creating" section of `.claude/skills/git-worktree/SKILL.md`) whose `main.tsx`
renders the gallery's scenarios in place of the app.

## Stage a defect's states in the order it was reported, not in whatever order is easy to reach

The order the states are brought up in decides the outcome on its own often enough — whether an element was already
mounted when an overlay opened, what held focus when a region appeared, which of two things observed the other — that
a run reaching the same final state by another route proves nothing about the route that was reported. A gallery
scenario here raised the connection banner and then opened a dialog over it, where the defect had been filed the other
way round; every measurement passed, and the fix it passed still failed in the packaged app. So when reproducing a
defect or verifying its fix, put the steps in the order the report gives, and where a scenario cannot reach the state
that way, extend it until it can rather than reporting the pass the other order gives.

## Check the page in WebKit with real pointer input, and with long and CJK text

The app renders in WKWebView, so a pass in Chrome says nothing: a HeroUI tag's remove button that ignored the mouse
in WebKit passed a scripted click in Chrome. Run a browser check of the page in Playwright's `webkit` and drive the
pointer through `page.mouse` rather than a click dispatched from script. Headless WebKit still is not the app for how
things paint (it has no GPU compositing), so a rendering defect is judged in the real window. Where a change puts text
in a constrained layout, include a long text and a CJK one: overflow defects there showed only with long text. Judge
a colour or a dimming from computed styles or full-resolution pixels, not from a downscaled screenshot. A packaged build
offers no DOM to measure, so settle a geometry question there from ink extents read off a capture at the display's
native resolution, allowing for its scale factor (a Retina display gives 2 image pixels per CSS pixel), and settle a
question of overlap or reachability with a real click at the measured point, since a capture only ever shows what is
on top.

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
where the same key sent as `key code` moves it immediately; its clicks (`c:`) are fine. Prefer `osascript` with
`key code`, and never conclude "the app swallows this key" from a scripted probe without confirming by hand.

macOS mangles typed text of its own accord as well: with automatic correction and capitalization on (the default),
text sent character by character into a packaged app's search field arrived as `Note` for `note` and `ran k` for
`rank`, and the field's first Escape then reverted the correction instead of clearing the field. So type a probe's
text as `key code` presses with a short delay between them, and read a garbled value, or an Escape that did not
clear, as the correction rather than as a defect in the field.

The page's HeroUI controls depend on input modality and timing in ways scripted input easily misses, in the app and
in a plain browser alike. A tooltip opens on focus only after a real Tab key press (`element.focus()` or a click
leaves it closed), and on hover only once HeroUI's `--tooltip-delay` (1500 ms in its default theme) has passed with
the pointer resting on the control; a toast's close button takes pointer events only while the toast is hovered. So a
tooltip that did not appear, or a control a script could not press, is not a finding until real input reproduces it.
A Tab walk, on the other hand, needs no change to the system's keyboard-navigation setting: unlike Safari by default,
this app's WKWebView moves Tab onto buttons whether `AppleKeyboardUIMode` is set or not, so a button that Tab skips
is not explained by that setting.

Two further limits on macOS: driving the real app this way requires Accessibility permission granted to the host
application of whatever runs the script, and native `<select>` popups cannot be driven through the accessibility tree
at all — to make a control scriptable, build it from something other than a native `<select>`, or drive it by hand.

So where a check has to run in the real app, keep the GUI out of the setup: create the consoles, projects and sessions a verification needs by sending the
daemon's own protocol requests, and drive only the behaviour under test through the window. A session's status can be
set up the same way, without an agent prompt or a model turn: POST `{"hook_event_name":"PermissionRequest"}` to the
daemon's `/hook/<session>` to raise its hand, and a later `UserPromptSubmit` lowers it again. Trusted folders are the
exception: the protocol adds one only as the answer to a live agent's trust confirmation, so seed them by inserting
rows into the `trusted_directories` table of `$HOME/.octoboard/octoboard.db` while the app is stopped.

Keystrokes go to whichever application is frontmost, and a click at screen coordinates to whichever window is on top
at that point, not to Octoboard. Activate it and confirm it is the frontmost process immediately before every
keystroke and every click: a `Cmd+Q` or `Cmd+W` that lands on another application closes the user's own work.

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
