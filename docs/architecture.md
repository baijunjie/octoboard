# Architecture

Why Octoboard is built the way it is: what it is, how it is layered, the technology choices, the constraints that keep
the design open, and the traps found along the way. What the product does is in `docs/product/`; what the agent CLIs do
is in `docs/agent-cli-reference.md`; the technology stack and the code layout are in `docs/project-map.md` and the
per-end structure docs under `docs/structure/`. This document keeps only the reasoning and the constraints, not a
restatement of the code.

## Positioning

Octoboard is a control board for orchestrating agents across several projects. In a **console** the user gives a
request to a console session; the console session decides which project it belongs to, starts a dedicated agent
session in that project's directory and hands it the task; the session reports back to the console session, and the
console session reports to the user. The user can step into any session at any time to watch it or take over.

It is neither a new agent nor a new terminal. Every session is a native agent CLI process (Claude Code, Codex, Grok
Build), and Octoboard only adds organization, orchestration and observation on top. It is not tied to any provider, and
the retention and cleanup of session data follows each agent's own rules.

### Two deployment forms

Octoboard is deployed in one of two forms, and the other documents name them as here:

- **The desktop form**: the macOS and Windows applications. The window, the daemon and the projects are on one machine, used by
  one person, and the daemon starts and stops with the application.
- **The server form**: the daemon alone, on a machine without a display — a Linux server, Ubuntu say — started and kept
  running there, listening on the server's loopback on a fixed port, and reached by its owner from another machine over
  SSH: from a browser through the owner's own SSH port forward, or from the macOS or Windows application through an SSH
  tunnel the application builds itself. The machine showing the window and the machine holding the daemon and the
  projects are two different ones. This is what Linux is for: there is no Linux desktop application.

In both forms the daemon **never listens beyond loopback**. Reaching it from another machine — the server form, or
another device of the owner's reaching their desktop — goes through SSH, which authenticates and encrypts; Octoboard
builds no network surface, TLS or device credential of its own (see "Constraint: access from another machine goes
through SSH").

Both forms are **one user's**. The server form is the same user reaching their own daemon from elsewhere, not a group
sharing one daemon: the daemon does not tell clients apart, whoever reaches it with its key is taken for its owner and
can do everything the owner can — and from another machine that means whoever can log in over SSH as the daemon's Unix
user, so that login is to be kept like a password. That is this version's design boundary, not a gap left for later;
what several users would take is in "Why the daemon is split out".

The server form is **not built yet**. Today the daemon binds to loopback only, on a fixed port when it is given one
(`--port`) and runs in the background from the command line or as a systemd user service, but it does not serve the UI,
a daemon run as a service has no way yet to hand its key to its owner, and the application cannot connect to a remote
daemon. What the form asks of the design is stated below, where each constraint comes up.

"Server" names where the daemon lives: a machine kept running for its owner to reach from elsewhere, with nobody sitting
at it. The word *host* is not free for it. A host is any machine that runs sessions — the server is one, and
so is the desktop — and a *remote host* is a different arrangement again: a second host, reached by the daemon the
user is talking to, so that sessions run on a machine other than the daemon's (see "Why the daemon is split out"). The
server form is the reverse: one host, and what is remote is the clients.

## Core concepts

| Concept | Description |
|---|---|
| Console | A management unit for a group of projects, independent of other consoles. It has its own working directory and its own default agent, which its console sessions run and the sessions under its projects fall back to. |
| Host | A machine that runs sessions: the user's own machine in the desktop form, the server in the server form. Only the daemon's own machine exists today; every project and session still carries a `host_id`. |
| Client | A window or browser tab the user operates the daemon through: the desktop application's window, the application on another machine reaching the daemon through its SSH tunnel, or a browser reaching it at a loopback address (on the daemon's machine, or on another one through an SSH port forward). Any number of them may be connected at once — the desktop window and a browser tab on a laptop, say — and the daemon does not tell them apart: every client is the same user, sees the same consoles, projects and sessions, and may do everything. |
| Project | A directory on some host, associated with a console. |
| Session | An agent CLI process running in a console or project directory, created by the user or a coordinator session. |
| Console session | The agent process running in the console's working directory, given the orchestration tools. |
| Coordinator | A session that can create and manage child sessions. A project coordinator works independently or reports to a console session. |
| Worker | A project session that reports to a parent session and cannot manage child sessions. |
| Agent provider | The layer that plugs an agent CLI in: its descriptor and its behavior (how to launch it, inject capabilities, report status, read its records and screens, and drive its accounts). |

Each agent provider is defined in two places in the daemon: a static descriptor (`providers/`), registered below
the wire layer so every layer can read it, which holds its names, install URL, binary, where it keeps its files on disk
(its configuration directory and the variable naming it, where a session's records lie in it, the files kept beside
them), how a project marks itself as set up for it, the names it gives its instruction file and Octoboard's tools, and
the capabilities it declares; and a behavior interface (`AgentProvider` in `adapter/`), which covers launching it,
injecting Octoboard's capabilities, reading its titles, and gathers the sub-interfaces a provider implements in its
own modules: interpreting its hook events, finding, reading and watching its own records of a session (its record
store), reading its terminal screens and dialogs, its accounts (sign-in, quotas, reset credits, account switching),
its model catalogue, its status line and, for a compatibility source, what it offers a provider made compatible with
it. Every method beyond the core has a default (a no-op or "unsupported"), so a provider implements only what it has.
Core code branches only on the capabilities and data a provider declares, never on which provider it is, so adding a
provider means writing its own modules and registering them. Clients read the descriptor data the daemon serves, and
keep only each provider's presentation (its mark, colour and messages); only the daemon launches and drives the
agents. An agent without hook support would degrade to an ordinary terminal session
with no status and no automatic reporting.

## Layers

```
   The desktop form                            The server form
┌──── Desktop application (Tauri) ────┐   ┌──── Browser or app elsewhere ────┐
│ menu tree · terminal · report panel │   │ the same UI, for the same user   │
└──────────────────┬──────────────────┘   └────────────────┬─────────────────┘
          on the same machine                   over SSH, to loopback
                   └───────────────┬───────────────────────┘
                                   │ WebSocket
                        ┌──────────▼──────────┐
                        │ Daemon              │
                        │ coordinator + host  │
                        └──────────┬──────────┘
                              agent processes
```

`octoboardd` is a headless, long-running Rust process with two roles:

- **Host role**, present on every machine that runs sessions: manages PTYs and agent processes, receives hook reports,
  serves MCP for local sessions, lists directories, clones repositories, and probes remotes and directories for the
  agent a project is set up for.
- **Coordinator role**, on the machine the clients connect to — the user's own in the desktop form, the server in the
  server form: stores console, project, session and report-page data, executes the console session's orchestration
  requests and routes them to the right host, and handles report delivery and synthesis.

A client — the desktop application's window, or a browser — is only that: it renders terminals, shows status and
forwards input.

The UI is not part of the desktop shell. It is its own package, built once, which the desktop shell bundles and loads and
which also runs in a plain browser; that is what the server form rests on — a Linux server has no shell and offers the
same UI to a browser. To keep one build working in both places, the UI reaches native capabilities (the quit flow and
exit heartbeat, system notifications, the Dock badge, the native window's own appearance and visibility, the window
chrome the top bar must keep clear, the app menu's items and the menu bar icon's menu) only through a platform adapter
chosen at startup, with a Tauri implementation and a browser one; in a browser a capability that is not there means
the feature is absent, not an error (copying instead falls back to the browser's selection copy). It finds the daemon
without Tauri as well: by an address handed to it, or at the origin it was loaded from. Its state is one store fed by
the daemon client's events, readable outside React too. The phone's browser is not a design target — the native mobile
apps have their own UI.

**A browser is a client Octoboard does not control, and it grants two of the capabilities the UI is built on only to a
secure context**: the Notifications API, which the raised hand's notification goes through, and the async Clipboard
API. A page over plain `http` is a secure context only at a loopback address (`localhost`, `127.0.0.1`); at any other
address it is not, so a browser reaching a server over bare `http` never tells anyone that an agent is waiting — and
the whole point of the orchestration is that an agent raises its hand and a person answers. This is one more reason the
daemon stays on loopback: through an SSH port forward the browser opens `http://localhost:<port>`, a secure context with
no TLS, and the UI is to be served from that same origin as the daemon's WebSocket endpoints (the daemon does not serve
it yet).

## Why the daemon is split out

Session processes are owned by the daemon's side rather than by the window: the daemon runs them in process holders
(below). That is what lets the daemon run with no window
at all — the server form, where there is no application to start or stop it — and, in the desktop form, what would
let it outlive the application (background operation; today it exits with the application there). **Remote hosts** —
a daemon reaching a second host to run sessions there — are **not built yet**, and no workflow for them is settled.
They are not the server form: the server is a single host, and what is remote in that form is the clients.

### Process holders

A **holder** is a process, a mode of the daemon binary, that owns the terminals and is the parent of every agent and
shell-terminal program, so the daemon can go down and come back while the programs keep running. The daemon drives
it over a WebSocket link on loopback; the holder keeps no screen, it forwards the output and keeps a raw tail per
terminal. These choices are not visible from any one module:

- **Holders, not a handover of PTY handles between daemons.** One mechanism covers a crash, a planned restart and an
  update: a handle handover covers only the planned moment (a crash closes the master), and on Windows a
  pseudoconsole lives as long as the process that created it, with no API to move it. The cost is a holder protocol
  that later builds must keep speaking, and one more local hop per byte.
- **One holder per build, as the usual allocation.** New terminals go to a holder of the daemon's own build; a holder
  of another build keeps the terminals it runs, gets no new ones and exits after its last. It is not an invariant: a
  start keeps one holder of its build for new terminals and drains any other, and a holder that cannot be linked to
  for a while is replaced by a new one for new terminals. One holder carries many terminals over one link, so a
  terminal whose output nobody takes (its reader stopped) holds up the link, and with it the other terminals of that
  holder and the replies to the daemon, until it is read again: a stated limit. A daemon adopts holders from the
  previous stable release up to its own build and ends any other with its programs: a session whose helpers could not
  reach the new daemon would be silently broken. The protocol ignores message types and fields it does not know, so
  builds within that range keep talking; it is kept compatible within that promised range, and the range's floor only
  moves by the previous-stable release rule (`apps/daemon/src/holder/compat.rs`).
- **Helpers reach the daemon through their holder.** A session's hooks, MCP bridge, git gate and status line get the
  holder's port and a key per launch, which stay valid as long as the holder, and the holder relays to whichever
  daemon is linked. Reports wait for a daemon; a call its caller waits on waits up to a deadline that covers an
  ordinary restart. A helper and its holder come from the same build, so the only interface that crosses builds is
  the holder link. Persisting the daemon's port and key instead would lose what is sent during the restart and leave
  a port another program could take.
- **Holders and helpers run from a copy of their build** under the data directory (`builds/<build>`), removed once no
  holder of that build runs. Helpers start afresh on every call, so without the copy an update replacing the
  installation would run new helpers against an old holder; on Windows the copy also leaves the installation folder
  free for the installer.
- **Adoption is approximate, except after a planned stop.** A daemon that starts adopts what the holders run, rebuilds
  each screen from the raw tail, marks it inexact (`screen_inexact` on the wire) and resizes the program by one row and
  back so a full-screen program repaints. Terminals no record claims are ended, sessions no holder runs are
  interrupted. Only an ordinary stop ends every session. A planned stop (a planned restart, or a stop with no
  successor) instead has each holder mark where its output to the stopping daemon ends, saves each screen once it has
  read up to that mark, and leaves everything running; the next daemon restores a saved screen it verifies (the same
  handover token, a whole tail since the mark, the same size) and replays only the tail after the mark, with no mark
  and no redraw, and never clears the mark of a process already inexact. Anything it cannot verify falls back to the
  approximate path.
- **A transport failure never ends a holder.** A refused connection, a handshake that times out and a link that keeps
  dropping say nothing of whether a holder can go on, and one holder carries many terminals. While its registry file is
  there and its verified process runs, the daemon keeps trying to link to it with a growing pause (up to 30 seconds),
  its terminals running unreachable and its sessions claimed (`holder_link: "pending"` on the wire): not interrupted,
  resumed or cleaned up after. To know exactly which sessions such a holder runs, the daemon records which holder runs
  each launch (`Store::held_terminals`); a session no record places stays claimed too while such a holder is left, and
  resuming the interrupted sessions waits for that. Only a holder known to be gone (no registry file, or its process
  gone), one outside the compatible range, one whose welcome cannot be read, and the daemon's ordinary stop end what it
  runs.
- **The holder answers a few queries itself while no daemon is linked.** A program waits on its terminal's answer to a
  query, and the bundled Windows console host holds it until one comes. The holder keeps no screen, so it runs a
  bounded streaming recogniser on the output and answers, while no daemon is linked, only fixed queries whose answer
  needs no screen, with the daemon's own answer: the primary device attributes, for now. The cursor position and every
  other screen-dependent query go unanswered and wait out the host's hold, a stated limit. Which side answers a query
  is decided in output order, under the same lock a link attaches under: what is read while a daemon is linked is that
  daemon's, and a tail handed to a daemon is never answered from, except the part after a planned stop's mark when its
  screen is restored exactly: there only the queries still fresh (about a second) are answered, and never a kind the
  holder answers itself.
- **A holder's crash is the limit of crash survival**: its programs end with it, through its own crash table and the
  terminals closing.

**Constraint: the UI and the daemon interact only over the network protocol (`apps/daemon/PROTOCOL.md`) — no Tauri IPC,
no shared state.** Hooks and the MCP server of a session point at the daemon on the host the session runs on. The server
form is what makes this a hard constraint rather than a tidy one: a client on another machine has nothing but the
protocol, carried through SSH. What any client of that protocol must implement — framing, replies and broadcasts, absent
and null fields, the key, and the protocol generation the daemon's first frame states — is PROTOCOL.md's "What every
client implements".

**Constraint: access from another machine goes through SSH.** Anyone who reaches the daemon can start agents and type
into their terminals, or open a shell terminal and run any command, as the daemon's own user. It therefore binds to
loopback only, in both forms, and the access guard (`access.rs`) with the per-launch key keeps out web pages on other
sites and pages served from `localhost`. Another machine reaches it only through SSH — the owner's port forward for a
browser, the application's own tunnel for the application — so SSH does the authentication and encryption, and the
daemon gains no external surface, TLS or device credential of its own. A device is let in by its SSH key and revoked by
removing that key (a connection already open lasts until it drops); such a key identifies a device of the owner's, never
a person: the daemon stays one user's, and every client is that user. What the daemon must still provide is a way for
its owner, logged in as its user, to obtain its address and key, including for a daemon a service manager runs; that
does not exist yet.

**Known gap: other users of the same machine share its loopback.** The key is what keeps their processes out, so it must
reach only the daemon's own Unix user. Today keys the daemon accepts are carried in the command lines of a session's
agent CLI (its hook and MCP configuration) and of the `octoboardd` helpers it runs, which other users of the machine can
list, so a server shared with other users is not yet protected from them.

**Known boundary: several users on one daemon are outside this design**, and authentication per person would be the
smallest part of it: every session runs as the daemon's own Unix user, with that user's agent logins, quotas and SSH
keys, and a project is one checkout that two agents would overwrite each other in, so each user would need a working
tree and an operating-system identity of their own; a shared terminal would need an owner of its input; and trust
confirmations, settings and sessions would each need an owner and an authorization. None of it is planned.

## Technology choices

| Layer | Choice | Why |
|---|---|---|
| Daemon | Rust + `tokio` | A single binary that can be deployed straight to a Linux server, which is the whole server form. |
| PTY | `portable-pty` on Unix; a pseudoconsole (ConPTY) backend of the daemon's own on Windows | `portable-pty` is from the WezTerm project. On Windows it gives no process handle before the process runs and takes no second thread attribute, so the process could not be placed in its job before its first instruction (see "Running on more than one operating system"). |
| MCP server | `rmcp` | A stdio child process per session, bridging to the daemon over loopback HTTP; see below. |
| Desktop application | Tauri 2 | The daemon ships as a sidecar; native macOS and Windows APIs are callable from the Rust side. |
| Frontend | React + HeroUI + Tailwind CSS + TypeScript, state in a Zustand store | One component library for the whole UI; a vanilla Zustand store can be read and subscribed to outside React. |
| Terminal | `xterm.js` | macOS cannot embed a Terminal.app or iTerm window in another application; PTY + `xterm.js` is the standard approach and terminal compatibility is its job. |
| Report panel | a component tree the daemon validates, rendered by the UI's own React components (ECharts, loaded on demand, for charts) | A page holds no markup or script of an agent's, so nothing needs a sandbox. |
| File viewer | `@pierre/diffs` (Shiki on its JavaScript regex engine) | One library for files and diffs; loads offline in the packaged WebView under the app's CSP, no WebAssembly. Behind an application-owned interface. |
| Storage | SQLite (`rusqlite`, coordinator side) | — |

## Running on more than one operating system

The desktop form runs on macOS and Windows and the server form on Linux, from one daemon code base. These decisions keep
it that way, with the reasons that are not visible in the code.

- **Every OS-specific token lives under the daemon's `platform` layer**, and under per-platform modules of the desktop
  crate; a test enforces it (`docs/structure/daemon.md`, "OS-specific tokens"). Scattered OS code cannot be audited, and
  a rule a test can check cannot erode. The Windows port was built by moving each piece behind a neutral signature on
  macOS first with behaviour unchanged, so the existing suites verified each extraction on a platform that already
  worked, and with a Windows compile check from the first step, so the signatures were shaped against a real second
  target before any Windows code was written.
- **Business code branches on a declared fact, never on the OS.** Where behaviour must differ by host, the platform
  layer states a fact and business code reads it, the rule providers already follow with declared capabilities:
  `HostFacts` (the host's OS, path style, file links and the words it uses) goes to clients in the snapshot, the desktop
  shell reports the client's own to the UI, and `TermFacts` describes the terminal backend. A fact belongs to whatever
  decides, which need not be the OS: `TermFacts` is the facts of the console host that started, because Windows has two
  of them. A needed branch on the OS in business code means the platform signature was wrong, and the signature is what
  gets fixed.
- **A trait appears only where a test needs a fake.** Platform modules are chosen at module level, as the process-table
  reader always was, so a platform has no dynamic dispatch and no second implementation to keep in step.
- **Semantics over bytes.** Where the host renders the stream (Windows' system console host), terminal state is read
  from the parsed screen, the title event and the agent's hooks, and where the stream is the program's own bytes each
  reader keeps what it reads today. The daemon's own writes into a terminal are semantic inputs (a paste, a key) that
  each backend encodes. The stream of the system's console host is its repaint of a screen, not the program's bytes:
  titles collapse and repeats drop, the alternate screen is not passed, frames between two paints are lost, and a phrase
  is split by cursor moves, so only the parsed screen holds what is drawn.
- **One normalisation point per boundary for paths.** Paths on the wire are made and validated in one place
  (`platform::names`); an absolute path of the daemon's host travels in that host's native spelling with the host's path
  style announced once in the snapshot, and clients only show it or split it for display; the UI has one path module. A
  name is its bytes in the host's native encoding, which on Windows is the WTF-8 of the UTF-16 name, so a name with an
  unpaired surrogate survives the trip. A Windows host shows full paths and never `~`: it is a PowerShell habit that
  Windows' own tools do not share.
- **The data directory is `%USERPROFILE%\.octoboard` on Windows**, the same place relative to the home as `~/.octoboard`
  elsewhere. The agents keep `~/.claude` and `~/.codex` under the home on Windows too, so one rule covers every host and
  nothing is split between `%APPDATA%` and `%LOCALAPPDATA%`.
- **The Windows window uses the system's title bar and frame, and has no application menu.** The native frame gives
  snap, maximise and accessibility for free (an overlay variant is polish that is not scheduled). Settings, About and
  Quit are reachable from the rail and the notification-area menu, whose plain text items come from the same model as
  the macOS menu bar icon's, and the shortcuts are registered by the page, which also handles them wherever there is no
  native menu.
- **Windows 10 21H2 is the floor and Windows 11 the verified target.** ConPTY needs Windows 10 1809 and WebView2
  Evergreen is required. Windows 11 is where the ConPTY behaviours the terminal layer depends on were fixed and
  measured, and the bundled console host (below) is what takes the terminal's behaviour out of the OS version's hands.
  Nothing was measured on Windows 10 or on x64 hardware. Windows builds are x64 only, and on Windows on Arm the app and
  the daemon run under emulation. Git for Windows is a requirement, since both Claude Code and Codex need it for their
  own shell tooling and Octoboard's git hooks run under its `sh`.
- **Windows is checked from a Mac and on a Windows runner.** `pnpm check:windows` cross-checks both crates through
  `cargo xwin` against the MSVC target, and the `windows-check` job of `quality.yml` lints and tests them natively on
  `windows-latest`; a `windows-bundle` job builds the unsigned installer as the release builds it. Spikes and debugging
  run in a Windows 11 virtual machine, and a native x64 machine is what a release is spot-checked on.

### The two Windows console hosts

The daemon bundles Windows Terminal's `conpty.dll` and `OpenConsole.exe` (MIT-licensed, as VS Code and WezTerm do) and
loads them from beside its executable, with the system's pseudoconsole as the fallback. The system's host changes
behaviour from one Windows version to the next, and the bundled one passes the program's own bytes through, so every
reader that works on the byte stream elsewhere works unchanged. The two hosts need opposite `TermFacts`, which are
decided once from the files present before any terminal starts:

- The **system's host** (`conhost.exe`) repaints: its paints come about every 16 ms and the frames between are dropped,
  the alternate screen (1049) is not passed, a title becomes OSC 0 with repeats dropped and, before the program sets
  one, the program's path, and it answers device-attribute, status and cursor-position queries itself. A resize repaints
  the screen, and `ClosePseudoConsole` blocks while its output pipe is not drained. This is the fallback, read off the
  parsed screen.
- The **bundled host** is byte-identical and in order, one read per program write. It forwards every query to the output
  and holds the console, about half a second for each, until it is answered; its own start-up queries and its requests
  for focus events and win32 input mode (`ESC[1t ESC[c ESC[?1004h ESC[?9001h`, sent again after a program turns them
  off) are for the client side, except the device attributes query, which the daemon answers like any other. A query nobody answers stalls the agent, and Codex then loses the submit of every
  message sent to it. The daemon answers the queries its own emulator can answer the moment it reads them, attached or
  not (see "Answers to the program's queries" in `apps/daemon/PROTOCOL.md`), which keeps that stall short.
- A `conpty.dll` with no `OpenConsole.exe` in its own directory or in the subdirectory named for the machine's native
  architecture silently starts the system's host instead and behaves exactly as it. The daemon therefore looks at which
  host it started and warns when it is not the one the facts were decided for. The layout the installer ships is in
  `docs/structure/daemon.md`.

### A polite stop on Windows

A stop asks the agent to end before it ends it: the quit keys its provider declares, pressed through the terminal, then
`ClosePseudoConsole` on a thread of its own, then each member of the launch's job terminated by handle, skipping what
was kept and the console hosts, and never by `TerminateJobObject`, which would take the kept trees with it. The keys are
part of the provider's description (`quit_keys` on the descriptor, not on the capabilities that go on the wire), so no
provider's name appears in the shared stop path. They exist because closing the console is only a `CTRL_CLOSE_EVENT`,
which Claude Code answers with a polite exit (its partial reply flushed, `SessionEnd` fired) and Codex does not answer
at all: it dies at once with no `SessionEnd`, no `turn_aborted` and the turn in flight lost, so Codex declares Ctrl-C
twice, 650 ms apart, and a wait of six seconds ("Stopping an agent on a pseudoconsole" in
`docs/agent-cli-reference.md`). `CTRL_BREAK_EVENT` is not sent: it behaves like the close, and it needs the daemon to
attach to the console, which is process-wide state in a multi-threaded process. macOS ignores the keys and keeps its
signal.

### Process holders on Windows

What follows records how holders (see "Process holders" above) were measured to behave on Windows, which settled the
detach and the file location they use. The holder is started as measured: detached and breaking away from the
daemon's jobs, and where breakaway is refused, created with the desktop application as its parent process; with no
application to name, it starts inside the job and its terminals end with it, which is logged. That survival inside
the job is not guaranteed, and neither is the parent fallback's when the desktop application is itself in the same
outer job. The detached launch and the refused-breakaway fallback were also confirmed from a standard user's
interactive desktop logon: the desktop application started the daemon, and the holder stayed outside its jobs and
survived it. The measurements below retain the scope of the original OpenSSH harness.

A pseudoconsole lives as long as the process that created it and its handles, not as long as the process that started
that process, so a holder that owns the pseudoconsoles can keep a session's programs running while the daemon is gone.
This was measured with a throwaway harness on Windows 11 (ARM64, the x64 binaries under emulation), run over an
OpenSSH session (session 0, with no desktop logon), not from the desktop application. Its daemon stand-in was started
with a hidden console of its own, as the sidecar is. With both the bundled and the system's console host, a daemon
that exited normally and a daemon terminated outright each left the holder, the host and the agent running, cmd.exe and
Claude Code alike, and the agent still took input and gave output through the holder afterwards.

- **The detach.** A holder started with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` and no inherited handles has no
  console to lose when the daemon's hidden console closes, and the console host its pseudoconsole starts is its child,
  in its jobs rather than the daemon's. It also has to leave every job of the daemon's, since a kill-on-close job that
  closes ends its members. Where the job allows breakaway (the job an OpenSSH session runs its processes in does),
  `CREATE_BREAKAWAY_FROM_JOB` did it. Where it does not, that flag is refused with access denied, and a holder created
  with `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS` naming a process outside the job inherited that process's job membership
  instead, so it was in none of the daemon's; the parent named was a stand-in, since no `explorer.exe` ran without a
  desktop logon. Both holders kept running after the daemon's job closed. With no process outside the job to name, a
  holder cannot outlive it. The daemon's launches themselves still never ask to break away (see "A daemon inside a
  kill-on-close job" in "Known pitfalls of the Tauri / Rust approach").
- **A holder's own death ends its programs.** Terminating a holder closes its pseudoconsoles, and every process on them
  gets `CTRL_CLOSE_EVENT`: cmd.exe and the host were gone within about 0.4 to 1.5 seconds, Claude Code, which answers
  the event, within about 6 to 7 seconds. A holder's crash is therefore the limit of crash survival on Windows, as
  closing a console is for a launch today.
- **The installer and files in use.** The NSIS installer cannot replace a file a running process holds: an executable
  being run (`octoboardd.exe` as the MCP bridge, and so a holder in that binary too; `OpenConsole.exe` as a running
  host) or a DLL being loaded (`conpty.dll` in the holder). It killed none of them; it stopped on an "Error opening
  file for writing" box with Abort, Retry and Ignore, which in the updater's passive mode (`/P /UPDATE`) waits for an
  answer, and Ignore left the old file in place, a mix of two builds, with exit code 0. Hooks and the git gate are
  started afresh from the binary on every call, so they run whichever build is in the folder at that moment. With the
  holder, its host and the MCP bridge running from a copy of the build outside the installation folder
  (`octoboardd.exe`, `conpty.dll` and both `OpenConsole.exe` in their subdirectories, as the installer lays them out),
  the installer replaced every file it ships with no box, and the copy's holder, host and agent kept running and kept
  relaying input and output, and its MCP bridge kept running. No daemon ran during the install, so hooks and the git
  gate called from the copy during and after it showed only that the binary could be started from the copy
  throughout. The copy was measured under the data directory (`builds\<version>` in `%USERPROFILE%\.octoboard`), which
  the installer does not touch.

## Orchestration design decisions

- **Role and reporting relationship are separate.** Coordinator and worker describe whether a session can manage
  children; independent or reporting to a parent describes who directs it. A coordinator can have a parent and no
  children yet, so neither the presence of a parent nor the current child count determines its role. The daemon
  derives the role from its stored session state and sends it to clients explicitly, alongside the parent id.
- **Capabilities are injected per launch, never by editing the project's or the user's files.** That means arguments,
  an environment variable, or files Octoboard writes itself (for Grok a per-session `GROK_HOME` holding Octoboard's
  `config.toml` and `trusted_folders.toml` copies and `hooks/`). Every session starts with the project directory as its
  cwd, so the project's own instruction file, skills, hooks and permissions apply as they normally would. The mechanism
  differs per agent ("Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`).
- **The MCP server is a child process, not an endpoint the agent dials.** The daemon binary doubles as it
  (`octoboardd mcp`), speaking MCP on stdin/stdout and forwarding each call to the daemon over loopback HTTP. A
  Streamable HTTP endpoint in the daemon was ruled out by two of the three agents: Codex's verified MCP injection takes
  a `command` and `args`, and Grok's SSRF protection rejects plain HTTP and private addresses alike (also why Grok's
  hooks cannot be HTTP). One mechanism for all three beats a bridge for some. The session's identity travels in argv
  because Codex exports nothing to an MCP child and Grok's `{{session_id}}` templating does not work; a token issued per
  launch and dropped with the process means a local process that guesses the port cannot reach the tools, and the
  daemon resolves the calling session from the token rather than from an argument.
- **Structure in tool arguments, natural language in content.** Agents are not asked to emit JSON in reply text, where
  it mixes with prose and code fences; tool arguments are validated against a schema by the agent runtime. Fields a
  program acts on (status, open items) are arguments; what an agent must understand (the brief, a summary) stays prose.
- **Reporting is not forced.** Blocking the stop through the `Stop` hook works (verified on Claude Code and Codex), but
  every gated turn shows an error-styled `Stop hook error` line that `suppressOutput` does not suppress, and the model
  sometimes reads the injected demand as prompt injection and refuses — too costly for a product meant to feel calm. So
  the role description encourages `report`, nothing is blocked, and the daemon synthesizes a report for a session that
  reports to the console session and stops without one. The price is that `status` and `open_items` degrade to prose
  on those turns.
- **Information between sessions is a channel of its own, because "not an instruction" cannot be enforced.** A message
  is text written into the receiving terminal, so any session that can write to another could steer it. Commanding is
  therefore the only hard-enforced boundary (`instruct_session`, `archive_session` and `resume_session` act on the
  caller's own sessions). Sharing information is a separate tool, offered to every session and reaching any running
  session in any console or project, and keeping what it is given for an interrupted session until that session is
  resumed (see "Information for an interrupted session" in `docs/product/information-between-sessions.md`); it is
  delivered under a header naming the sender and saying the text is information to weigh, role descriptions tell each
  session whose work it takes, and the receiver's owner is copied so it is never blind to what reaches its sessions.
  Octoboard makes the distinction plain; whether a model keeps to it is the model's call.
- **A session whose work needs somewhere it does not reach asks the user for a session rather than being given more
  reach.** Two alternatives were set aside. Letting a project session drive sessions in other projects directly breaks
  the assumption that an owner and its bound sessions share a project — removing a project would leave dangling
  bindings, and a project's bulk archive delete would reach sessions elsewhere — and hands a session the user opened by
  hand the console's whole reach without asking. Promoting the session itself into a console session cannot work
  either, because a session's role description cannot change on resume: Claude Code replays its appended system prompt
  verbatim and Grok Build stores its `--rules` in the session record, so the session would get a console session's
  tools while still being told it is a project session. What it does instead is bounded. `request_session`, offered to
  every session, asks the user to start a session with no owner — in any project, or a console session — and only the
  user's approval starts it. `open_session` starts a session the caller owns, with no approval, where the caller
  already has reach. A binding changes after launch by a call of a session concerned, or by the user: an owner hands a
  child session to another parent with `transfer_session` or releases it with `release_session`, an independent
  project coordinator sets a console session or a coordinator of its own project as its parent with `bind_to_session`
  (how a session that asked for a console session comes to be directed by it), and the user lets a session work
  independently from the sidebar. The session's role description is fixed at its first launch, so the change is told
  to it in prose instead — in the call's result or a message, or at its next launch if it is not running. The behaviour
  of a request is in `docs/product/session-requests.md`, the rules a binding keeps in "Who can own a session" in
  `docs/product/changing-session-owners.md`, and the deferred notice in "A change of owner told at the next launch"
  there.
- **A project session goes to the user directly for permissions and questions, not through the console session.**
  Relaying through the console session only creates confusion when several projects ask at once. "Needs a console
  session decision" (the agent calls `report(status: needs_decision)`) and "needs a user decision" (a permission
  prompt or a question to a person, shown as the raised hand) are kept apart.
- **Detecting the raised hand is limited by what the hooks expose**, not by Octoboard: a question asked as plain prose
  is indistinguishable from a finished turn on all three agents, and that is a known gap rather than something to work
  around with terminal-text matching. State is read from hooks, never from rendered output, because that would need a
  VT emulator in the daemon and differs between an agent's own renderers. The one exception is each agent's own
  folder-trust confirmation, which no hook precedes and so can only be recognized in the output.

## Known pitfalls of the Tauri / Rust approach

Each has an established solution unless noted; any outstanding manual verification is stated in its entry.

- **PATH and environment.** A macOS application launched from Finder does not inherit the shell's `PATH`, so `claude`
  and `codex` (under `~/.local/bin` or a node-version manager's shims) cannot be found and API keys are missing. A login
  shell is not enough: on zsh it is `~/.zshrc` that sets those up and a login-only non-interactive shell never reads it.
  The daemon therefore snapshots the environment from a **login + interactive** shell. A launch reuses the last snapshot
  while it is valid (under ten minutes old, every `PATH` directory it found still present, since a version manager's
  `PATH` entry can point at a per-shell-instance directory) and retakes one in the background for the next launch,
  because the login shell's serial chain of execs can take seconds; with none valid it waits for a fresh one. The
  snapshot taken once per daemon start to work out which agents are available (`src/agents/availability.rs`) is the
  first one a launch can reuse. The daemon spawns the agent directly rather than inside a shell.
- **The snapshot must be filtered, not only taken.** The shell passes the daemon's own environment through, so a daemon
  started from inside an agent session would hand that session's identity to every agent it launches. On Claude Code
  that silently turns transcript saving off and makes `--permission-mode` not apply. Stripping the markers does not
  disturb authentication, which lives in the macOS Keychain. The markers are enumerated, never matched by prefix, and
  stripped regardless of which agent is being launched. Build the list by dumping `env` inside a live session of each
  agent and stripping what is there and only there: `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and
  `CLAUDE_CODE_USE_BEDROCK` are user settings that a prefix match would drop.
- **Terminal data does not go through Tauri IPC.** High-frequency output would pay serialization overhead, and it would
  break the constraint above. The frontend connects to the daemon over WebSocket directly.
- **Sidecar signing.** The bundled daemon must be signed and notarized with the application or Gatekeeper blocks it.
  Credential access is not a concern: an agent launched from the bundle reaches the user's Keychain login normally,
  because a Keychain ACL is evaluated against the agent binary's own signature rather than its parent's.
- **The Rust toolchain is pinned, and moves with the stable release.** `rust-toolchain.toml` names the exact version
  every machine and CI builds with, and `rust-version` matches it. Building on one version while users install with a
  newer stable lets a lint that newer Rust denies by default break their build unseen. Raise the pin to the latest
  stable rather than holding a floor: an old floor forces exact pins on whole chains of transitive crates (a crate's
  declared `rust-version` is advisory and often wrong in both directions), and the Tauri crates and their
  `@tauri-apps/*` packages in the UI must move together, since the Tauri CLI refuses to build when the two differ in
  major or minor version.
- **Full-width punctuation from a CJK input method depends on the pinned `xterm.js`.** A mark such as `？`, which an
  input method emits without a candidate window, reaches WKWebView's `input` before the mark's own keydown. `xterm.js`
  drops such a commit while its "key down seen" flag is set, and up to 6.0.0 it armed that flag on every keydown, a
  bare `Shift` included, so the first press was swallowed. The pinned 6.1 beta leaves the flag alone on a
  modifier-only keydown, which is what makes the first press work; `TerminalUserInput.test.ts` pins that library
  behaviour, so an upgrade or downgrade that loses it fails there. The path itself was confirmed by hand with a real
  Chinese input method on the pinned beta, and each `xterm.js` upgrade needs the same check, since synthetic key events
  do not go through it: `？`, `！` and `（` on the first press, normal Pinyin composition, no duplicates when typing `，？`
  quickly, `Ctrl+C` still interrupting, and `？` twice with `Shift` released in between. By the library's source (not
  observed), a mark pressed while another non-modifier key is still held can still be dropped, since the flag stays
  armed then by design (relaxing it duplicates input in fast typing). Keep xterm's own composition handling rather
  than adding a composition handler of our own or synthesizing composition events.
- **Mouse reports past column 95.** The encoded byte exceeds 127, so `onData` never sees it; only `onBinary` does, as a
  string of raw code units, hence a mask back to bytes rather than a UTF-8 encode. Not yet confirmed by hand: the
  forwarding was read line by line but never exercised with a mouse-aware TUI, and none is reachable from Grok's bash
  mode, which has no controlling TTY.
- **A packaged application needs file-access permission per volume.** The bundled `.app` raises a macOS prompt the
  moment a project lives on an external volume, which `tauri dev` never does, so associating a project has to cope with
  the user declining or not having answered yet.
- **`Ctrl+C` does not reach the terminal on its own.** WKWebView swallows it above `xterm.js` while other modifier
  combinations pass, so the UI intercepts it and forwards `0x03` itself.
- **macOS Quit does not raise Tauri's `ExitRequested`.** Cmd+Q, the application menu's Quit and the Dock's Quit all send
  `terminate:`, and nothing in the Tauri/`tao` stack implements `applicationShouldTerminate:`, so a confirmation hung on
  `ExitRequested` is skipped. The application owns its Quit menu item, and the Dock's Quit can only be caught by
  overriding the application delegate. That selector is also how a logout, restart or shutdown arrives, with nothing to
  tell them apart, so holding a confirmation up in front of a logout is the unavoidable price of asking before the
  Dock's Quit.
- **WebView differences.** Tauri uses the system WebView (WKWebView on macOS, WebView2 on Windows). There is no Linux shell to pick one:
  Linux is the server form, reached from the desktop application or from whichever browser the user has, which the UI
  treats as a plain browser like any other.
- **Windows refuses to replace or delete a file another process holds open.** Unless the holder opened it with
  delete sharing, a rename over it or a removal fails with a sharing violation, where Unix would unlink the name and let
  the holder keep its file. A dying process can hold its files a moment after it is reported gone, so cleanup that runs
  at start (the data directory's `run/`) tolerates the violation and leaves the file for the next start, and code that
  replaces a file in place must expect the failure on Windows rather than treat it as a broken disk.
- **File links on Windows need Developer Mode or elevation.** A directory alias is a junction, which any user can make,
  but a file symlink needs the `SeCreateSymbolicLinkPrivilege` that only Developer Mode or an elevated process grants.
  The daemon probes once per run whether it can make one (`platform::fs::file_links_available`) and announces it as the
  host's `file_links`; configuration sharing, which links files, is reported unavailable where it is false, and a
  console's `CLAUDE.md` is written as the line `@AGENTS.md`, which Claude Code expands, instead of a link. A copy of the
  text was rejected because Grok Build reads both names and loads two separate files twice; a hard link both for that
  and because a replace-by-rename write breaks it silently. SQLite's Windows layer names a database's `-wal` and
  `-shm` from the path as given, so a database opened through a file link and through its target keeps two logs and
  loses writes; Windows therefore never shares a database through a link (`SQLITE_FOLLOWS_FILE_ALIASES`).
- **Agents keep their credentials in the Keychain on macOS and in a file on Windows.** Claude Code on Windows stores its
  OAuth tokens in `.credentials.json` under its configuration directory rather than in a system credential store, so
  on Windows an account's configuration directory carries its login, and what is shared or copied between accounts
  there is a credential.
- **A Windows launch's processes are held in a named job without kill-on-close.** Each launch's processes are
  members of a job object named for the launch, which is how the daemon lists and stops them (Windows has no process
  group to signal, and another process's environment cannot be read cheaply). Kill-on-close was rejected because a
  process cannot leave a job, so the kept and carried-over process trees would die whenever the daemon exits. A job's
  name disappears with its last handle, so the daemon duplicates the job's handle into each child it launches; a
  restarted daemon reopens the job by name while such a member lives, and otherwise falls back to the ledger's process
  id, creation time and parent chain.
- **Closing a pseudoconsole is what ends its processes, and the host's pipe never ends by itself.** The console's output
  pipe stays open after every process on it has exited, so the daemon closes the console itself once the process is gone
  and a reader waiting for the end would wait for ever. The close blocks (the system's host) or leaves the host and its
  processes alive (the bundled one) while output is unread, so it runs on a thread of its own with a drainer reading
  meanwhile, and a reader is never paused or joined first. A closed console sends every process attached to it
  `CTRL_CLOSE_EVENT`, which ends it however it is held (a process that handles the event is ended by the system after
  about five seconds, the attached processes one by one). So a terminal launch does not survive the daemon, and neither
  does a process an agent's tool starts on the agent's own console when its launch ends. Such a process is therefore
  listed as not keepable (`process_shares_agent_console`), and only what a user starts on a console of its own can be
  kept. A headless child gets a hidden console host of its own, which is a member of the launch's job, so a member list
  holds console hosts.
- **Input to a pseudoconsole never stalls, but it is slow, and some of it is changed on the way.** The host reads its
  input pipe continuously and queues what it reads without a bound, so there is no macOS-style stall after a fixed
  number of bytes; a writer is slowed instead (the system's host takes 0.24 to 0.75 MB/s, one 4 KiB write took up to 1.2
  s), so a write has a deadline per slice rather than for the whole. A NUL byte is dropped on the way to a program. The
  host hands a program that reads console input records, as Codex does, the paste's text as typed keys and an Enter
  written behind it as one more key of the same burst, so the daemon writes the Enter apart, once the paste is drawn.
  Win32 input mode sequences are never written: once one has been, the bundled host holds a lone Escape until the next
  key and joins the two. And Windows' per-process "ignore Ctrl-C" flag is inherited, so a daemon started from a session
  that set it (over ssh, say) would hand it to every terminal process, none of which would then raise `CTRL_C_EVENT` for
  a written `0x03`; the daemon clears it on itself before its first terminal process, which also makes Ctrl-C on a
  console of its own reach the daemon as a request to end.
- **A daemon inside a kill-on-close job loses its launches when that job closes.** The job a terminal or a remote
  session puts its programs in does, and nesting a launch's own jobs under it raises no error. A launch can leave it
  only where the outer job allows breakaway and otherwise fails to start with access denied, so launches never ask for
  it. The desktop app's sidecar is started with a hidden console of its own, because the daemon's group leaders share
  the daemon's console. A job's `Local\` name is per logon session, so a process of another session does not find it.
- **A Store PowerShell leaves the launch's job.** A PowerShell 7 installed from the Store is reached through an app
  execution alias (a reparse point of tag `IO_REPARSE_TAG_APPEXECLINK` in `WindowsApps`). While any process of its
  package runs for the user, creating it with the job list is refused as access denied, and what a packaged desktop app
  starts leaves every job it is in (the desktop app breakaway). The daemon therefore creates such a shell, whichever
  alias it is, suspended and without the job list, puts it in its jobs, and then lets it run, with the desktop app
  policy `PROCESS_CREATION_DESKTOP_APP_BREAKAWAY_DISABLE_PROCESS_TREE` so that what the shell runs stays in them: the
  shell is in its jobs before its first instruction, which keeps the busy reading and the end of the jobs whole. The
  price is that what it runs carries PowerShell's package identity, which changed nothing visible in Git, Node, npm,
  Claude Code, Codex or writes to `%APPDATA%`, `%LOCALAPPDATA%` and `HKCU\Software`, and where it did cause trouble the
  fallback is to skip Store aliases for the login shell. Other routes were set aside: skipping the alias loses the
  PowerShell the user installed, and starting the package's program around its activation is not what the alias starts.
  The boundary is what the daemon does not start: a Store `pwsh` that an agent's tool or hook starts (the agent chooses
  its PowerShell) is outside the launch's job with its descendants, and carries a package identity. Such a tree was
  listed under the launch through its parent chain, ending the session ended it, and the next daemon after a kill of the
  old one ended a surviving one. A tree whose chain to the agent breaks before any scan attributed it has nothing left
  that ties it to the launch (no tag is read on Windows, and it is in no job of the launch), so it is neither listed nor
  stopped; and the clean-up inside a panicking daemon reaches only job members.
- **Git for Windows runs a hook through a fresh MSYS `sh`, so a commit pays for every hook, and every hook has the same
  parent.** A commit runs 11 hooks (`pre-commit`, `prepare-commit-msg`, `commit-msg`, seven `reference-transaction`,
  `post-commit`); a bare `sh -c 'exit 0'` costs about 40 ms there, a commit through Octoboard's dispatcher measured
  about 0.7 s in the final run and 2.3 to 3.2 s in a loaded virtual machine, against 0.1 s with no hooks. A session
  still registers the full hook set, as on macOS: naming only the hooks a session needs saves about 0.2 s (git runs
  `reference-transaction` for 7 of the 11), and would silently skip the user's own hooks in other repositories an agent
  touches and any hook installed after the launch. Because git for Windows' `git.exe` is not an MSYS process, `$PPID` is
  1 in every hook of every repository, so the dispatcher cannot pair a transaction's `prepared` call with its
  `committed` one by process id; it keys them on a hash of the working directory, `GIT_DIR` and the transaction's own
  refs, and its completeness check turns away a pair two transactions wrote to. A hook that backgrounds the daemon's
  recorder must redirect all three of its streams, as it does, or git, or a caller capturing git's output, waits for it.
- **A Windows daemon is not told of a sign-out or a shutdown.** It handles Ctrl-C, Ctrl-Break and a closing console
  (closing its window ends its children and it exits with code 0), but a process that has loaded `user32.dll` or
  `gdi32.dll`, as the daemon has, is sent neither the logoff nor the shutdown notice (Microsoft's documentation of
  `SetConsoleCtrlHandler` says so, and Windows 11 behaved as documented in a real logoff and a real shutdown). The
  system ends the user's processes and the daemon's log shows no orderly stop. The desktop app's sidecar is the
  exception as far as time allows: before starting the daemon, the app sets its shutdown level to `0x300`, ahead of
  the daemon's default `0x280` (`SetProcessShutdownParameters`, higher levels first). It passes the confirmed system
  end of session on and asks for the daemon's ordinary stop, within four seconds. Equal default levels let Windows
  terminate the daemon's hidden console before that request reaches it. What is left is dealt with at the next start, as
  after a crash ("Crashes and forced termination" in `docs/product/application-lifecycle.md`), and the process ledger's
  start-up reconciliation stops the attributed processes its records name that are still running. A held update is not
  installed at such an exit, and the next start offers it again.
- **Reading another process on Windows is cheaper than it looks, and weaker.** A full scan of the same user's processes
  (a toolhelp snapshot, the token's user, times and memory, the listening ports from `GetExtendedTcpTable`) costs 10 to
  25 ms. A command line comes from `NtQueryInformationProcess(ProcessCommandLineInformation)`, which needs no read
  access to the process and works across bitness and for an elevated process of the same user; a working directory needs
  a read of the process parameters, which is denied for an elevated process to a daemon that is not elevated, so it is
  unknown there. A process's environment is not read, so nothing is attributed by its tag and the job and the parent
  chain do it; pids are reused and never reparented, so a parent must have started no later than its child.
- **MSYS programs are slow through a pseudoconsole.** A Git for Windows `cat` of a large echo took 28 s for 4 MiB, with
  one stall of 12.5 s, where a native echo through the same host took 0.6 s.
- **End-to-end terminal latency was never measured.** Only the daemon-to-WebSocket path was (well under a
  millisecond); the time from key press to glyph on screen was not.
- **PTY output and slow clients.** Under heavy output a reader that always runs ahead fills any bounded channel to a
  client within a fraction of a second, and neither framing nor batching addresses that. So the daemon's PTY reader
  never waits on a client and keeps no channel: each client has its own backlog, sent as its credit window (bytes it has
  acknowledged) allows, and a client that falls more than 4 MiB behind gets a fresh paint without history in place of
  its backlog instead of being dropped (`apps/daemon/PROTOCOL.md`, "`GET /ws/term/:session` — terminal stream"). The
  512 KiB starting window, the 4 MiB cap and the web client's acknowledgement every 64 KiB are starting values, not yet
  tuned on WebKit or on a browser over a slow link. The size of the daemon's own read buffer (4 KiB to 256 KiB) made no
  measurable difference to throughput, so output frame size is not worth tuning.

## Data model

Stored entities: console, host, project, session, account, report-panel page, trusted folders, information held for an
interrupted session, branch origin (the session responsible for a branch) and application-wide settings. The shapes
are in `apps/daemon/src/store/`; what is worth knowing is the reasoning behind a few fields.

The database carries a schema version (`PRAGMA user_version`, `SCHEMA_VERSION` in `store/schema.rs`), and a new database
gets the whole current schema at once, stamped with Octoboard's `PRAGMA application_id`. A file at the current version
is used as is, and a file with no tables is initialized in place. A file with tables and no application id (from a build
before the first release) is moved aside as `<name>.superseded-<unix seconds>` (a counter added when taken; its
write-ahead log checkpointed into it first and any sidecar files moved with it; never deleted, with a warning logged)
while a fresh database starts. An older file is copied aside as `<name>.pre-v<N>` and brought up to date by the steps in
`MIGRATIONS`, one per version, all in one transaction, so a failure leaves it as it was. A file of a newer version or
one below 1, one marked as another application's, or one that is not SQLite, is refused and left untouched. Version
changes and migration code are written only between released versions: a schema change no release has carried is made in
place, at the current version; a change to a released schema raises the version and adds the step from the released one,
whose schema is kept for the migration tests.

- What belongs to one provider of a console or a project (its account, account-default flag, launch model and effort)
  is a row of a child table keyed by the owner and the provider, not a column, so a provider is added without a
  schema change; rows of a provider this build does not register are kept and left out of loads.
- `Session.id` is Octoboard's own and `agent_session_id` the agent's. They are separate because some agents cannot
  pre-allocate an id.
- A session row is never written back whole from a copy read earlier. Its fields change on tasks of their own — hooks,
  renames, archives, the exit watcher, account switches — so each change writes only its own columns, and a change of
  status that must not undo another is a conditional write: a status the agent reports never moves a session out of
  `interrupted` or `archived`, and the process ending never moves it out of `archived`.
- `Session.bound_to` is the id of a session's parent — a console session or a project coordinator of the same project —
  or unset for an independent session. `open_session` sets it to the creating session; `transfer_session` changes the
  parent; `bind_to_session` sets a parent for an independent project coordinator; and `release_session` clears it and
  makes the session a coordinator again, leaving an independent coordinator. A coordinator that sets a console session
  as its parent keeps its children and its ability to manage them. `Session.role` (`coordinator` or `worker`, stored and
  sent to clients and MCP records, separately from the parent id) preserves that capability even when it has no
  children; a project session created under a console session starts as a coordinator, and one created under a project
  coordinator as a worker. Capability checks use `Session::is_worker()`, which reads the stored role, so every write
  that sets or clears `bound_to` writes the role with it: a console session and a session with no parent are always
  coordinators. A project session reporting to another project session is always a worker, so orchestration has at most
  two levels of reporting relationships. Reports route directly to the parent id because a console may hold several
  console sessions at once.
- `Session.colour` and `Session.ordinal` are set only for a console session: a badge colour from a fixed palette,
  assigned on creation and never reused while still in use among the console's other console sessions, and a
  per-console ordinal that is one past the highest ever handed out there — kept on the console record itself
  (`consoles.next_console_session_ordinal`) so deleting the console session that held the highest ordinal does not
  let a later one reuse it.
- `Project.path` is stored absolute and lexically normalized (a relative path is refused), as the user gave it.
  Whether two paths name the same directory, or a trusted folder covers a project, is decided from where they really
  are on disk (device and inode, links followed), not from the stored text.
- `Account` is a named config directory of one agent, kept once for the whole application. A console refers to one
  account per agent by id (`accounts`, an entry for each agent), rather than holding a path directly; `None`
  means that agent's default account, the state of pinning nothing. A project carries the same entries with one
  state more — inherit its console's reference, the agent's default account, or a named account — since pinning
  nothing is a choice of its own that a project may make against a console that names an account. It travels as a
  tagged value, and in storage the id column still only ever holds an account id, the default state living in a
  separate per-agent flag, so nothing stored can be mistaken for an id. `Session.account_id` is the account the
  session's own agent reads, and `Session.config_dir` is that account's directory at the moment the session was
  opened (`None` for the default account) — both copied onto the session when it is opened, because the agent keeps
  its transcripts there and a resume must find them.
- A console's and a project's **per-agent model and reasoning effort** are the opposite: read from the console or the
  project at every launch rather than copied onto the session, so a changed value reaches the session at its next
  resume. Nothing but the agent interprets them, which is why they are stored as free text per agent rather than as an
  enumeration, and why the models a dialog offers are asked of the agent itself for the account in question, kept only
  as a short-lived cache in the daemon: a list maintained in Octoboard would be a second source of truth that goes
  stale with every agent release.
- Every project and session carries a `host_id` while the host table holds a single local record, so going remote needs
  no data migration. Trusted-folder entries ignore `host_id`, which is harmless while every host is local.
- `Page.session_id` is the coordinator that pushed the page — a console session or a project coordinator —
  not its console or project: each session has its own page history. Deleting a session deletes its pages in the
  database, and deleting its project or console therefore deletes them too.
