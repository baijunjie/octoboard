# wkbench: UI latency in the system WKWebView

`pnpm bench:wkwebview` (run in `packages/ui`) measures how long the real UI takes to answer the user's input —
switching console, attaching a session's terminal, scrolling it — and what one style recalculation and layout cost per
region of the page. It runs the production bundle in the **system WKWebView**, the engine the desktop app runs in,
because nothing else gives the real number: style recalculation there is several times slower per element than in
Playwright's headless WebKit (which reports about a fifth of it) and slower still than in Chrome, so a cause that
dominates in the app can look negligible in both. Use it for every performance change: run it on the base and on the
branch, and compare the two tables.

## Prerequisites

- macOS with the Xcode command line tools (`swiftc` compiles `host.swift`).
- Rust, for the daemon (`cargo build --release -p octoboardd`); `~/.cargo/bin` is added to the build's `PATH`. Pass
  `--daemon <path>` to use a binary you already have.
- Python 3 at `/usr/bin/python3`, for the stand-in agent.
- `pnpm install` from the workspace root (Playwright is only used for the `--engine` and `--profile` options).
  `--engine webkit` also needs Playwright's own WebKit: `pnpm exec playwright install webkit`. `--engine chromium`
  and `--profile` use the installed Chrome instead.

## Running it

```sh
pnpm bench:wkwebview                              # switch (with attach), scroll and restyle, 3 rounds
pnpm bench:wkwebview --scenarios switch --reps 5
pnpm bench:wkwebview --skip-build                 # reuse the bundle and host binary of the last run
pnpm bench:wkwebview --scratch ~/wkbench-main     # keep a run's scratch directory somewhere of your choosing
pnpm bench:wkwebview --probe                      # who forces style and layout (unminified bundle)
pnpm bench:wkwebview --scenarios none --profile attach   # Chrome CPU profile by source file and component
```

`pnpm bench:wkwebview --help` lists every option. `--probe` runs the whole scenario on the unminified bundle with every
layout read timed, so its tables are for attribution, not for comparison with a run without it. A window opens and
takes focus for the length of a scenario: WebKit runs no frames for a page nobody can see, so a hidden window would
measure a frozen page. Leave the machine alone while it runs. A first run also builds the daemon and the bundle, which
takes minutes; on a machine whose endpoint-security software inspects every process spawn, each step can stall for
seconds, which is normal.

Each scenario is limited to 150 s plus 30 s per `--reps`, and the run gives up on a page that posts nothing for 150 s
(longer than any one step of the driver's own timeouts). The page posts a frame count every 5 s, so the message says
whether it never loaded or loaded and then ran no frames (a window nobody sees gets none).

The page is always opened with `languages=en`: the app picks its language from the system's otherwise, and the lookups
rely on its English labels.

To measure a daemon of your own, start `octoboardd` on a scratch `HOME`, seed it with `node fixtures.mjs <port> <home>`
(the `home` holds the stand-in projects and must have the stand-in on its login shell's `PATH` as `claude`, as the
run's scratch `HOME` does), then run with `--port <port>`. That `HOME` must be outside `--scratch`: a run with `--port`
leaves the scratch `home/` and `tmp/` alone, but any other run recreates them. The harness does not stop that daemon,
but it drives its sessions and, under `idle`, renames and restores the titles of the extra projects' sessions: never
use a daemon that holds real work.

Everything the run makes goes under `--scratch` (default `packages/ui/.bench`, ignored by git): the throwaway `HOME` and
`TMPDIR`, the daemon build (`target/`), the bundles (`dist/`, `dist-debug/`), the host binary and the logs
(`daemon.stdout`, `daemon.stderr`, `cargo-build.log`, `vite-build-*.log`). `home/` and `tmp/` are recreated each run.
To delete a scratch directory by hand, `chmod -R u+rwx` it first: the daemon leaves read-only git hook directories
under `home/` that `rm -rf` cannot remove.

### What it does, in order

1. Compiles `host.swift`, builds the production bundle with `vite build`.
2. Starts `octoboardd` on the scratch `HOME` and `TMPDIR` with a clean environment, whose login shell puts
   `standin-claude.py` first on `PATH` as `claude`, so no real agent is launched.
3. Seeds the fixtures over the daemon's WebSocket protocol (`fixtures.mjs`): three consoles with one driven project
   and two sessions each, plus extra projects whose sessions stop drawing after 3 s, to give the sidebar a realistic
   size. It then waits `spinner_secs + 2` s, so the driven sessions' spinners have stopped and their replays no
   longer change.
4. Serves the bundle statically (never the dev server) on a free port and runs each scenario in the host.
5. Prints the tables, then stops the host, the static server and the daemon.

## What it measures

The in-page scripts post one JSON line per step. `instrument.js` only observes (frames, WebSocket traffic, React
commits); `driver.js` dispatches the user's real input (pointer events on a control, wheel events on the terminal).
The app has no benchmark hook.

| Scenario  | What it does |
|-----------|--------------|
| `switch`  | Selects a session (warm-up), then per round: console avatar to Console 2, attach `p2-s1`, attach `p2-s2` (same console), avatar to Console 3, attach `p3-s1`, avatar to Console 1, attach `p1-s1`. |
| `scroll`  | Attaches `p2-s1`, then wheel-scrolls its terminal up and down for 3 s each, one event per frame. |
| `restyle` | Calibrates one forced style recalculation and layout per region: sets a custom property and changes the padding by 1px on the region, then reads `offsetHeight`. |
| `idle`    | Not in the default set. The page shows a console for 5 s while `event-pump.mjs` renames sessions on the daemon, as a busy board's broadcasts do. |

### Reading the tables

For `switch` and the first steps of `scroll` and `idle` (all in ms, the median of the rounds):

- **toIdle**: from the press to the last busy frame. A frame later than 34 ms after the one before it means the main
  thread was busy, and the step is over once there has been none for 500 ms. It never comes out lower than where that
  wait began, which is after the next frame and, for an attach, after the terminal changed: a step that never blocks
  reads as the time to those. For an attach the step also waits for the terminal to show the target session: the
  stand-in prints its pid once, in its status line, after its spinner has stopped redrawing, so the pid tells sessions
  apart and that the end of the replay has been parsed. `min` and `max` are the spread across rounds. A `(!)` marks a
  row with a round that measures nothing: an attach whose terminal never changed within 30 s, or a step the main
  thread was not idle after within 20 s. Such rounds are left out of the row's statistics.
- **press JS**: the synchronous part, from the press to React's commit of the discrete update.
- **forced style+layout**: a `document.body.offsetHeight` read right after the press, the style and layout still dirty
  once the handlers have run.
- **next frame**: from the press to the next frame's timer, after style, layout and paint.
- **commits**: React commits between the press and idle. **worst frame gap** and **gaps >50 ms** (the count of frame
  gaps over 50 ms) describe the pacing of the whole step.

Scroll and idle rows show frame pacing over the driven window. The restyle table lists each region's element count and
the cost of one pass (a custom property and 1px of padding changed on the region, so style recalculation of everything
under it plus the layout that depends on it): multiply by the number of forced passes an interaction makes (each
synchronous layout read after a DOM change is one) to judge it, and make the restyle cheap first.

Run-to-run noise is easily 10 to 20 percent, more with other apps busy: compare medians of at least 3 rounds, on the
same machine, with nothing else running.

## Other engines and attribution

`--engine webkit|chromium` runs the same two scripts in Playwright (`playwright.mjs`). Read their numbers as ratios
between two builds, never as the app's own cost. The page is a 1440 x 872 viewport at device scale 2 there; the host's
window is 1440 x 900 points including its title bar, so layouts can differ by a few pixels. Chrome is the
installed one (`--chromium <path>` or `OCTOBOARD_CHROMIUM` for another). `--profile avatar|attach` records a Chrome CPU
profile of those steps and prints self time by source file and function and inclusive time by React component
(`profile.mjs`, through the source maps of an unminified build). Use the system WKWebView for how long, Chrome for which
code.

## Adding a scenario

1. In `driver.js`, add a branch on `scenario` that ends in `log({ done: true })`, using `step(name, target, attach)` for
   a timed press (`attach` waits for the terminal to change session) and `P.summarize(t0, t1)` for frame pacing;
   `P.card`, `P.avatar` and `P.press` find and press the app's controls.
   Anything the scenario needs from the page must come from the DOM or the daemon's fixtures, never from a hook in
   the app. Needing more fixtures means a change in `fixtures.mjs` (a stand-in's behaviour per project is its
   `.standin.json`: `lines`, `spinner_secs`, `fps`, `winch_reprint`, `append_every`). Give every session a finite
   `spinner_secs`: a spinner that never stops keeps filling the daemon's replay buffer, and the replay a step measures
   would change with the run's age.
2. Add its name to `SCENARIOS` in `run.mjs` and a branch in `printResults` for how its lines become a table (a
   scenario whose steps carry `pressJs` is already printed by the generic one). A separate script, like `domcost.js`,
   is wired in `runScenario`'s `script` choice.

## Safety rules

- The harness drives **its own throwaway daemon** only: a scratch `HOME` and `TMPDIR`, a clean environment, the stand-in
  as the only agent. It never looks for, attaches to or reads a running daemon. The one exception is `--port <n>`,
  which the operator passes explicitly; the harness then starts, seeds and stops nothing, but it drives that daemon's
  sessions and, under `idle`, renames and restores some of their titles.
  Never point `fixtures.mjs` at a daemon that is not throwaway: it creates consoles and sessions and trusts folders.
- It stops only the processes it started, by their PIDs (the builds, the daemon, the host, the event pump), on
  success, on failure and on Ctrl-C. Do not `pkill` or `killall` by name: other worktrees' daemons and the user's own
  app match the same patterns.
- `--scratch` must be empty or a previous scratch directory of this tool (it carries a `.wkbench-scratch` marker);
  anything else is refused, since `home/` and `tmp/` are deleted at the start of a run.
- The host's web data store is non-persistent, so a run leaves nothing in the real user's WebKit profile
  (`~/Library/WebKit/<bundle id>/`) — unlike a run of the built app.

## Files

| File | Role |
|------|------|
| `run.mjs` | The entry point: build, start, seed, serve, run, print, clean up. |
| `host.swift` | The Cocoa host: a `WKWebView` on a URL with `instrument.js` at document start and a driver after load. |
| `instrument.js` | Observation hooks, the heartbeat, the lookups and press, the main-thread-idle wait, the terminal-change wait, `?probe=1` forced-layout attribution. |
| `driver.js` | The `switch`, `scroll` and `idle` scenarios. |
| `domcost.js` | The `restyle` calibration. |
| `fixtures.mjs` | Consoles, projects and sessions over the daemon's protocol. |
| `standin-claude.py` | The stand-in agent: a transcript, a spinner region redrawn for a while, then its pid once. |
| `event-pump.mjs` | Synthetic daemon broadcasts for `idle`; restores the titles it changed when stopped. |
| `stall.mjs` | The 150 s silence watchdog, which reads the heartbeats to say what stalled. |
| `playwright.mjs`, `profile.mjs` | The same scripts in Playwright; the Chrome CPU profile. |
