# Verifying behaviour in the desktop app

## Keep a verification as narrow as the change

The user has objected to drawn-out testing. Verify in the app only the behaviour this change altered, in a short,
targeted run — the states, languages or screens the change actually touches, not a tour of the rest or a matrix of
every combination — and leave what a unit test or the type checker already covers to them.

## Launch the real window for any change on the startup or mount path

A webview that throws while mounting is a blank window — no message anywhere, and neither `tsc` nor a reviewer reading
the diff can see it; three review rounds here passed on code whose window never rendered once. So whenever a change
touches startup, the daemon-sidecar handover or the terminal's mount, run the app and capture the window, then confirm
the whole chain positively: the window paints, the sidecar starts, the webview connects, the menu fills, a selected
session's terminal renders. Capturing the window needs no Accessibility permission of its own, so this check is
always available and costs about a minute.

## Build the daemon and confirm what the running sidecar executes before trusting any verification of it

Launching the app in dev mode does not build the daemon: the dev command starts the frontend dev server only (for the
build step itself and when to rerun it, see the "Development" section of `apps/desktop/README.md`). In a worktree that has never
been built, the missing sidecar fails loudly at compile time; in one that has been built before, the launch silently
uses the binary already sitting there, and a verification of a daemon-side change then measures the *old* behaviour and
reports a pass or a failure that has nothing to do with the change. So build the daemon first, then confirm positively
that the running sidecar is executing it: take the content hash of the executable the live sidecar process has open and
compare it against the fresh build output. The sidecar directory the build script copies into is not that executable —
the process runs from a further copy under the Tauri crate's own target directory — so an up-to-date sidecar directory
on its own proves nothing about what is running.

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

## Read what the shipped webview sends to the daemon off a packaged build, through a wrapped sidecar

A dev-mode window does not answer what the shipped app puts on the wire: it loads from the dev server (`devUrl` in
`apps/desktop/src-tauri/tauri.conf.json`), so the `Origin` it sends differs from the packaged webview's. So anything
that depends on those requests — a check the daemon applies to them, a header the webview is expected to send — is
verified against a packaged build. A release build has no devtools, so observe the traffic between the webview and the
daemon instead, without touching the source: in a copy of the built `.app`, rename `Contents/MacOS/octoboardd` to
`octoboardd.real` and put an executable in its place that starts a logging TCP proxy in front of it. The proxy has to
print the daemon's `octoboardd listening on 127.0.0.1:<port>` line again with its own port, because the shell connects
to whatever port that line on the sidecar's stdout names. This relies on the build being unsigned, as the swap breaks
a signed bundle's signature.

## Build the bundle a verification runs against with `pnpm tauri build`, never a bare `cargo build`

The frontend bundle the window loads (`packages/ui/dist`, which `apps/desktop/src-tauri/tauri.conf.json` names as
`frontendDist`) and the `octoboardd` sidecar under `src-tauri/binaries/` are both produced by that file's
`beforeBuildCommand`, and only the Tauri CLI runs it. A `cargo build` in `src-tauri/` embeds whatever happens to be
sitting in those two places — nothing at all, or the previous build's output — so the window comes up blank, or on
code that is not the code under test. Blank is the dangerous one: it is indistinguishable from a webview that threw
while mounting, so a build mistake reads as a defect in the change and gets chased through the frontend instead.

Run that build with every `APPLE_*` variable unset (`env | grep '^APPLE_'` shows what the shell carries; drop each
with `env -u <name>` on the build command). The Tauri CLI reads them straight from the environment (see the "Release
builds" section of `apps/desktop/README.md`), and a shell set up to release this app has them exported, so a
verification build otherwise gets Developer ID-signed and uploaded to Apple's notary service with nothing in the
command or its output to warn of it — unreleased code sent to Apple, minutes added to the build, and a signed bundle
that the wrapped-sidecar swap above breaks.

## Launch a built app with `open`, never by exec'ing its binary

An app exec'd from a shell is registered with the system but never activated, and WebKit leaves its page quiescent
for as long as it stays that way — measured here as a WebContent process whose CPU time did not move at all across
four seconds, and started accruing within a second of an `open -a` on that same process bringing it to the front. So
anything read off an exec'd app about launch timing, first paint, or what is on screen is a reading of a frozen page.
`open` still gives the run everything the shell gave it: `--env VAR=value` per variable to override, `-o` and
`--stderr` for the app's own output, `-n` to force a second instance.

## A throwaway `HOME` and `TMPDIR` isolate the daemon's data, not the webview's

Point `HOME` and `TMPDIR` at fresh directories for every verification run. The daemon keeps its database and instance
lock under `$HOME/.octoboard` and its port file under the temp directory (see the "Pointing it at a daemon" section of
`packages/ui/README.md`), so on the real `HOME` a run either works on the user's own data or, while the user's own
Octoboard is running, loses the lock and opens the window on the `?error=` startup screen instead of the app.
The daemon then takes that directory as the user's home, and the app offers less around it: a project whose parent
folder is or contains it gets no "Trust parent folder" (see the "Trusted folders" section of
`docs/product/launching-agents.md`). So put project directories inside the throwaway `HOME` (`$HOME/code/<project>`),
not beside it in one scratch directory.

WKWebView ignores both variables, though: it keeps the page's `localStorage` under the *real* user's
`~/Library/WebKit/<bundle identifier>/WebsiteData/`, so everything the page persists — the appearance choice
included — carries over from the previous run, and a run on a fresh `HOME` is not a fresh profile at all. The user's
own installed Octoboard and every worktree's build share `dev.octoboard.app`, so that directory is the user's own
profile. Isolating the run means moving it aside before and putting it back afterwards, which needs the user's
go-ahead up front and is possible only while no app with that identifier is running: check with
`lsappinfo find bundleid=dev.octoboard.app` (empty output means none) immediately before moving it. Otherwise run on
the shared profile, note each setting the run will change (the appearance and the language first of all) beforehand,
and put it back afterwards, because the user's own app reads the same values.

The app's defaults domain is shared the same way, so to run it under other system languages pass them for that launch
only — `open <bundle>.app --args -AppleLanguages '(zh-Hans-CN, en)'` — and never `defaults write dev.octoboard.app
AppleLanguages`, which the user's own app then reads too. This steers only a launch whose profile has no
`octoboard.language` key yet: that launch picks the language from the system's languages and stores it, and every
later launch ignores them. To test that pick, note the key's value and remove it first, and restore it afterwards.
To read a value out of the profile, open the sqlite file normally rather than with `immutable=1`: the app's last
write may still be sitting in the WAL, which `immutable=1` skips, answering with the value before it.

Keep the `.app` and both throwaway directories on the internal disk when the checkout is on a removable volume. A
build here is ad-hoc signed and therefore has a fresh code identity every time, so macOS's consent prompt for
reaching files on such a volume comes back at the user on every single run and no grant it is given ever applies to
the next build.

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

So keep the GUI out of the setup: create the consoles, projects and sessions a verification needs by sending the
daemon's own protocol requests, and drive only the behaviour under test through the window. A session's status can be
set up the same way, without an agent prompt or a model turn: POST `{"hook_event_name":"PermissionRequest"}` to the
daemon's `/hook/<session>` to raise its hand, and a later `UserPromptSubmit` lowers it again. Trusted folders are the
exception: the protocol adds one only as the answer to a live Claude Code trust screen, so seed them by inserting
rows into the `trusted_directories` table of `$HOME/.octoboard/octoboard.db` while the app is stopped.

Keystrokes go to whichever application is frontmost, and a click at screen coordinates to whichever window is on top
at that point, not to Octoboard. Activate it and confirm it is the frontmost process immediately before every
keystroke and every click: a `Cmd+Q` or `Cmd+W` that lands on another application closes the user's own work.

## Running dev apps share the machine: confirm which one answers, stop yours by PID, warn before touching `src-tauri`

Several worktrees are often running at once, each with its own `packages/ui` dev server and `octoboardd`. The dev
server's port is fixed (5174, `strictPort`), so a second one started in the background fails to bind. The URL then
keeps answering with the other worktree's build, and the verification passes or fails against code it never ran.
Confirm that the process listening on the port is the one you started, or start yours with `--port` on a free port.
When cleaning up, stop only processes you started, by PID. A pattern kill such as `pkill -f target/debug/octoboardd`
also matches every other worktree's daemon.

A `pnpm tauri dev` app running from the worktree you edit rebuilds and restarts itself on every change under
`apps/desktop/src-tauri/`, and the restart interrupts every session in it: the daemon exits with the app and ends the
agents (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`), which may include the very
agents doing the editing. So when the user has that app running, tell them before editing Rust there and land the Rust
edits together rather than one at a time.

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

`kill -STOP` on the app's WebContent process freezes the page without a debug build, and the window's close button then
does nothing, as with a real hang. It is not the same state, though: a double `Cmd+Q` did not escape it, while
scripted AppleEvent quits did. Do not read a `Cmd+Q` that fails to quit under `SIGSTOP` as a verdict on the
wedged-window escape hatch.

Every WebKit application's page process has the same name (`com.apple.WebKit.WebContent`), so find Octoboard's own
from `launchctl print pid/<app pid>`, never by name — stopping the wrong one freezes the user's browser.
