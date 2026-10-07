# Agent availability and the default account

> Goal: Octoboard works out by itself which agents are available and what each one's default account resolves to, so
> a user who never opens Settings still has something to pick; and a user with no agent installed is told to install
> one instead of being let through to a launch failure.
> Completion criteria: on a machine with an agent installed, a client that connects receives that agent as available
> and receives the directory its default account resolves to — the directory the agent's variable is exported to in
> the user's login shell, else the agent's own default. (What a screen does with either is milestones 3 and 4.)
> On a machine with no agent binary on the login shell's `PATH`, no session can be opened — by
> the user or by a hub — and every place that would open one says what to install instead. Before the first
> determination of a run has landed, nothing is refused for unavailability and nothing claims an agent is missing.
> Starting the daemon creates and removes no account.

## Technical design

- [ ] **Availability.** An agent is available when its binary resolves on the user's login shell `PATH`, and that is
      the only test; the reasoning is in the overview's decisions. Availability has three states, not two:
      available, unavailable, and **not yet determined** — the state every run begins in.
- [ ] **This milestone creates no accounts.** The default account exists for every agent by construction
      (milestone 1) and needs nothing discovered to exist; the accounts the user has are already stored. Octoboard
      therefore never mints an account of its own at startup, and in particular never offers the agent's own default
      directory as a second, pinned account — the reason is in the overview's decisions.
- [ ] **What the default account shows.** It pins nothing, so what it is shown as is derived, not stored: the
      directory the agent's variable is exported to in the user's login shell, else the agent's own default
      directory. Its name is Octoboard's own and is not editable (milestone 1); only the directory is derived here.
- [ ] For **Grok Build**, what is derived is the *source home* the per-session home would be built from — the
      exported `GROK_HOME`, else `~/.grok` — not the per-session home itself, which is Octoboard's and exists only
      for the duration of a process.
- [ ] **When no agent is available at all**, opening a session is refused: the session dialog cannot be submitted,
      the Hub row cannot start a hub, and the hub's own tool for starting a session is refused with a reason it can
      report. The refusal names what to install. "No agent available" means availability has been determined and
      found none, never that it has not been determined yet.
- [ ] **When some agents are available and others are not**, the unavailable ones appear in today's agent pickers —
      the session dialog's and the console dialog's — named, not selectable, and labelled as not installed. They are
      shown rather than hidden so that the reason a user cannot pick them is on screen. Milestone 4 replaces those
      pickers with one grouped list and carries the same rule into it.
- [ ] The prompt to install is a message about the machine, not about one project: it says which agents Octoboard
      supports and that one of them has to be on the user's `PATH`. It does not tell the user how to install an
      agent or where to get it.

## Implementation plan

- [ ] Determine availability and the default accounts' directories once per daemon start, **without holding up the
      start**: clients are served immediately, and the result is broadcast when it lands. A login-shell snapshot is
      budgeted at ten seconds, and blocking the daemon's start on it would put that on every application launch for
      a result that almost never changes.
- [ ] Resolve each agent's binary and read each agent's variable from one login-shell environment snapshot, the
      same kind of snapshot a launch takes. Treat a snapshot that does not complete as "not determined this run"
      rather than as a failure of the start: nothing stored changes, and availability stays not yet determined.
- [ ] Carry both as derived state of its own — held on the daemon's in-memory state, replayed in the state snapshot
      a client receives on connecting, and broadcast by its own event when it changes — rather than as a field of
      the stored settings record, which is written only by the user's own updates. The not-yet-determined state has
      to be representable on the wire, not implied by an absent field.
- [ ] Refuse to open a session when its resolved agent is known to be unavailable, on the same path the existing
      launch refusals run on, so the refusal reaches the user the way a failed launch does. While availability is
      not yet determined, refuse nothing: the launch's own refusal already covers a binary that is not there.
- [ ] Show the install prompt where a session would be opened, and in the sidebar's empty state when no agent is
      available at all.
- [ ] Add every string to the message catalogues of all offered languages.
- [ ] Bring the product docs along:
      - `docs/product/launching-agents.md` — "The launch environment" states what each agent's variable is set to and
        when, which is now expressed as the account a session holds, and says nothing today about an agent that is
        not installed at all;
      - `docs/product/sessions.md` — "Opening a session" lists the dialog's fields and the ways a launch can fail,
        neither of which covers an agent that is not installed;
      - `docs/product/consoles-and-projects.md` — the console dialog's agent rows, which can now hold an agent the
        machine does not have;
      - `docs/product/sidebar.md` — the sidebar's empty state, which is where the install prompt appears;
      - `docs/product/hub-orchestration.md` — a hub's tool for starting a session gains a refusal it can report.

## Notes for the developer

**Reusable capabilities**

- The login-shell environment snapshot and the binary resolution against its `PATH` both exist for launching a
  session, including the timeout and the three distinct ways a snapshot can fail; reuse them rather than shelling
  out separately.
- The per-agent default directory and the per-agent variable name are already known to both the daemon's adapters
  and the UI's agent table.
- The daemon already has a place for derived, per-run state that is replayed in the state snapshot and broadcast
  on change; availability and the default accounts' directories belong there rather than in the settings row.

**Development notes**

- Octoboard installs nothing and writes nothing into the user's agent configuration. This milestone reads; it must
  not create a directory it did not find, and must not run an agent to find out anything.
- A hub can start sessions through its own tools, so the refusal has to exist in the daemon and not only in the
  dialog.
- The snapshot is taken from the user's login + interactive shell, which is deliberately not the environment the
  daemon itself runs in; an agent installed under a version manager is found only through that snapshot.
- What the grouped picker shows while availability is not yet determined is milestone 4's to settle; this milestone
  only guarantees that nothing is refused and nothing is called missing in that window.

**Reference docs**

- `docs/product/launching-agents.md` — "The launch environment", the snapshot and its failure modes, and what each
  agent's variable is set to.
- `docs/product/consoles-and-projects.md` — "Agent config directories", the per-agent variables and defaults.
- `docs/product/hub-orchestration.md` — how a refusal reaches a hub.
- `docs/memory/writing-daemon-code.md` — where derived state lives and how it reaches a client.
