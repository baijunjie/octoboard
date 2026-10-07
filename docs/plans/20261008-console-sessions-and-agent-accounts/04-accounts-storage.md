# Accounts: storage and protocol

> Goal: an account exists as a stored entity with a name, an agent and a config directory; a console refers to one
> account per agent instead of holding a path; a session records the account it holds beside the directory it
> already records.
> Completion criteria: the daemon's type check and test suite pass. Creating, renaming, repointing and removing an
> account over the protocol works, a client is told about the change, a name that collides within its agent is
> refused, and removing an account a console refers to leaves that console on the agent's default account. Editing a
> console's config directory in the dialog still takes effect, since the dialog is not replaced until milestone 7 —
> including, now intentionally, a path that does not exist, which the dialog refuses today. The product doc
> statements this milestone falsifies are updated with it.

## Technical design

- [ ] An account record: an id, the agent it belongs to, a name, and the config directory, stored in the same
      expanded absolute form a console's path is stored in today.
- [ ] **The default account of an agent is the state of pinning nothing**, not a row. It cannot be created or
      removed, it holds no directory, and its name is Octoboard's own word for it — translated like any other
      string, and **not editable**, so there is nothing about it to store. What it is shown as is derived; that is
      milestone 5's business.
- [ ] A console refers to one account per agent, by id. No reference means that agent's default account.
- [ ] A session records the account it holds, by id, beside the config directory it already records. A session on a
      default account records no directory, which is the shape such a session already has today. Both are written
      when the session is opened, and the directory stays the value the launch uses.
- [ ] **A name is required and unique within its agent**, compared trimmed and ignoring letter case, and the
      agent's default account name takes part in that comparison. A request that would collide is refused with a
      reason naming the account it collides with.
- [ ] The account list travels to clients as part of the settings record, which is already a record that may grow,
      so no new event is needed.
- [ ] Requests to create an account, to change one (name and directory, independently), and to remove one.
- [ ] Removing an account referred to by a console clears that console's reference, which puts it back on the
      agent's default account. Sessions holding it are left alone.

## Implementation plan

- [ ] Add the account table and the new console and session columns, following the project's existing way of adding
      a column to the schema.
- [ ] Normalize an account's directory by the rules a console's path goes through today, minus the existence check:
      trim, expand a leading `~`, require an absolute path, normalize lexically. A path that is not absolute is
      reported the same way a bad path is today, with nothing saved; a directory that does not exist is accepted.
- [ ] Resolve what a session is launched with from the account it holds — the account's directory, or nothing pinned
      for a default account — and keep each agent's existing rule for how that reaches the process. For Claude Code
      and Codex, pinning nothing means Octoboard sets no variable, so the snapshot's own value stands unchanged;
      Grok Build is always given a `GROK_HOME`, and what an account pins for it is the source home that home is
      built from.
- [ ] Narrow the existing launch refusal for a vanished config directory to a session that has a conversation on the
      agent's side. A new session and a session that never had a conversation launch into a directory that is not
      there, which is what lets the agent create it.
- [ ] Refuse at launch a **Grok Build** session whose pinned source home is not an initialized Grok home, on that
      same refusal path, naming the directory and what is wrong with it. The reason is in the overview's decisions.
- [ ] Keep the console dialog working unchanged for this milestone: the path it shows and saves for an agent is the
      referenced account's directory. Saving a path there repoints that account when **this console is the only one
      referring to it**, and otherwise creates a new account and points this console at that — editing one console
      must not silently change another's, which it never could before the accounts were shared. Where the console
      was on the agent's default account, saving a path creates an account for it. The dialog is replaced by a picker
      in milestone 7; until then it must not become a field that saves a value no launch consults. An account the
      dialog mints is named after the directory's last path component, falling back to the whole directory when that
      name is already taken for the agent, since the dialog has no name field to ask with.
- [ ] Mirror the new protocol shapes into the UI's copy of the protocol and into the protocol document.
- [ ] Bring the product docs along:
      - `docs/product/consoles-and-projects.md` — "Agent config directories" describes a console holding a path per
        agent, requires that path to exist, tables what a blank versus a set value means, and says Octoboard does not
        check a Grok home, all of which this milestone changes; "Editing a console" is stated in those terms;
      - `docs/product/launching-agents.md` — "The launch environment" is phrased in terms of a session that holds a
        config directory of its agent, which is now an account, and the "Grok Build" paragraph ends by saying
        Octoboard does not check that the directory is a Grok home, which this milestone makes false;
      - `docs/product/sessions.md` — the refusal of a vanished config directory is stated unconditionally both in
        "Opening a session" and in "Archiving, interruption and resuming", and this milestone narrows it to a session
        that has a conversation.

## Notes for the developer

**Reusable capabilities**

- The settings record and its update request already carry one optional field per setting and broadcast a change
  only when something actually changed; the account list and its requests belong with them.
- The config directory normalization and the launch-time refusal of a vanished directory both exist for the console's
  fields; reuse them rather than writing new rules.
- The store has one method per operation behind a single connection, and the schema is created in one batch with
  each later column added individually; follow that pattern rather than introducing a migration framework.

**Development notes**

- The daemon's protocol types and the UI's copy of them are kept in step by hand. Changing one without the other
  leaves no compile error.
- The agent brands are a closed enum with an adapter per brand. An account belongs to one brand and nothing about
  this milestone should widen that enum or the adapter trait.
- Pinning an agent's own default directory is not the same as pinning nothing — see the default-account decisions in
  the overview. Treating the default account's absence of a pin as if it meant a stored `~/.claude` is the one way
  this milestone can break every existing user.
- A session's config directory is deliberately excluded from the session update path today. The account a session
  holds is written at creation only in this milestone; the write path that changes it belongs to milestone 8.

**Reference docs**

- `docs/product/consoles-and-projects.md` — "Agent config directories", the behaviour being replaced, including why
  a pinned default is not a blank field.
- `docs/product/launching-agents.md` — "The launch environment", how each agent is pointed at its directory, and
  "Per-agent specifics a user will notice" for Grok's requirement on its home.
- `apps/daemon/PROTOCOL.md` — the protocol document to extend.
- `docs/memory/writing-daemon-code.md` — the daemon's conventions.
