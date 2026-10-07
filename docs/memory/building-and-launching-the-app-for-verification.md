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
build step itself and when to rerun it, see the "Development" section of `apps/desktop/README.md`). In a worktree that has never
been built, the missing sidecar fails loudly at compile time; in one that has been built before, the launch silently
uses the binary already sitting there, and a verification of a daemon-side change then measures the *old* behaviour and
reports a pass or a failure that has nothing to do with the change. So build the daemon first, then confirm positively
that the running sidecar is executing it: take the content hash of the executable the live sidecar process has open and
compare it against the fresh build output. The sidecar directory the build script copies into is not that executable —
the process runs from a further copy under the Tauri crate's own target directory — so an up-to-date sidecar directory
on its own proves nothing about what is running.

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

`pnpm build:app` also strips every `APPLE_*` variable from the build's environment and fails if the `.app` comes out
Developer ID-signed, so there is nothing to remember to unset. The reason it exists: the Tauri CLI reads `APPLE_*`
straight from the environment (see the "Release builds" section of `apps/desktop/README.md`), and while the shell
exported them, a plain verification build was Developer ID-signed and submitted to Apple's notary service three
separate times, with nothing in the command or its output to warn of it — unreleased code sent to Apple, minutes added
to the build, and a signed bundle that the wrapped-sidecar swap above breaks. One of the three came from an agent
stripping the variables with `UNSET=$(...)` and `env $UNSET ...`, which does nothing in zsh because zsh does not
word-split an unquoted parameter. Do not run a bare `pnpm tauri build` for a verification: a shell that carries
`APPLE_*` would sign it again.

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
of the user's own running app, interrupting their sessions.

A `pnpm tauri dev` app running from the worktree you edit rebuilds and restarts itself on every change under
`apps/desktop/src-tauri/`, and the restart interrupts every session in it: the daemon exits with the app and ends the
agents (see "Crashes and forced termination" in `docs/product/application-lifecycle.md`), which may include the very
agents doing the editing. So when the user has that app running, tell them before editing Rust there and land the Rust
edits together rather than one at a time.
