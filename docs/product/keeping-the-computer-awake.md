# Keeping the computer awake

The daemon can keep the computer it runs on — the daemon's host — from sleeping while the user is idle, through the
**Prevent sleep** setting and its **When to prevent sleep** choice in Settings' App behavior section (the rows
themselves are described in "Prevent sleep" in `docs/product/app-behavior-settings.md`). Prevent sleep is off to begin with. Here,
as in the setting's own words, "the computer" is the daemon's host: for a client connected to a daemon on another
machine, that machine, not the one the client runs on.

## Which computers can be held awake

- **macOS** and **Windows**: always.
- **Linux**: only where logind answers on the system bus. The daemon asks once, as it starts and before it answers any
  client, waiting at most **5 seconds**; a bus that does not answer in that time counts as no. The answer stands until
  the daemon ends.
- Any other host: never.

Where the host can be held awake, every client offers the setting — the desktop application and a browser alike;
where it cannot, no client shows the rows. The values are still stored there, and an import or a console session's
settings tools can still change them (see "Where the choice is stored" below), but they hold nothing. A client learns
which it is from `snapshot.host.keep_awake`.

## When the computer is kept awake

While Prevent sleep is on, the daemon keeps the computer awake according to When to prevent sleep:

- **Always**: for as long as the daemon runs. In the desktop application the daemon starts and stops with the
  application, so this is for as long as Octoboard runs — with its window shown or closed into the background; a
  daemon started on its own holds until it is stopped (see "The daemon on its own" in
  `docs/product/server-form.md`).
- **While sessions work**, the choice to begin with: while at least one session, in any console or project, is
  *starting*, *working*, *dormant* or *working in background*, a *paused* one that still has work running in the
  background included (see `docs/product/session-statuses.md`). A session *waiting for the user* does not count, nor
  does one otherwise at its prompt — so this is the set that holds a quit up (see "Quitting" in
  `docs/product/application-lifecycle.md`) without the sessions waiting for the user. Once no session counts, the
  computer is left to sleep by the system's own settings.

The hold is the daemon's own: it is taken from the daemon's start, with no window open and no client connected, and
does not wait for one. Turning the setting off, or a change of the choice or of the sessions that ends the need,
releases the hold at once. The daemon's ending in any way — a quit, a stop, a crash — releases it with it, the system
dropping it along with the process. When the desktop application starts a new daemon after one died (see "Starting up"
in `docs/product/application-lifecycle.md`), nothing is held in between, and the new daemon takes the hold again once
its condition is met. On the screen shown when the daemon failed to start there is no daemon, so nothing is held.

When the host refuses the hold — on Linux, logind refusing the lock or the system bus not answering within **5
seconds** — nothing is held and the daemon logs why. It does not ask again until the need has ended and come back: the
setting turned off and on, the choice changed, or, under While sessions work, no session counting and then one
counting again.

## What is held

Only the sleep that idleness brings is held off: the system does not go to sleep while the user is idle. Closing a
laptop's lid, or putting the computer to sleep by hand, still sleeps it. Every hold is named "Octoboard is keeping the
computer awake".

- **macOS**: two power assertions, one against idle display sleep and one against idle system sleep
  (`PreventUserIdleDisplaySleep` and `PreventUserIdleSystemSleep`), so the display does not turn off, and the screen
  does not lock, while the user is idle either. `pmset -g assertions` lists both under the daemon's process,
  `octoboardd`.
- **Windows**: one power request for the display and the system, so the display does not turn off either, as
  `powercfg /requests` lists it. The display part is best effort: a session with no display (a service, or an SSH
  session in Session 0) refuses it, and the request then holds the system alone, which still counts as held; the
  daemon's log records the refusal.
- **Linux**: a logind inhibitor lock on `idle`, in `block` mode, taken over the system bus under the name Octoboard,
  as `systemd-inhibit --list` lists it. Besides it, when a session bus is reachable and something on it serves
  `org.freedesktop.ScreenSaver`, the daemon inhibits that screen saver too, under the same name and reason: that is
  what keeps a desktop's own idle blanking, screen lock and automatic suspend from acting, where the desktop honours
  it. This part is best effort: a session bus or a screen saver service that is absent, refuses, or does not answer
  within **5 seconds** leaves the logind lock standing alone, and nothing reports it.

## Whether the computer is held now

The daemon reports whether it is holding the computer awake right now — the setting is on, its condition is met and
the host granted the hold — as `keep_awake_held` in the snapshot, announcing each change with `keep_awake_held_updated`.
It is never stored. In the desktop application the menu bar icon shows it while the window's UI is connected to the
daemon: its eyes turn red, and its accessible name — its tooltip on Windows — says that Octoboard is keeping the
computer awake (see "The icon's look" in `docs/product/menu-bar-icon.md`).

## Where the choice is stored

The daemon stores Prevent sleep and When to prevent sleep among its application settings, as `keep_awake` (`true` or
`false`, default `false`) and `keep_awake_when` (`always` or `working`, default `working`), so every client shows the
same choice and a change made anywhere takes effect at once. They are changed by the `update_settings` request, which
refuses any other value of `keep_awake_when`; besides Settings' rows, a console session can read and change them (see
"Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a
configuration export that includes the App behavior section carries them, an import asked to apply that section
setting them on the importing daemon whether or not its host can be held awake (see "The application settings an
export carries" in `docs/product/settings-in-a-configuration.md`).
