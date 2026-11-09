# Process holders

`apps/daemon/src/holder/` is the mechanism that lets the programs the daemon runs on a PTY (agents and project shell
terminals) outlive the daemon. A holder is a process, the `octoboardd holder` CLI mode, that owns the PTYs and the
programs on them; the daemon speaks to each terminal over one WebSocket link to a holder instead of holding the
descriptor. A daemon that starts finds the holders of the previous run through their registry files and adopts what they
kept running.

The module has two halves in one tree: `server/` is the holder, `client/` is the daemon's side. Around it sit
[`../terminal/backend.rs`](../terminal/backend.rs) (a terminal's backend is local, as `platform::term` starts it, or
held; `start` is the one place production code spawns a terminal), [`../coordinator/adopt.rs`](../coordinator/adopt.rs)
(at start, matches the terminals the holders kept with the store), [`../server/relay.rs`](../server/relay.rs) (the
daemon's end of the helper requests a holder relays), [`../cli/holder.rs`](../cli/holder.rs) (the CLI mode),
[`../request_guard.rs`](../request_guard.rs) (the Origin/Host rules and key compare the holder's helper routes share
with the daemon's guard) and [`../platform/build_files.rs`](../platform/build_files.rs). The process ledger treats a
holder and what it starts as the daemon's own; see [`../processes/ledger/README.md`](../processes/ledger/README.md).

## Where things are

- `protocol.rs` — the frames of the link: JSON text for control, binary for output and tails. A receiver ignores a
  message or field it does not know, and an enum that can grow has a `serde(other)` variant, so builds a little apart keep
  talking (the file's module doc states the rule).
- `registry.rs` — the owner-only `<holder id>.json` a holder is found by (`paths::holders_dir`; the directory is passed
  in, so tests use their own). It holds the port, the secret the link is opened with and the holder process's identity.
- `compat.rs` — which holders a daemon adopts, by semantic version: `FLOOR` up to the daemon's own. `FLOOR` is the
  previous stable release, and `apps/desktop/scripts/release-desktop.mjs` refuses a release while it is not the greatest
  stable `desktop-v*` tag below the new version. The protocol is kept compatible within that promised range; `FLOOR`
  only moves by that previous-stable release rule, never to admit a change that breaks the protocol.
- `quit_targets.rs` — the application's quit record (`application-quit`, `paths::application_quit_record`): its
  versioned JSON, the start's reconcile of it (unscoped to scoped) and the targets still to resolve; "How it works" has
  the rules.
- `build.rs` — the build id (the version and a short hash of the executable) and `Builds`, the copy of the build under
  `paths::builds_dir` that holders and their helpers run from, so that an installer can replace the installed files while
  a holder runs (on Windows a file in use cannot be replaced). The copy is the executable plus the
  `platform::build_files` companions, made in a temporary sibling directory and renamed into place, and reused when
  whole. If it cannot be made the holder runs from the installation, with a warning. Copies no live holder runs from are
  removed after adoption at start and after a holder's link ends; making a copy, starting from it and removing copies
  are serialized, so a copy is never removed between being made and being started from.
- `server/` — the holder. `mod.rs` the listener, the link and the exit rule (and the companions no terminal has bound
  yet), `terminal.rs` one terminal's threads and its 1 MiB raw tail (and what a handover restarted it with),
  `companion.rs` a terminal's companion (its endpoint, start, readiness wait, exit watch and end), `offline.rs` the
  recogniser of the queries it answers itself while no daemon is linked,
  `launches.rs` the per-launch keys it issues, `helpers.rs` the helper routes on its port (`/hook`,
  `/git-ref`, `/context-window`, `/revert-gate`, `/mcp`: the daemon's paths and bodies, with the launch's key bound to the
  terminal and its session), `relay.rs` the path of those requests to the daemon.
- `client/` — the daemon's side. `mod.rs` `Holders` (starts a holder on the first terminal, adopts the existing ones,
  tries again those it cannot link to yet, ends a holder), `link.rs` the link and its reconnect rules, `held.rs`
  `HeldTerminal` and `HeldReader`, `companion.rs` `HeldCompanion`, `relay.rs` the types of the relayed requests,
  `test_holders.rs` holders that run in the test's own process.

## How it works

- **Helpers.** A launch in a holder has its hook script, MCP command, status line, Claude hook URL and git hook
  environment name the holder's port, a key the holder issued for the launch before the program starts
  (`Holders::issue_launch`, bound to the launch's MCP token), and the holder's executable (`terminal::backend::Helpers`).
  A launch the local backend runs keeps the daemon's. `client::install`, called by `src/main.rs`, makes the daemon use
  holders; a process that never calls it, the tests' stand-ins included, runs its terminals itself.
- **Lifetime.** A holder outlives its daemon: when the link drops it keeps its programs and their tails. It exits once
  it has no terminal, no spawn pending, no companion awaiting its terminal and none being ended, and either no daemon
  is linked or the daemon sent `drain`. The daemon's ordinary
  stop ends every terminal itself, after which the holders exit; a planned stop hands them over instead ("Handover").
  One daemon is linked at a time; a newer link takes the holder over. The held terminals never enter the daemon's crash
  table (a holder has its own, which ends its programs when it crashes).
- **Relay.** The helpers call the holder, which relays to the linked daemon. A report (nobody waits on it) is answered at
  once and queued in order while no daemon is linked (1024 requests or 16 MiB of bodies, the oldest dropped first); a
  question (the revert gate, an MCP call) waits up to `RELAY_DEADLINE` for a link and fails at once if the link drops
  while it is in flight, since the daemon may have acted on it. A helper that hangs up sends `relay_gone`. The daemon
  runs the relayed request through the same `callbacks::handle` its HTTP routes run, once `AppState::end_adoption` says
  the adopted sessions exist: reports that arrive before are held in order and handled as the adoption ends, before it
  reads as done (`AppState::after_adoption`), so the exit of an adopted session, which waits for that, settles them;
  questions wait up to 20 s.
- **Reconnect.** A link that drops without a `parting` is a crash, which the next welcome reports (`last_parting`). A
  transport failure never ends a holder: three cases are told apart. A holder that *exited* (it left no registry file,
  or its verified process is gone) has its link lost, with no reconnect or end. One *confirmed incompatible* (a welcome
  this build cannot read) is ended (`client::end_process`: asked to end, which the holder takes as its own stop and ends
  every program, then killed after a grace; on Windows killed at once, which closes its pseudoconsoles; never the
  daemon's own process). One that *cannot be connected to yet* (a refused connection, a handshake that times out, a link
  that drops again soon after it was opened again) keeps its process and its registry file: the link is opened again at
  once, and after a try that fails it is *pending* (`Link::pending`, `HeldTerminal::unlinked`), tried again in the
  background with a pause that doubles from half a second up to 30 seconds, for as long as the holder's verified
  process runs and its file is there, the pause going on from where it was when a link opened again drops within a few
  seconds. Calls made while a dropped link's first try is under way (writes, requests, spawns, the daemon's parting)
  wait for its outcome, bounded by the connect deadline; once the link is pending they fail at once, and requests fail
  at their deadlines as before. Its terminals' owners are told when it becomes pending and when that ends
  (`HeldTerminal::on_link_changed`), which the wire shows as `holder_link: "pending"`; new terminals go to a new holder
  meanwhile, and the old one is told to drain once it is back. A link opened again while the daemon parts is used only
  to part over it; a launch key revoked while the link is down is revoked once it is opened again. The daemon's
  ordinary stop ends a holder whose link is pending by its verified process (`Holders::shutdown`), since its terminals
  cannot be ended over the link. A link lost for good closes its connection.
- **The welcome is the truth.** The welcome of a link opened again says what the holder runs: every terminal it lists
  that the link does not own (a start that was not confirmed, a release or end lost with the connection) is killed, hung up
  and released. A frame for a terminal the link does not know (a start the daemon gave up on that the holder confirms
  late, or output of one nobody owns) has that terminal killed and released. A holder that cannot deliver `spawned` to
  the link that asked ends and forgets the terminal; the confirmation is sent before the terminal is followed by the
  current link, and a link that did not ask for the terminal is never closed for failing to follow it. A spawn is waited
  for as long as the holder takes: only its answer or the end of the connection ends the wait, so a start is never
  repeated on a guess.
- **Takeover.** A newer link ends the older with a final `superseded` frame naming the run id of the daemon that took
  over (`by`, absent when that daemon sent none), which the older daemon takes as lost to another daemon: no reconnect,
  no end. A frame naming the daemon's own run (a connection of a link it gave up that got through late) is a plain
  drop instead, and the link is opened again, taking the holder back. The `hello` carries a run id, random per daemon
  process (made with its `Holders`, so that tests running several in one process have each stand for a daemon of its
  own), and the holder refuses a `hello` from a run id it was taken over from with the same frame, so a stalled old
  daemon cannot take it back (a `hello` without a run id, from an older build, is accepted, and the run before it counts
  as taken over).
- **Output across a gap.** Output printed while the link was down never reached the parsed screen, so the terminal's
  owner is told (`HeldTerminal::on_resumed`, `PtyCore::on_resumed`): `AppState::register_live` and the shell terminals'
  `register` mark the screen inexact, publish the record and have the program repaint. A tail sent with a welcome is kept
  only for a terminal that welcome promised and adoption has not yet taken or given up on; a reconnect's tail covers the
  output the earlier connection routed, which is dropped.
- **Queries while no daemon is linked.** The holder keeps no screen, but a program waits on its terminal's answer to a
  query, and the bundled Windows console host holds it until one comes. So the holder follows each terminal's output
  with a bounded streaming recogniser (`server/offline.rs`: one byte at a time across reads, skipping what is inside
  OSC, DCS, APC, PM and SOS strings, keeping at most one short sequence) and, while no daemon is linked, answers the
  queries whose answer needs no screen with the daemon's own answer, through the terminal's serial input writer: the
  primary device attributes (`ESC [ ? 6 c`), where the host does not answer it itself. It answers only a query as its
  last byte is read, never one already in the tail. The cursor position and every other query that needs the screen go
  unanswered and wait out the host's hold, a stated limit. Which side owns a query is decided in output order under the
  terminal's stream lock, which a link attaches and detaches under: a query that ends while a daemon is linked is that
  daemon's, one that ends while none is, the holder's. The welcome lists what the holder answers (`answers_offline`; an
  older holder sends none and answers none), and the daemon never answers from a tail it is handed
  (`PtyCore::seed_output`), so no query is answered twice. A tail replayed after a handover
  (`HeldTerminal::replay_first`) is read as live output, so it answers the fresh queries in it, except the kinds in
  `answers_offline`, which the holder answered already.
- **Handover.** A daemon that stops for another to take over (a planned restart) hands its terminals over instead of
  ending them. `Holders::hand_over` sends `handover` with a token to every live link of the holders it is named whose
  welcome says the holder carries it out (`handover: true`; an older holder is never sent one), all at once, each
  answered within 5 seconds. For each terminal whose output goes to that link, the holder, in one step under the
  terminal's stream lock, queues `handover_mark` behind every output frame already queued, stops sending the terminal's
  output to the link, and records where the mark falls in the tail under the token. A mark is sent only to the link that
  is still current: a link that was taken over or closed while the mark waited for room gets none. The tail stays the
  rolling 1 MiB of output, so it keeps the history before the mark; room for the mark is reserved before the lock is
  taken, as for output. Terminals other links follow get no mark and keep their tails. `done` follows the last mark on
  the same channel. So the mark divides the output exactly: what came before it reached the stopping daemon, and the
  tail holds what follows it from the mark's place on. From the mark on no daemon is linked for the terminal's output,
  so the holder answers the queries in it it may ("Queries while no daemon is linked"). Each welcome reports, for a
  marked tail, the token, `since` (the offset into the tail sent with the welcome at which the output since the mark
  begins; absent from an older holder, so 0), whether the mark has fallen off the front of the tail (`truncated`, once
  more than 1 MiB came after it) and the age of the output since the mark by place (a place per 25 ms of reading at
  most, the latest 4096, offset 0 the first byte after the mark; ages counted back from the moment the welcome is made),
  as `TerminalInfo::handover`. Listing a marked tail in a welcome pins its front: until the link attaches to the
  terminal (or ends) the tail keeps what the terminal prints past 1 MiB, up to 2 MiB, instead of dropping it, so the
  told `since` still holds when the tail is handed over however much the terminal prints meanwhile. The attach still
  refuses a tail marked again, a mark that fell off the front (the pin gave up) or a changed token. The daemon's link,
  on a mark, closes the terminal's output, so its reader reads what is queued and ends, and from then on the terminal is
  the next daemon's: no frame that names it is sent (a release, signal, hang-up, write or request; requests answer as
  over a lost link), its owner is not told of the link going pending or coming back, and nothing records it as exited,
  whether the holder reports an exit, a later welcome says so or the link ends. The one exception is the answers to the
  queries in the output its reader still takes in, all of it from before the mark, which neither the holder nor the next
  daemon answers: the daemon's own and its clients' replies to those left to them go as ordinary `write` frames
  (`HeldTerminal::write_answers`) until the daemon parts. The planned stop waits, within its deadline, for the reader to
  end, the answers' writer to have every write acknowledged and no client to owe a reply (`PtyCore::answering`,
  `PtyCore::replies_owed`, `PtyCore::answers_lost`) before it saves the screen; a terminal whose answers do not all go
  in is left to be rebuilt from its tail, inexact. A client's typing is refused meanwhile, and its terminal socket stays
  open for the replies. If the link drops after a mark and the stopping daemon links again, its `hello` names the
  terminals it handed over (`handed_over`): the holder lists them in the welcome but attaches none of them to that link,
  so their output stays unlinked, the holder goes on answering the queries it answers while no daemon is linked, and the
  output since the mark stays in the tail where it began. After a planned parting no terminal of that link reads as
  exited, marked or not: the holder keeps them all for the next daemon. `Link::hand_over` returns the terminals marked:
  all of them once `done` arrives, and when it does not within 5 seconds or the link drops, those whose marks were
  routed by then (their outputs are closed), `None` only when none was marked or the holder does not carry it out. A
  link that is being opened again is waited for first (settled), which may take up to about 15 seconds before its own 5
  second deadline starts. `Holders::begin_planned_stop` sets the stop as planned before the planned stop lists its
  terminals, so a pending holder that is linked to from then on is parted from with `parting { handover: true }` and
  left running instead of being reported to the stopping daemon, and it returns the holders still pending, whose saved
  screens the stop carries over. `Holders::hand_over` hands over only the holders it is named, those of the terminals
  listed, so a holder linked to late, whose terminals were not listed, is left untouched too. `Holders::part_planned`
  then parts from every link with `parting { handover: true }`, stops trying the holders it cannot link to, and ends no
  holder, unlike `Holders::shutdown`. The daemon that starts next finds each terminal with its tail and the handover
  facts of the welcome the tail came with (`FoundTerminal::handover`, `welcomed_at`); `FoundTerminal::since_handover` is
  the output since the mark (the whole tail when no handover marked it), which a daemon that restores the screen saved
  at the mark replays through `HeldTerminal::replay_first`, which has the reader return it before the live output, while
  one that rebuilds the screen from the tail seeds the whole of it. A welcome that told a tail's token, or that the mark
  was still in it and where, closes the link when the tail is no longer so by the time it is handed over (marked again,
  the mark fallen off its front or moved within it), and the daemon's next link is told the tail as it is.
- **Adoption at start.** `Holders::adopt_existing` links to each holder the registry lists, all at once, with one try of
  3 seconds each, so that the start stays well inside the time the application gives the daemon (a file whose holder is
  gone is removed; one whose process identity cannot be read is removed when nothing listens on its port, and kept,
  logged, for a later start when something does and it cannot be linked to; the build sweep reads such a holder as gone
  in the first case) and takes each terminal with its meta and tail as a `FoundTerminal`. It keeps a holder of its own
  build for new terminals and tells the others to `drain`. A holder whose version is outside `compat::Range` (older than
  the floor, newer than the daemon) is not adopted: its terminals are ended, it is drained and parted from; one whose
  welcome or version cannot be read is ended by its registry pid, once its process identity is verified; both are done
  on threads of their own, off the start, and what such a holder runs stays claimed until it is ended
  (`Adoption::ending`, resolved as gone; one that cannot be ended is pending instead). One whose verified process runs
  but that does not answer that one try in time is *pending* (`Adoption::pending`): it is not ended, and it is tried
  again on a thread of its own, a second later and then with a pause that doubles up to 30 seconds, until it is linked
  to, when its terminals are taken over as at start and handed to the caller (`Resolved::Linked`, under its registry
  file's holder id, which its welcome has to name, else it is ended as unreadable; the helpers' requests it relays
  meanwhile held back until the caller drops `HeldRelays`; it is drained), or known to be gone (`Resolved::Gone`). A
  link that drops while its terminals are being taken over, at start or later, and is not lost for good is not waited
  for but given up (`Link::abandon`: nothing it holds is ended or released, and no frame that would end one is sent) and
  its holder is pending in the same way, every terminal left with the holder for the link that takes it over next. What
  `coordinator::adopt` and `coordinator::unlinked` do with them is in those files' module docs. A quit of the
  application leaves a durable record of the holders to end (`quit_targets`: the application's `application-quit` file,
  see PROTOCOL.md, "The quit record"), which every start reconciles under the instance lock, whether or not
  it follows the application (`--parent-pid`), before any holder is adopted or terminal started: a record that names no
  holder, or an empty file, is scoped to the holders the registry lists, each by id and process identity, and written
  back atomically; one of an unknown version or unreadable reads as unscoped, logged as an error. The daemon gives the
  targets to `Holders::end_quit_targets`: a target is never adopted or made the current holder, so new terminals go to a
  fresh one, and it is refused as one outside the range is, its terminals ended and its sessions left interrupted; any
  other holder found is adopted as usual. A target whose verified process runs but that cannot be linked to (at start,
  or on any later try) is ended by that process rather than tried again; one whose process cannot be verified is never
  signalled and stays in the record. `HeldTerminal::end` tells whether the program's end is confirmed (the holder
  answered that it reaped it, or it was reaped already); a failed kill, a lost link, no answer or a leftovers sweep
  that could not be finished is not, nor is a terminal the holder's welcome listed that its adoption left out, and then
  the refusal ends the holder by its verified process and reports it gone only if it is, so that an unconfirmed end
  publishes no `Resolved::Gone` and the sessions' claims and held-terminal records stay. A target is removed from the
  record (rewritten atomically, the file removed with the last) only as it is resolved: its ending confirmed, or the
  holder found gone, at start or long after it; never on a timeout or because the holders were installed, and where they
  could not be the record is left as it is. The file is rewritten or removed only while it still says what the daemon
  last wrote: a record the application wrote meanwhile (every quit writes one) is left in place. The endings run off the
  start, and `Holders::wait_endings` waits for them, for up to `ENDINGS_WAIT` in all, which bounds only the start (the
  application gives the startup line only so long): an ending still running then carries on, and updates the record when
  it finishes. A refused holder's terminal tails travel with `Resolved::Gone` (`EndedTail`, with the handover mark when
  the tail has one, whether the holder was refused at start or once linked), so that the daemon rebuilds the sessions'
  previews from them. Stated limits: a holder the previous daemon launched that had not yet written its registry file
  when the record was scoped is not a target, and a record that cannot be rewritten stays unscoped and is read again by
  the next start. `Holders::pending_holders` lists the holders that could not be linked to and are tried again.
- **Companions.** A launch can have its holder run a process for its terminal before the terminal: its companion (a
  private server the program attaches to). The holder knows nothing of what it serves. Under the terminal id the launch
  key was issued for, the daemon has it open an endpoint (`open_endpoint`: a socket named as asked in a new owner-only
  directory under `/tmp`, or a loopback port guarded by a new random token, which the holder makes and reports), start
  the companion on it (`start_companion`: the daemon's command, environment and working directory, how readiness shows
  and within how long), then spawn the terminal with `companion: true`, which binds the companion, or is refused when
  it is missing or has exited. The companion runs as a plain child with no PTY (`platform::proc::server`): standard
  input closed, output piped and read for the port after the prefix it was given (`PortAfter`) and discarded when readiness
  is the socket existing (`SocketExists`), leading a session of its
  own (on Windows a job of its own nested in the launch's, with no console), in the launch's containment and the
  holder's crash table. Owner-death cleanup also works when the holder cannot run a handler or a destructor:
  `apps/daemon/src/platform/proc/server/windows.rs` creates the server directly in a private kill-on-close job, whose
  handles stay in the holder, and the launch's job, using an explicit list of inherited standard handles. The launch
  job itself cannot kill on close, since its members can be kept and hold their own handles on it. On Unix,
  `apps/daemon/src/platform/proc/server/parent.rs` creates a watcher after `setsid` and before exec; only the holder
  keeps the pipe's write end, so its death gives the watcher EOF and the watcher kills its own group, including the
  companion. The
  watcher stays in that group to reserve its id, closes inherited descriptors and is double-forked so the companion
  does not inherit an extra child to reap. These resources belong to the holder, so a daemon's parting leaves them
  intact. One that exits first or is not ready in time is ended, with its endpoint. The holder enforces
  the lifetime itself, linked or not: once a terminal's exit is published, or the holder ends or releases it, its
  companion is ended (a polite request to its group, a 2 second grace, a kill and a sweep, its endpoint removed); a
  companion that exits by itself has `companion_ended` sent to the linked daemon and its terminal ended (a polite
  terminate, a kill after the terminal's polite lead and 3 seconds). A companion no terminal has bound is the request's:
  it is ended when the connection it was asked over ends, on `revoke_launch` or `end_companion`, by a second
  `open_endpoint` under its id, by a spawn that expects none, by a spawn refused while the holder drains, and after 2
  minutes; a spawn that expects it over another link than the one that opened it is refused, and so is one made while
  its start is under way. It counts as something to serve in the exit rule, as does each end under way (ended on a
  thread of its own, the holder exits only once its grace, kill and sweep are done; an end that finds another
  under way on the same companion, such as its terminal's exit watch or its own exit, waits for it, so the counted end
  covers them all), is never listed in a welcome, and a
  draining holder opens or starts none (a start that finds the holder draining ends the opened one). A holder that is
  ended ends its terminals, and their companions, in parallel. The welcome says the holder runs companions (`companions`; an older holder is
  sent no companion frame) and lists each terminal's bound companion with its endpoint, process and whether it exited.
  On the daemon's side `Issued` carries the requests, `HeldCompanion` reads as exited from `companion_ended`, a welcome
  that lists it exited or not at all, its end, its terminal's release, the link lost for good, or the connection
  dropping before a terminal bound it (the holder ends an unbound one with its connection), and a found terminal
  carries its companion (`FoundTerminal::companion`). The daemon reaches the companion at its endpoint directly, not
  through the link.
- **Starting a holder.** `Holders` starts one from the build's copy on the first terminal, detached
  (`platform::proc::start_detached`) so that it outlives the daemon, and waits for its registry file. On Windows, where
  the daemon's job refuses breakaway, it is started under the desktop application, or with none inside the job, with a
  warning; survival is not guaranteed then, nor when the application is in the same outer job ("Process holders on
  Windows" in `docs/architecture.md`).
- **One holder per build** is the usual allocation, not an invariant: a start keeps one of its build for new terminals
  and drains the others, and a pending link's holder is replaced for new terminals. The terminals of a holder share its
  link, so one whose reader stops taking output holds up the others and the replies to the daemon once its queue
  (`OUTPUT_CHUNKS` in `client/link.rs`) is full, until it is read again: a stated limit.

## External interfaces

- `server::run`, `server::run_until`, `server::HolderConfig` — run the holder until it has nothing left to serve (the
  `octoboardd holder` mode calls them).
- `client::install`, `client::current`, `client::ClientConfig`, `client::launch_detached` — make the daemon run its
  terminals in holders, and reach the installed `Holders`.
- `Holders::spawn`, `Holders::issue_launch`, `Holders::adopt_existing`, `Holders::sweep_builds`, `Holders::shutdown` — the
  daemon's calls: start a held terminal, have the holder issue a launch's key (`Issued::spawn` then starts the program),
  take over what holders kept running (`Adoption`, `Found`, `FoundTerminal`; `OnResolved`, `Resolved` and `HeldRelays`
  for a holder linked to later), sweep unused build copies, part from every holder (ending those that cannot be linked
  to).
- `Issued::companions`, `Issued::open_endpoint`, `Issued::start_companion`, `Issued::end_companion`, `HeldCompanion`,
  `FoundTerminal::companion` (`FoundCompanion`) — a launch's companion: whether the holder runs companions, its endpoint,
  its start, its end before the terminal binds it, and its handle (`exited`, `end`).
- `Holders::begin_planned_stop`, `Holders::hand_over`, `Holders::part_planned` — a planned stop: stop trying the
  pending holders (returning them), hand the terminals of the holders named over under a token (the `HeldAt` of each
  terminal handed over), then part from every holder leaving all of them running.
  `FoundTerminal::handover` (`protocol::HandoverTail`, `protocol::TailAge`), `since_handover`, `welcomed_at`,
  `answers_offline` and `holder_id`, and `HeldTerminal::replay_first`, are what the next daemon restores a
  handed-over terminal from.
- `quit_targets::reconcile`, `quit_targets::QuitTargets`, `quit_targets::Target`, `Holders::end_quit_targets`,
  `Holders::wait_endings`, `client::ENDINGS_WAIT` — the application's quit record read at start, the endings of the
  holders it names (`HeldTerminal::end` tells whether one is confirmed), and the bounded wait for them.
- `Holders::pending_holders` — the holders that could not be linked to and are tried again.
- `Holders::versions` — the version each linked or pending holder reports, by holder id, which
  `coordinator::handover_check` judges against the next daemon's floor.
- `HeldTerminal`, `HeldReader`, `HeldAt` — a held terminal and its output stream, answering the calls a
  `platform::term::Pty` and a `ChildHandle` do, over the link, with whether its holder can be reached now
  (`unlinked`, `on_link_changed`) and where it runs (`held_at`, which the daemon records with the launch).
- `RelayHandler`, `RelayRequest`, `RelayReply` — what answers a request a holder relays; the daemon's is
  `server::relay::DaemonRelay`.
- `build_id`, `build::Builds` — a build's identity and the copy holders run from.
- `compat::FLOOR`, `compat::Range`, `compat::Version`, `compat::Verdict` — the adopted version range.
- `registry::RegistryEntry` with `write`, `read`, `list`, `remove`, `nothing_listens` — the holder's registry file.
- `protocol::ToHolder`, `protocol::FromHolder`, `protocol::BulkKind` and the types they carry — the frames.
- `RELAY_DEADLINE` — how long a waiting helper request waits for a daemon to link.
