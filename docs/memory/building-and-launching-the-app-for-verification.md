# Building and launching the app for verification

## Launch the real window for any change on the startup or mount path

A webview that throws while mounting is a blank window — no message anywhere, and neither `tsc` nor a reviewer reading
the diff can see it; three review rounds here passed on code whose window never rendered once. So whenever a change
touches startup, the daemon-sidecar handover or the terminal's mount, run the app and capture the window, then confirm
the whole chain positively: the window paints, the sidecar starts, the webview connects, the menu fills, a selected
session's terminal renders. Capturing the window needs no Accessibility permission of its own, so this check is
always available and costs about a minute.

## Build the daemon and confirm what the running sidecar executes before trusting any verification of it

Launching the app in dev mode does not build the daemon: the dev command starts the frontend dev server only (for the
build step itself and when to rerun it, see the "Development" section of `apps/desktop/README.md`). In a worktree that
has never been built, the missing sidecar fails loudly at compile time; in one that has been built before, the launch
silently uses the binary already sitting there, and a verification of a daemon-side change then measures the *old*
behaviour and reports a pass or a failure that has nothing to do with the change. So build the daemon first, then
confirm positively that the running sidecar is executing it: take the content hash of the executable the live sidecar
process has open and compare it against the fresh build output. The sidecar directory the build script copies into is
not that executable — the process runs from a further copy under the Tauri crate's own target directory — so an
up-to-date sidecar directory on its own proves nothing about what is running.

## Read and stage what the shipped webview exchanges with the daemon through a wrapped sidecar in a packaged build

A dev-mode window does not answer what the shipped app puts on the wire: it loads from the dev server (`devUrl` in
`apps/desktop/src-tauri/tauri.conf.json`), so the `Origin` it sends differs from the packaged webview's. So anything
that depends on those requests — a check the daemon applies to them, a header the webview is expected to send — is
verified against a packaged build. A release build has no devtools, so observe the traffic between the webview and the
daemon instead, without touching the source: in a copy of the built `.app`, rename `Contents/MacOS/octoboardd` to
`octoboardd.real` and put an executable in its place that starts a logging TCP proxy in front of it. The proxy has to
print the daemon's `octoboardd listening on 127.0.0.1:<port>` line again with its own port, because the shell connects
to whatever port that line on the sidecar's stdout names. This relies on the build being unsigned, as the swap breaks
a signed bundle's signature.

The same proxy is how a real daemon error reply is put into the window's own request path, for an alert or a recovery
action that answers one: have it hold one specific request of the window's (an `open_session` for an agent), change
state through the real daemon on a separate control connection (remove that agent) while withholding from the window
the broadcasts that would show it the change first, then forward the original request unchanged, so the window
receives the daemon's genuine refusal (`agent_not_added`). A refusal provoked on the control connection itself never
reaches the window's alert, so it is no check of that alert, and neither is a race left to timing.

## Nothing the daemon logs once the application is gone is readable, and putting its log on a file changes what it does

Applies to verifying what the daemon does after the application exits, crashes or is killed. The daemon's log goes to
its stderr, and that stderr is a pipe whose read end the application holds and forwards to its own stderr
(`apps/desktop/src-tauri/src/sidecar.rs`), so on a packaged build the daemon's lines land wherever the application's
own output does. Every line the daemon writes after the application is gone therefore goes into a dead pipe and is
readable nowhere — and a `tracing` call added to diagnose such a path leaves no trace at all, as does a task that
panics on one, since Tokio swallows a task's panic.

Starting the daemon by hand with its stderr on a file does make those lines readable, but it also takes the broken
pipe away, which is part of the condition being investigated: a daemon that misbehaves under the application can
behave perfectly with its log on a file. So treat a stderr-on-a-file run as the control showing that the pipe is what
matters, not as the observation, and get the evidence off the path another way — in a throwaway build, have the code
under investigation write to a file of its own, around a `catch_unwind` where a panic is the suspicion.

## Build the bundle a verification runs against with `pnpm build:app`, never a bare `cargo build`

The frontend bundle the window loads (`packages/ui/dist`, which `apps/desktop/src-tauri/tauri.conf.json` names as
`frontendDist`) and the `octoboardd` sidecar under `src-tauri/binaries/` are both produced by that file's
`beforeBuildCommand`, and only the Tauri CLI runs it (`pnpm build:app`, run in `apps/desktop`, is that `tauri build`;
`--bundles app` passes through, the arguments known to move the output out of `target/release/bundle` are refused up
front, and anything else that moves it is caught after the build, by the bundle's age). A `cargo build` in
`src-tauri/` embeds whatever happens to be sitting in those two places — nothing at all, or the previous build's
output — so the window comes up blank, or on code that is not the code under test. Blank is the dangerous one: it is
indistinguishable from a webview that threw while mounting, so a build mistake reads as a defect in the change and
gets chased through the frontend instead.

`pnpm build:app` is also the only build that cannot sign (what it strips and refuses is in the "Release builds"
section of `apps/desktop/README.md`). Do not run a bare `pnpm tauri build` or `pnpm release` for a verification: the
Tauri CLI signs and notarizes with whatever `APPLE_*` the calling shell carries, with nothing in its output to say so —
a plain verification build has been Developer ID-signed and sent to Apple's notary service that way — and
`pnpm release` loads the credentials from the gitignored repo-root `.env.secret` itself, so wherever that file is
filled in, a shell carrying no `APPLE_*` is not a build that cannot sign. Nor strip the variables by hand.

Never run a bare `pnpm tauri build` or `pnpm tauri bundle` at all, for any purpose, on macOS: only the scripts
(`pnpm build:app`, `pnpm install:macos`, `pnpm release`) put the `actool` shim in `apps/desktop/scripts/actool/` on the
CLI's `PATH`. Without it, compiling the layered icon crashes `actool`'s `ibtoold` helper ("attempt to insert nil
object"), and the helper left running keeps failing every later `actool` call on the machine — the scripts' builds in
every worktree included — until `killall ibtoold`. So when a build reports that error, or a script reports the layered
icon missing although Xcode 26 is the selected developer directory, run `killall ibtoold` and rebuild through a script
rather than chasing the Xcode install the message suggests; killing the helper beforehand does not make a bare build
safe.

## Launch a built app with `open`, never by exec'ing its binary

An app exec'd from a shell is registered with the system but never activated, and WebKit leaves its page quiescent
for as long as it stays that way — measured here as a WebContent process whose CPU time did not move at all across
four seconds, and started accruing within a second of an `open -a` on that same process bringing it to the front. So
anything read off an exec'd app about launch timing, first paint, or what is on screen is a reading of a frozen page.
`open` still gives the run everything the shell gave it: `--env VAR=value` per variable to override, `-o` and
`--stderr` for the app's own output, `-n` to force a second instance.

## Isolate a packaged verification with a build-time app identifier and a throwaway environment

On macOS, a throwaway `HOME` and `TMPDIR` isolate the daemon, but do not isolate WKWebView. The daemon keeps its
database and instance lock (`daemon.lock`) under `$HOME/.octoboard` and its port file (`octoboardd.port`) under
`$TMPDIR`. WKWebView still writes under the real user's `~/Library/WebKit/<bundle identifier>/`, including
`localStorage` and content rule lists. Every normal build shares the installed app's `dev.octoboard.desktop` identifier.
An empty daemon project list can prune per-project preferences in that shared profile even when the verification
never opens their settings, so do not launch a normal build expecting fresh `HOME` alone to protect the user's app.

For an independent packaged verification, build from source with a unique identifier and product name in `apps/desktop`:

```sh
pnpm build:app --bundles app --config \
  '{"identifier":"dev.octoboard.verify.<unique-run>","productName":"Octoboard Verification"}'
```

Changing a copied bundle's `Info.plist` after building is not an equivalent route: a re-identified
copy has failed to show a window. Copy the freshly built bundle to a scratch location, never over the installed app,
and check both its `CFBundleIdentifier` and code-signing identifier before launching. If the linker left a generic
ad-hoc signing identifier, ad-hoc sign only the scratch copy with the chosen identifier
(`codesign --force --deep --sign - --identifier <identifier> <scratch-app>`), then require
`codesign --verify --deep --strict <scratch-app>` to pass. This does not require a Developer ID or notarization.

Launch that exact scratch path with `open -n` and a clean environment: fresh `HOME`, `TMPDIR`, `ZDOTDIR`, XDG
directories and agent config directories, with a controlled `PATH`. Set each override through `open --env`; do not
assume the Launch Services process inherits the caller's environment, nor that an `LSEnvironment` in the bundle's
`Info.plist` is applied: on the first launch of a rebuilt clone Launch Services ignored its `HOME`, and the daemon
started against the real home and hit the user's instance's occupied lock. Put test projects inside the throwaway home
(`$HOME/code/<project>`), not beside it: the trust dialog cannot offer a folder checkbox for a project whose parent is
or contains that home (see the "Trusted folders" section of `docs/product/folder-trust.md`).

Before driving the window, verify isolation positively: the running app executes the scratch bundle, its daemon owns the
scratch database and lock and listens on its own port, and its WebKit processes have their local storage and content
rule lists open under the unique identifier. Check the installed app's PID and executable separately so an automation
target cannot silently select its window. If any of these checks fails, stop the verification rather than moving,
restoring or editing the installed app's profile. Never run two bundles with the same identifier at once: an earlier
build retained for comparison carries it too, shares its WebKit profile, and leaves `open -b` and an accessibility
target free to pick either, so stop one before launching the other. During cleanup, stop only the verification's PIDs
and remove only its scratch bundle, data, and identifier-specific WebKit/cache/defaults artifacts; real-user Library
artifacts are outside the throwaway `HOME` and need to be included explicitly.

To test other system languages, pass them for that launch only:
`open <scratch-app> --args -AppleLanguages '(zh-Hans-CN, en)'`. This steers only a profile with no
`octoboard.language` key: the first launch stores its choice and later launches use that value. Use a fresh unique
profile when testing the initial pick. To inspect a profile value, open the sqlite file normally rather than with
`immutable=1`: the app's last write may still be in the WAL, which `immutable=1` skips.

Keep the `.app` and throwaway directories on the internal disk when the checkout is on a removable volume. An ad-hoc
build has a fresh code identity each time, so macOS's consent prompt for access to that volume can recur on every run.

## Reload a dev window fully before judging a defect in it

Vite's hot update applies module by module, and when one module cannot be Fast Refreshed the `pnpm tauri dev` window
can keep running a mix of old and new code with nothing on screen to say so. Before treating something seen in a dev
window as a defect of the current code, force a full reload (touching `packages/ui/index.html` does it).

## Running dev apps share the machine: confirm which one answers, stop yours by PID, warn before touching `src-tauri`

Several worktrees are often running at once, each with its own `packages/ui` dev server and `octoboardd`. The dev
server's port is fixed (5174, `strictPort`), so a second one started in the background fails to bind. The URL then
keeps answering with the other worktree's build, and the verification passes or fails against code it never ran.
Confirm that the process listening on the port is the one you started, or start yours with `--port` on a free port.
When cleaning up, stop only processes you started, by PID, never with `pkill` / `killall`: a pattern such as
`pkill -f target/debug/octoboardd` also matches every other worktree's daemon, and a looser one has killed the daemon
of the user's own running app, interrupting their sessions. The same goes for the gallery: `pkill -f vite` or
`pkill -f gallery-sweep` stops every other worktree's gallery server and sweep along with yours. For a dev server the
PID to stop is the one listening on its port (`lsof -ti tcp:<port> -sTCP:LISTEN`), not the one recorded when it was
started: `pnpm exec vite` and `pnpm dev` alike run Vite as a child process, and stopping the `pnpm` PID leaves Vite
serving on the port.

A `pnpm tauri dev` app running from the worktree you edit rebuilds and restarts itself on every change under
`apps/desktop/src-tauri/`, and the restart interrupts every session in it: the daemon exits with the app and ends the
agents (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`), which may include the very
agents doing the editing. So when the user has that app running, tell them before editing Rust there and land the Rust
edits together rather than one at a time.
