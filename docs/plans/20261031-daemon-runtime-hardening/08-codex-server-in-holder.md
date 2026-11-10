# 08 Codex app server in the holder

> Platform scope: macOS and Windows.
>
> Goal: a Codex session's private app server lives in the holder that runs its terminal, as the terminal's companion,
> so a daemon crash, a planned restart and a desktop update leave the server running with the terminal, and the daemon
> that adopts the terminal links to the server again and keeps delivering the session's automatic messages through
> Codex's own queue.
> Completion criteria:
> - Launch a Codex 0.166.0 or later session with holders installed: `codex app-server` is a child of the holder, in the
>   launch's containment, leading a session of its own; the terminal attaches to it with `--remote`; automatic messages
>   go into Codex's queue as they do today.
> - Kill the daemon outright (`kill -9`) and let it start again: the session keeps running, a message sent to the
>   adopted session goes into Codex's queue, not the terminal, and after a cancel the messages Octoboard queued before
>   the restart are started as they would have been without it.
> - `octoboardd restart` with such a session working: the same, with the exact screen; messages left in the outbox by
>   the planned stop are delivered by the successor through the queue.
> - Exit Codex in the terminal: the holder ends the server within a few seconds and nothing of the launch is left. Kill
>   the server: the terminal is ended and the session goes through its exit path, whether or not a daemon is linked.
> - A launch in a holder whose welcome does not advertise companions runs the ordinary terminal launch, with guarded
>   terminal delivery, and no `--remote`.
> - A server that does not come up leaves no companion in the holder; the terminal starts unbound, as the ordinary
>   launch.
> - The same on Windows, where `octoboardd restart` is not supported and a planned stop followed by a start takes its
>   place, the loopback port read from the server's output by the holder and the token reaching the terminal.

## Technical design

### The holder runs the server as the terminal's companion

- [x] A **companion** is a process a holder starts for a terminal before the terminal, under the terminal's id (the id
      `IssueLaunch` reserves), ends once the terminal has exited, and whose death ends the terminal. The holder knows
      nothing of Codex: the daemon sends the command line, environment and working directory, how the companion
      listens and how its readiness shows, and the holder reports the endpoint. The terminal id ties the launch's key,
      its companion and its terminal together.
- [x] Protocol, under the module's compatibility rule (new fields `serde(default)`, new message types ignored by a
      receiver that does not know them, enums with a `serde(other)` variant):
  - `FromHolder::Welcome` gains `companions: bool` (false from builds that predate it): the holder runs companions.
    A daemon sends none of the frames below to a holder whose welcome says false.
  - `ToHolder::OpenEndpoint { id, req, kind: EndpointKind }`, answered by
    `FromHolder::EndpointOpened { req, endpoint: Option<CompanionEndpoint>, error }`. `EndpointKind` is
    `Socket { prefix, name }` (a socket named `name` in a new owner-only directory under `/tmp` whose name starts with
    `prefix`) or `Loopback` (a new random token; the port is the companion's to report). The holder owns what it
    opened: the directory goes when the companion ends.
  - `CompanionEndpoint`, tagged by `kind`: `Socket { path }` or `Loopback { token, port: Option<u16> }` (the port
    absent until reported), plus `Unknown`.
  - `ToHolder::StartCompanion { id, req, command: Command, ready: Ready, ready_within_ms, containment }`, answered by
    `FromHolder::CompanionStarted { req, pid: Option<u32>, identity, port: Option<u16>, error }`. `command` is the type
    the spawn carries, over the daemon's own environment and working directory; `containment` is the launch's, as the
    spawn names it. `Ready` is `SocketExists` (the opened socket
    path appears; Codex links it to a socket elsewhere once it listens) or `PortAfter { prefix }` (a line of the
    companion's output holds a non-zero port number right after `prefix`, which the holder reports). A companion that
    exits first or is not ready within the deadline is ended and the start answered with the error. A kind of
    endpoint or readiness the holder does not know is answered with an error.
  - `ToHolder::EndCompanion { id, req }`, answered by `FromHolder::Done`: ends a companion the terminal has not bound
    yet (a resume's first server, a launch that falls back), removing its endpoint. `RevokeLaunch { id }` ends the
    companion of `id` too.
  - `SpawnSpec` gains `companion: bool` (default false). A spawn that expects one is refused (`SpawnFailed`) when the
    companion of its id is missing or has exited, so no terminal is attached to a dead server; one that expects none
    ends any companion left under its id and starts unbound.
  - `TerminalInfo` gains `companion: Option<CompanionInfo>` with `{ endpoint: CompanionEndpoint, pid, identity,
    exited }`, so every welcome tells the next daemon where the server listens and whether it still runs.
  - `FromHolder::CompanionEnded { id }`: the companion of terminal `id` exited by itself, neither on an `EndCompanion`
    nor after its terminal's exit.
- [x] How the holder starts a companion: as a plain child with no PTY, standard input closed, output piped and read to
      its end only for `PortAfter` and discarded otherwise; leading a session of its own (`Reach::Session`: on Windows
      a job of its own nested in the launch's) inside the launch's containment opened as the terminal's is
      (`Containment::join_launch`), and in the holder's own crash table, so the holder's own crash ends it as it ends
      its programs. The start and its readiness wait run off the runtime, as a spawn does; the daemon waits for the
      answer or the end of the connection, as for a spawn.
- [x] Lifetime, enforced by the holder whether or not a daemon is linked:
  - Once a terminal's exit is published, its companion is ended: a polite request to its process group, a short grace,
    then a kill and a sweep of what it left, and its endpoint directory removed. A terminal the holder ends or
    releases ends its companion the same way.
  - A companion that exits by itself while its terminal runs has `CompanionEnded` sent to the linked daemon, and the
    terminal is ended as the daemon ends it today when the server dies: a polite terminate, then a kill after the
    polite lead the terminal reports. A daemon that is linked does the same through `server_gone`; both are harmless
    together.
  - A companion no terminal has bound yet is ended when the connection it was asked over ends (whatever the ending),
    on `RevokeLaunch` or `EndCompanion`, and after a bind deadline in any case; it is never listed in a welcome.
  - A companion awaiting its terminal counts as something to serve: the holder's exit rule treats it as a pending
    spawn. A draining holder refuses `OpenEndpoint` and `StartCompanion` as it refuses a spawn.
- [x] The welcome is the truth for companions as for terminals: a daemon rebuilds a transport only from what the
      welcome lists, and a companion listed `exited: true` is a server that is gone.

### The daemon: where a private server runs

- [x] A provider-neutral host interface beside `PrivateServer`, with two implementations that the launch chooses by
      where the terminal will run:
  - `ServerHost`: `open_endpoint(kind) -> ServerEndpoint`, `start(command, ready, within) -> (RunningServer, port)`,
    `end()` (the server opened under this host, before the terminal binds), `companion() -> bool` (whether the
    spawn is to expect the server as its terminal's companion).
  - `RunningServer`: `exited() -> bool`, `end()`, `ends_with_its_terminal() -> bool` (whether the holder, not the
    daemon, ends it).
  - The interface and both hosts live in `terminal::server_host`, below the sessions that launch with them.
  - The **local host** is today's in-process start (the private listen directory or token, the child led as a
    session in the launch's containment, the daemon's crash table, the readiness wait, the polite end with grace):
    moved out of the Codex transport, which only ever read a port from a line and a path's existence of it. It is
    what the tests' stand-ins and a daemon without holders use.
  - The **held host** is the launch's `Issued` credentials: the frames above over the holder link, under the
    launch's terminal id. Its `RunningServer` reads `exited` from `CompanionEnded`, from the welcome, and from the
    link being lost for good (the holder gone); a link that is only pending leaves it unknown and running.
- [x] `PrivateServer::start` takes the host; the Codex plan builds its `app-server --listen …` arguments from the
      endpoint the host opened (the socket path, or the SHA-256 of the token) and its readiness from the platform
      (`SocketExists`, or `PortAfter("listening on: ws://127.0.0.1:")`); a resume's two starts go through `end` and a
      fresh `open_endpoint` between them, as today's second server does.
- [x] `Helpers` says which host a launch has: the local host when the terminals run in the daemon, the held host
      when the holder's welcome advertises companions, none for a holder that does not. A plan with a private server
      and no host is launched the ordinary way, as a server that fails to come up is today.
- [x] Launch flow: issue the launch's credentials; plan; where the plan has a private server and a host, start the
      server through the host and take the terminal arguments and environment it gives; spawn the terminal with
      `companion: true`. A start that fails ends what it opened through the host, then the ordinary spawn follows
      with `companion: false`. A refused spawn fails the launch as any spawn failure does.
- [x] The daemon's crash table never holds a held companion, as it never holds a held terminal.

### The daemon: the Codex transport over a server it does not own

- [x] The transport holds a `RunningServer` and an endpoint instead of a child: its exit watch polls `exited`, its
      `send` reads it to hold a message when the server is gone, and `stop` ends the server only when the local host
      runs it; with the held host `stop` only marks the transport stopping and withdraws what the session was told,
      since the server is the holder's and ends with the terminal. Nothing else of the queue logic changes.
- [x] Two constructors: `start` (the launch, as today, through the host) and `adopt(endpoint, server, launch id,
      known thread)` for a terminal taken over. `adopt` opens no server; with a known thread it identifies the
      conversation at once, without one it opens the thread watch (a thread the user starts after the restart) and
      leaves earlier conversations to the loaded-threads discovery `send` already does.
- [x] Octoboard's messages in Codex's queue are known by their id, not by memory: the outbox already names each
      message `<launch id>:<ticket>`, so a queued item whose `clientUserMessageId` starts with the launch id and a colon
      is Octoboard's, whichever daemon run queued it. The outbox's ticket counter is seeded from the clock at daemon
      start, so a launch's ids never repeat across runs. The transport keeps `added` for the run's own bookkeeping
      (a refused message is forgotten) and asks the prefix for anything it did not add itself.
- [x] A message already in the queue under its own id is started where it waits, never added again: `send` looks the
      id up in the queue before adding, for an id that carries the launch's prefix or that this run held at start,
      whether or not this run remembers adding it. What is lost with a daemon and
      what it costs:
  - the identified thread: taken from the session record's agent session id (Codex's thread id, learned from its
    first hook payload, or the resumed thread), else re-identified as above;
  - `added`: recovered by the prefix, so a cancel after the restart still starts Octoboard's first waiting message,
    and a new message behind Octoboard's own is started as today;
  - `start_held` (taken by Codex but not started, as it was overloaded): the queue lookup covers a message still
    queued, since the restart outbox saves each message's ticket and the successor queues it again under the same
    one, so its id is unchanged (a ticket already in use, or a file of an older build, gets a new one); one Codex ran by itself between the restart and the successor's retry runs twice. A stated limit: it
    needs an overload refusal at the moment of the restart;
  - `last_start`: a second queue start within 10 s reads the thread's status, which refuses a start during a running
    turn, so nothing is started twice;
  - the held-message notice: told again by the successor after its own 30 s; a notice the predecessor broadcast is
    not withdrawn by it, as events are not persisted.
- [x] Messages in flight at a handover: the outbox seals and saves what is queued; a request already
      sent to the server completes or fails within its own 5 s, independent of the holder link, and the uncertain
      rules stand. Nothing of this milestone writes into the terminal during a planned stop: a message the server
      declines (nothing of it reached it) falls back to the terminal only while the handover barrier is down; one
      declined after the barrier rose (a send begun before it) is not written but put back
      at the front of its queue, so the seal saves it for the next daemon, as the barrier rules have drains under way
      put back what the handover refused (kept rather than failed: nothing of it was delivered, and the successor
      delivers it whole).

### Adoption: re-linking to the server

- [x] The holders' client carries the welcome's companion: `FoundTerminal` gains the found server (its endpoint and a
      `RunningServer` over the link), and the link routes `CompanionEnded` by terminal id to it, for terminals it
      spawned and terminals it adopted alike.
- [x] Every adopted launch goes through the provider: `rebuild_launch` takes what the adoption knows (the adoption
      record where there is one, the launch id, the session record's agent session id, the found server) and
      `RebuiltLaunch` gains an optional transport. Codex rebuilds its transport with `adopt` when a server was found
      whose endpoint is complete (a loopback endpoint without a port is treated as no server, logged); the other
      providers return none.
- [x] The adopted session is built with that transport and registered as today: `register_live` attaches the events,
      so a server already gone fires `server_gone` on attach and the terminal is terminated, an idempotent step where
      the holder ended it already and the welcome lists the terminal exited too.
- [x] A terminal whose meta names a Codex launch but whose welcome lists no companion is adopted without a
      transport, as today. This covers every terminal an older holder runs (it never ran a server) and the terminals
      of a daemon between the private server's arrival and this milestone, whose server was the daemon's own: after
      an orderly exit that server died with the daemon and the terminal with it, so the holder lists it exited and it
      goes through the exit path; after a kill the server was orphaned and still serves the terminal, the session
      falls back to terminal delivery, and the server is swept with the launch by the process ledger when the launch
      ends. Unreleased builds only; no rule is added for them.
- [x] A planned stop and a crash change nothing for a companion: it is the holder's, and the next daemon finds it in
      the welcome. `handover_check` stays as it is: a holder in the compatible range interrupts no Codex session on
      its account, since in a holder without companions the daemon launched no server.

### Windows

- [x] The holder starts the companion with no console of its own to lose (`CREATE_NO_WINDOW`, as a session leader),
      its output piped, and reads the port after the prefix from that pipe; the daemon never sees the companion's
      output.
- [ ] Jobs: the companion joins the launch's job and a job of its own nested in it, as the terminal does, so the
      launch's processes are listed as today and the holder's end closes them. Where the holder itself could not break
      away from the daemon's job, the companion ends with that job as the terminals do, a stated limit of milestone
      02.
- [x] The token is the holder's: made at `OpenEndpoint`, given to the daemon in the answer, put in the terminal's
      environment by the daemon through the spawn, and listed in every welcome. It crosses the holder link, which is
      authenticated by the holder's secret from an owner-only registry file and carries the launch keys and MCP
      tokens already; the server still receives only its SHA-256.

### Security of reaching the server directly

- [x] The daemon connects to the companion's endpoint itself, not through the holder: on macOS a socket in
      a directory only the user can enter, as today, now made and removed by the holder (a holder killed outright
      leaves the directory, as a killed daemon did); on Windows the loopback port guarded by the token above.
- The direct connection keeps working while the holder link is down, so a message is queued through a pending holder:
  - [x] macOS, real Codex 0.166.0: one message reached the queue while `holder_link: pending`, with the same id
        present once after reconnect. Evidence: `/tmp/octoboard-remaining-20261110-7fd0/codex-pending.json`.
  - [ ] Windows: verify queue delivery while the holder link is pending.

## Implementation plan

- [x] Holder side: the companion in the holder's protocol and server (a companion module beside the terminal's; the
      terminal binds its companion at spawn and ends it at exit; the link's end, revoke and drain rules; the welcome's
      capability and per-terminal companion). Covered by the holder test harness (`holder::server::tests`,
      `holder::protocol::tests`, run at push): a companion started before its terminal and listed in the welcome with
      its endpoint; ended once the terminal exits; its death ending the terminal with no daemon linked and announced to
      a linked one; an unbound companion ended with the link, on revoke and on `EndCompanion`; a spawn expecting a dead
      companion refused; a resume's second endpoint elsewhere; the port read after the prefix; a start that never
      becomes ready failing and leaving nothing; a draining holder refusing one; the protocol's compatibility tests
      covering the new fields and `Unknown` variants.
- [x] Holders' client: the welcome's capability on `Issued`, the companion requests and their answers over the link, the
      client-side `RunningServer` and the routing of `CompanionEnded` and of a link lost for good, the found server on
      `FoundTerminal`. Covered by the in-process test holders (`holder::client::companion_tests`,
      `holder::client::tests`, run at push): a launch in an old-welcome holder told it has no host; an adopted terminal
      carrying its companion and marked exited by the frame; a held companion ended over the link before its terminal
      binds.
- [x] The daemon's host interface and the transport over it: `ServerHost` and `RunningServer` with the local host moved
      out of the Codex transport; the transport's `start` through the host and its `adopt`; message ownership by id
      prefix, the queue lookup by id before an add, the outbox's clock-seeded tickets. Covered by the existing transport
      tests unchanged in behavior (`adapter::codex::transport::tests`, `terminal::server_host::tests`,
      `messaging::outbox::tests`, run at push), plus: a transport adopted with a known thread queues without a watch;
      one adopted without a thread identifies the conversation from the loaded threads and takes a thread started later;
      one adopted to an exited server reports it gone on attach; queued messages of an earlier run are Octoboard's by
      their prefix and are started after a cancel; a message found in the queue under its id is started in place and not
      added again; stopping a transport over a held server leaves the server running; a resume restarts the server
      through a fake host.
- [x] The launch through the held host: `Helpers` choosing the host, the launch starting the server through it,
      `companion: true` on the spawn, the fallback ending what it opened. Covered with the test holders
      (`terminal::backend::tests`, run at push): a launch in a holder with companions starts the server there and the
      terminal attaches to it; a launch in a holder without them is the ordinary launch; a failed server start leaves no
      companion and spawns an unbound terminal.
- [x] Adoption: `FoundTerminal`'s server through `rebuild_launch` into the adopted session's transport. Covered with a
      fake app server (`coordinator::adopt::tests`, run at push) behind the endpoint a test holder's welcome lists: an
      adopted Codex session's message goes through the queue with the thread taken from the store; an adopted terminal
      whose companion exited goes through the exit path.
- [x] Documentation: the holder's README ("How it works" gains the companion, its lifetime and its place in the
      welcome), the product docs of the private app server's lifetime and endpoint, of messages through Codex's queue
      across a restart, and of what a holder runs, and the daemon structure doc.
- [x] Manual verification of the completion criteria on macOS with the built application, including a Codex session's
      hooks, MCP tools and queue delivery after a `kill -9` and after an update restart.
- [ ] The same on the Windows VM (the holder detached and under the desktop application, the port read from the pipe,
      the installer meeting no file in use).

## Decisions made without the user

- The daemon reaches the server directly at the endpoint the holder reports, rather than relaying JSON-RPC through
  the holder link: it reuses the whole RPC layer and its forty tests, keeps RPC latency apart from the link's output
  flow control, needs four request frames instead of a stream multiplexer, and costs nothing in trust (same user,
  same socket directory rules; on Windows a token over the link that already carries launch keys).
- The holder's primitive is a provider-neutral companion (an endpoint to open, a command to start with a readiness
  sign, an end), bound to the terminal by its id, so Codex's arguments, the hash of the token and the port line's
  format stay in the daemon, and the holder of this build serves whatever server a later daemon asks for in these
  terms.
- The holder enforces both directions of the lifetime itself, since no daemon may be linked when either end comes,
  and tells a linked daemon of a companion's death so it acts at once rather than at the terminal's exit.
- An unbound companion dies with the connection that asked for it: a launch straddling a link drop fails, as a
  spawn whose confirmation is lost already does, instead of a welcome listing servers nobody owns.
- The thread is re-identified from the session record's agent session id first, the existing discovery and thread
  watch after, rather than widening the terminal's meta: the meta is written once at spawn and the thread is often
  not known then.
- Ownership of queued messages is derived from the id the outbox already gives them, with the ticket counter seeded
  from the clock, so no transport state has to be saved for a restart; the one duplicate left (a message Codex ran
  between taking it under overload and the successor's retry) is stated rather than closed with a saved flag.
- The local backend keeps the daemon-owned server: there the terminals die with the daemon anyway, and the
  transport tests run on it.
- Old holders get the ordinary launch for Codex; neither a flag in the meta nor a rule in `handover_check` is added
  for the unreleased builds whose daemon-owned server sat behind a held terminal.
- **Choices left to the implementer by the design**: unbound companion bind deadline 120 s, companion stop grace
  2 s, the terminal's end 3 s after its companion dies, companion exit poll 100 ms; a spawn over a link other than
  the one that opened the companion is refused; the holder ends terminals and companions in parallel when asked to
  end; the holder does not exit while a companion's end is under way.

## Known gaps

- No test drives `session::term::launch` end to end with a holder (its parts are tested
  separately); the 120 s bind deadline has no test override; the generation refusal of a spawn has no direct test
  (the takeover test cannot tell it from the missing-companion refusal); an end that comes while a companion is
  still starting waits for its readiness wait (up to 15 s) before the process is ended, the holder staying up
  meanwhile; Windows paths have unit tests only for the refused socket endpoint.

## Notes for the developer

- **Reusable capabilities**:
  - The private listen of the platform's IPC module (the owner-only socket directory and the random token), moved
    under the holder's ownership for the held path and kept for the local host.
  - The process layer: `Reach::Session`, `ChildHandle::started`, `Containment::join_launch`, the crash table and the
    polite stop with grace, which both hosts start and end the server with.
  - The holder link's request and reply plumbing (`req` numbers, `request_as`, the pending and lost states) and its
    routing by terminal id, for the companion frames and `CompanionEnded`.
  - The in-process test holders of the holders' client and the fake app server of the transport tests, which the new
    tests drive together.
  - The Codex RPC endpoint and connection (`Endpoint::connect`, `client_args`, `client_env`), used unchanged by `adopt`;
    `listening_port` is test-only now, the host having taken the port from the server's output.
- **Development notes**:
  - The protocol's rule: a message may gain fields and an enum variants, never lose or change them; every new field
    is `serde(default)` and every growing enum has a `serde(other)` variant; a daemon sends a companion frame only to a
    holder whose welcome advertises companions.
  - The holder has to run a `Command` as a plain child with pipes, which the terminal spawn does not need today: an
    accessor for that is the one new seam in the platform's terminal command type.
  - In-process test holders share the test's pid, so ending a holder by its process is not covered by unit tests
    (a holder's process is the test's own); the companion tests end things through the link.
  - Codex's `clientUserMessageId` is opaque to Codex; the prefix rule relies on the outbox naming messages
    `<launch id>:<ticket>` and on nothing else queuing ids of that shape.
  - The product docs state that the server is ended when the terminal exits and the terminal when the server exits;
    the statements stay true with the holder as the actor, and gain the restart cases.
- **Reference docs**: `apps/daemon/src/holder/README.md`, `docs/product/process-holders.md`,
  `docs/product/per-agent-launch-specifics.md` ("Codex's private app server"), `docs/product/message-delivery.md`
  ("Messages through Codex's own queue"), `docs/product/background-processes.md` ("Automatic stopping and its
  boundaries"), `docs/architecture.md` ("Process holders on Windows"), `docs/structure/daemon.md`,
  `02-process-holders.md` in this directory.

## Mid-task handoff

**State.** Everything is implemented. On 2026-11-10, on macOS, with real Codex 0.166.0: the server ran as a child of
the holder in the launch's containment, leading a session of its own, with the terminal attached through `--remote`
and automatic messages in Codex's queue; after `kill -9` of the daemon, and across `octoboardd restart` and a real
beta.4 to beta.5 desktop update, the server survived, new messages went into the queue, a message sealed into the
restart outbox was delivered once by the successor, and after successive cancels each queued id started exactly once;
exiting Codex ended the server and removed its socket; killing the server with no daemon linked ended the terminal;
a holder whose welcome did not advertise companions, and a server that did not come up, gave the ordinary launch. On
the Windows VM, over SSH only: the server and the remote terminal ran under the holder with the port and token path
working, both in jobs with the holder outside them; the session and MCP delivery survived a daemon kill; the server
survived a planned stop and a start and took a later queued message; terminal exit ended the server. Evidence, on the
verification Mac: `/tmp/octoboard-manual-20261110-67e2/REPORT.md`, `/tmp/octoboard-gui-20261110-67e2/REPORT.md` and
`/tmp/windows-verification-evidence/REPORT.md`. The scripted suites (`holder::protocol::tests`,
`holder::server::tests`, `holder::client::companion_tests`, `holder::client::tests`, `platform::proc::server::tests`,
`terminal::server_host::tests`, `terminal::backend::tests`, `messaging::outbox::tests`, `restart_outbox`,
`adapter::codex::transport::tests`, `coordinator::adopt::tests`) run at push.

On 2026-11-10, main `035746de6` also passed the pending-holder direct-connection run on macOS with real
Codex 0.166.0, using an isolated local model provider. Evidence: `/tmp/octoboard-remaining-20261110-7fd0/REPORT.md`.

**Windows SSH follow-up, 2026-11-10.** On local main `035746de6`, rebuilt inside the VM, with real Codex 0.166.0:

- [x] A message accepted before manual planned-stop/start stayed queued with the same client ID, then started exactly once after cancelling the active turn — `/tmp/windows-ssh-remaining-evidence/queue-rollout-final.json`.

The session was first taken out of command approval; the successful enqueue used a working text-only turn with no
approval screen. The queue contained the same single entry before stop, offline and after start, and was empty after
cancel. The final thread and rollout contained one UserMessage with that client ID and one final marker reply.
Full account and limits: `/tmp/windows-ssh-remaining-evidence/REPORT.md`, "08 Codex queued message". This was session 0,
not a desktop update, and did not exercise the sealed restart outbox race or a pre-kill queued message.

**Forced-holder companion cleanup follow-up, 2026-11-10.** The orphan reported in 02 is fixed. A server now starts
atomically inside an owner-only private kill-on-close job on Windows; Unix installs a double-forked pipe watcher before
exec, killing the server group if its owner disappears. The shared launch containment retains its keep/carry semantics.
Startup also discovers a lost holder's launch through its held record even when no task was attributed in the previous
ledger, preserving both recorded keeps and unknown keep decisions on an unreadable ledger.

The Windows SSH run reproduced an old holder's orphan with real Codex 0.166.0, then verified fixed startup cleaned it
and ordinary shutdown left none. A fixed holder's server and remote terminal survived planned daemon stop, ended on
forced holder termination offline, and stayed gone through startup reconciliation and shutdown. On macOS the real
app-server and a bound shell PTY ended after SIGKILL of the holder, without a daemon; terminal exit ended the server,
and server exit ended its terminal. Four new exact lifecycle tests passed on each host, three Windows startup/keep
regressions passed, and 12 concurrent exact lifecycle runs passed on each. Evidence and build hashes:
`/tmp/companion-orphan-evidence/REPORT.md`. No full suite ran; this closes the forced-holder orphan observation, not the
independent desktop or queue criteria below.

**Remaining, a real run on Windows from an interactive desktop logon**, blocked on the sign-in password of the VM user
`bjj`; the VM's working directory `C:\Users\bjj\windows-verification` keeps the source, the build and the isolated data,
and Codex is signed in there. With the daemon started by the desktop application: the holder detached and running the
server, the companion in a job of its own nested in the launch's (over SSH it was seen only in a job); a message queued
before a daemon kill started once after a cancel; the sealed restart outbox race; and a real desktop update restart
over a working Codex session, with the installer meeting no file in use. The planned-stop/start queue-and-cancel case
was established over SSH in the follow-up above. The companion left by forced holder termination was fixed and
reverified separately in the follow-up above.
