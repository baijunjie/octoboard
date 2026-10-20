# Driving and reading the real app window

## Rule out a locked screen or a window not on screen before reading anything off a capture or the tree

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

With the screen unlocked, the same blank reading comes from an instance that has no window on screen — hidden into
the background, or not brought up since it launched: its accessibility tree exposes only the native menu bar and the
status item's menu, `AXWindows` holds no web window, and a capture of its window comes back black. That is neither a
missing Accessibility or Screen Recording grant nor a missing tool. Reopen the instance by its bundle identifier
(`open -b <identifier>`; a reopen brings a hidden window back through `show_main_window`), then `AXRaise` its window
and make it `AXMain`; the WebKit accessibility tree appears once the window is showing. Capture only that window,
with `screencapture -l <CGWindowID>` and the ID taken from `CGWindowListCopyWindowInfo` filtered to the instance's own
PID.

## Get a diagnosis out of a blank window by rendering it into the DOM

The window's own pixels are the only readable output channel: devtools can only be opened by the keystroke injection
that needs Accessibility permission, and the native window title is set by the Rust shell and does not follow
`document.title`. So install a `window.onerror` / `unhandledrejection` handler that writes the error and its stack
straight into the document body, and read it off a screenshot. The React error boundary does not remove the need for
this — it catches render-time throws only, while a throw from a library's own scheduled work, or from module-level
code running before React mounts, still leaves a silent blank window.

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
Nor is an accessibility action's return code a verdict: `AXPress` on the status item returned
`kAXErrorCannotComplete` although its popup did open, so read what an action did off the tree or a capture, never off
the call's result.

## Drive the real app without touching the user's work

Where a check needs a real client against a real daemon but not the native window, the built UI is one: serve
`packages/ui/dist` on a local port and load it with `?port=<daemon port>` (the "Pointing it at a daemon" section of
`packages/ui/README.md`) in a headless browser driven over the DevTools protocol, with real mouse and key events. The
daemon treats it as any other client, so every daemon-side outcome and everything the agent in a session sees is
exercised, and no screen-recording or accessibility grant is needed for it; what it leaves out is the Tauri shell's own
event path, which carries no daemon traffic anyway (`apps/desktop/README.md`, the opening under its title "Octoboard
desktop application").
That is enough to confirm what a dialog does. It is not the app's WebKit, so it settles behaviour and never rendering.

So where a check has to run in the real app, keep the GUI out of the setup: create the consoles, projects and sessions a
verification needs by sending the daemon's own protocol requests, and drive only the behaviour under test through the
window. A session's status can be set up the same way, without an agent prompt or a model turn: POST
`{"hook_event_name":"PermissionRequest"}` to the daemon's `/hook/<session>` to raise its hand, and a later
`UserPromptSubmit` lowers it again. Trusted folders are the exception: the protocol adds one only as the answer to a
live agent's trust confirmation, so seed them by inserting rows into the `trusted_directories` table of
`$HOME/.octoboard/octoboard.db` while the app is stopped.

While the user is working on the machine, send the GUI steps to the instance under test alone: post key and pointer
events to its process (`CGEventPostToPid`), placed from that instance's own window frame, and bring its window forward
through accessibility (`AXRaise`, `AXMain`) rather than by activating the application. That leaves the real cursor,
the frontmost application and the user's typing where they were. Do not use global input for it — System Events
keystrokes, `cliclick`, moving the real pointer — and send a system-wide shortcut (Command-F5 for VoiceOver, for one)
only once the user has explicitly authorised it.

Global input, where it is used at all, follows the frontmost application instead. Keystrokes go to whichever application
is frontmost, and a click at screen coordinates to whichever window is on top at that point, not to Octoboard. Activate
it and confirm it is the frontmost process immediately before every keystroke and every click: a `Cmd+Q` or `Cmd+W` that
lands on another application closes the user's own work. One check does not cover a sequence — another application came
forward between two steps of one, and the click and the Enter after it edited a file open in the user's editor — and a
capture taken after the input is the only way to know where it landed, so take one between steps rather than reading the
next step's result as proof of the last.

It cuts the other way too: whatever is typed while Octoboard sits frontmost lands in it. Left frontmost on an agent's
sign-in screen, stray keystrokes completed a real Codex sign-in through the default browser, which was already
signed in to the account. So hand focus back after each GUI step, and never leave the window frontmost on a sign-in
screen.

## Input-method, reduced-motion and VoiceOver-off steps need the user's hands

Injected keystrokes bypass macOS input methods entirely, so a scripted CJK composition test passes without ever
exercising the IME and proves nothing. Ask the user to type it and report what they saw.

It cuts the other way too: scripted typing can instead be fed *through* whatever input source is active —
`cliclick t:` composes through a pinyin IME rather than typing the literal text — so any probe that types has to
switch the input source to a non-IME one (ABC) first and put it back afterwards.

Reduce motion cannot be switched on from a shell either: `defaults write com.apple.universalaccess reduceMotion` is
refused, and `defaults write com.apple.Accessibility ReduceMotionEnabled` succeeds but WebKit does not read it — the
app keeps animating after a relaunch, which reads as the page ignoring `prefers-reduced-motion`. For a check in the
app, ask the user to turn on System Settings → Accessibility → Display → Reduce motion, and off again afterwards.

VoiceOver can be started from a script, but only its own toggle, Command-F5, turns it off cleanly. Quitting the
VoiceOver process leaves its enabled flag (`voiceOverOnOffKey`) set, and macOS refuses a direct write that would clear
it, so the machine goes on reporting VoiceOver as on. Command-F5 is a system-wide shortcut, so ask the user to press it,
or to authorise sending it, rather than quitting the process to end a listening pass.

## `SIGSTOP` on the WebContent process simulates a hung page, but only roughly

`kill -STOP` on the app's WebContent process freezes the page without a debug build. It is not the same state as a
real hang, though: a double `Cmd+Q` did not escape it, while scripted AppleEvent quits did. Do not read a `Cmd+Q`
that fails to quit under `SIGSTOP` as a verdict on the wedged-window escape hatch.

Every WebKit application's page process has the same name (`com.apple.WebKit.WebContent`), so find Octoboard's own
from `launchctl print pid/<app pid>`, never by name — stopping the wrong one freezes the user's browser.
