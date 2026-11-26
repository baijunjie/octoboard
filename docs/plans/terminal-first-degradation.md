# Terminal-first degradation

**Status:** implemented on the `terminal-first` branch (milestones 01 to 10).

A temporary development plan: design only, split into milestones. It is one file by request, so the overview comes
first and each milestone is a section after it. Code placement is named down to the module, because the provider-neutral
core rule makes placement part of the design here.

## Overview

### Problem

P10 "Terminal first" (`docs/conventions/principles.md`) says a session always starts and runs as its agent's CLI would
in a plain terminal, each Octoboard enhancement degrades on its own, and the user is told by a non-blocking notice in
the terminal area. The audit taken before beta.9 found 23 places that break it, and the OpenCode work that followed
added more. They fall into six shapes:

- **A launch is refused though the CLI itself could start.** `AgentProvider::plan` returns a `Result`, and any error
  refuses the whole launch, as do `native_binary` and `terminal_binary`. Most of the errors are about what Octoboard
  adds (a launch directory, a settings file it cannot carry, a per-session home, a migration the CLI would run itself).
- **A CLI that exits as it starts ends the session.** An option one release of a CLI no longer takes makes it exit at
  once, and the session is recorded as interrupted. Nothing tries the CLI without Octoboard's options.
- **A session runs but waits on hooks that never come.** Messages are held for the life of the session, the status
  never moves, and a conversation id is never learned.
- **A resume is refused on a record that was only not found.** A record a CLI release renamed reads as proven gone:
  the resume is refused and an archived session may be deleted for it.
- **A launch is refused on a reading of Octoboard's own.** A login-shell snapshot that fails, an account its quota
  probe found signed out, and (silently) git hooks an old `git` cannot take.
- **Nothing on screen says a session is degraded.** A `session_notice` is a toast at the window's edge for a few
  seconds; nothing keeps it, so a client that connects later, or a user who looked away, never learns that the session
  in front of them runs without Octoboard's additions.

### Plan outline

1. **Standing notices and the terminal banner** (generic). A degradation notice is kept for as long as its condition
   holds and shown as a dismissible banner in that session's terminal area, to every client, whenever it connects.
2. **Launch tiers** (generic). A launch is planned at one of three tiers: enhanced, reduced, bare. A provider's `plan`
   can no longer refuse; when it fails the launch falls to the provider's infallible bare plan and a notice says why.
3. **Early-exit relaunch** (generic). A CLI that exits before showing any sign of starting is started again one tier
   down, at most once per tier.
4. **Hook absence** (generic). A launch whose hooks are not coming is read as a launch of a provider with no hooks:
   status from the program's own reports, or from its title where it reports nothing, held messages released on the
   input box, the user told.
5. **Resume without proof of absence** (generic). Absence is proof only where the reader was seen to recognise records
   the installed CLI version writes; anything less resumes with the CLI's own resume option and a notice.
6. **Launch-time fallbacks** (generic). Shell environment, account sign-in reading, session git hooks.
7. **Per-agent degradations.** Each provider's findings mapped onto 2 to 4, plus what only that provider can fix.

### Key design decisions

- **D1. Three tiers, walked downwards only.** *Enhanced* is the provider's `plan`. *Reduced* is an optional plan a
  provider gives after an early exit, leaving out what it knows a release may not take. *Bare* is the CLI as a person
  would start it for this session: binary, account, conversation to resume, model, task, working directory. One launch
  request starts at most one process per tier, so nothing loops, and bare is the floor: when it exits too the session
  ends as it does today, with the CLI's own message on screen, as in a plain terminal.
- **D2. `plan` cannot refuse.** Any error from it means "the enhanced launch is unavailable" and falls to bare. What
  stops every launch of the session is asked first and separately: by generic code (the directory is gone, the CLI is
  not on `PATH`, a pinned account directory is gone on a resume or cannot be named to the CLI) and by the provider's
  `launch_refusal` (what its CLI cannot do for this session whatever Octoboard adds: an account directory for an agent
  with the default account only). Why: under P10 the default must be to start, so the refusals are the short explicit
  list a reviewer can hold against "a plain terminal would fail the same way". Decided with it: Antigravity CLI with a
  file in both of its layouts degrades like the rest. The bare launch lets the CLI's own migration replace one file
  with the other, as a plain terminal run would, and the user is told afterwards.
  Rejected: a typed plan error (`Refused` or `Unavailable`). A refusal raised deep in planning is passed over when
  planning fails earlier for another reason, so refusals have to be asked first anyway, and every `?` would have to
  choose a side.
  Rejected: generic code telling refusals from degradations by code. It would keep a list of provider codes, which
  the provider-neutral core forbids.
- **D3. The bare plan is the provider's, required and infallible.** Its spelling of a resume, a model and a task is a
  fact of the CLI, so it is a trait method with no default: a default could only start the CLI without the conversation
  a resume names, which would silently open a new one. It returns a value, not a `Result`: it reads and writes
  nothing and names only arguments every measured version takes. The account is not its business: generic code
  exports the pinned directory through the layout. It carries no approval mode: the CLI's own configuration decides
  what the session may do, and the notice says so (decided: an option-free floor is worth more than carrying the mode).
- **D4. A bare launch is read as a launch of a provider with no integration.** Generic code answers the per-launch
  questions itself instead of asking the provider (status from the program's reports, no cancel that drops messages,
  no native stash, no tool-listing wait, no extension channel, no private server, no start hook to wait for). What
  generic code sets through the environment stays, since a variable cannot make a CLI exit: the git hook dispatchers,
  the process tag, the session ids. A bare session that has an owner has no `report` tool; only the user is told
  (decided), through the banner.
- **D5. A launcher that cannot be replaced is a degradation, not a refusal.** When `native_binary` or
  `terminal_binary` fails, the ordinary launcher is used and the launch is bare, since the enhanced arguments are what
  that launcher cannot carry. Free text is not put on a script shim's command line: there the task goes in as the
  session's first message.
- **D6. An early exit is told by what did not happen.** A process counts as having exited at start only when it ended
  by itself inside a window of 10 seconds, before the terminal or the daemon saw any sign of an interactive program,
  and with no key typed into it by a client. A person who quits has to type, and a CLI that got as far as drawing took
  its options. The window only bounds the watch; the evidence carries the decision. A lower tier that stayed up is
  remembered for that binary and version for 10 minutes. Rejected: reading each CLI's "unknown option" wording in
  generic code, which is per-agent text and changes with releases.
- **D7. The reduced tier is optional and the provider's.** Generic code offers the early exit (the output the CLI
  wrote, bounded, as plain text) and takes a plan or nothing. A provider with a measured refusal line can leave out
  exactly what was refused; one without leaves out everything it marked as version-sensitive, or gives nothing and the
  launch goes to bare.
- **D8. "Hooks absent" is a state of the launch that can be entered and left.** A bare launch starts in it. An
  enhanced launch enters it when a hook was due and did not come within 30 seconds (`HOOK_FALLBACK`, reused), and
  leaves it when a hook arrives. While in it every generic reader of a fact that only holds with hooks takes the
  no-hooks answer.
- **D9. Without hooks and without status reports, the title gives working and idle, and nothing more.** A fresh
  running title opens a turn; a title back at the prompt closes it only once it has settled, no dialog or prompt of
  the agent's is recognised on the screen, and the input box is drawn. A turn closed this way synthesises no report
  to an owner and does not mark the session paused, so a title can never produce a report by mistake. The residual
  risk is stated in milestone 06.
- **D10. Absence is proof only with a witness.** A record not found proves the conversation gone only where the
  provider's reader has, in this daemon's life, recognised a record written by the CLI version now installed. A
  release that renames records leaves no witness, so nothing is refused or deleted. With a witness the resume is
  still refused (`conversation_unresumable`), as decided. Rejected: "the records directory still holds some
  recognised record": records in the old format stay behind after a release starts writing a new one.
- **D11. A degradation notice is a standing `session_notice`.** The event gains an optional `standing` flag; the
  daemon keeps a launch's standing notices and serves them on the `Session` record, so every client has them whenever
  it connects, and withdraws one when its condition ends. A client shows them as a banner in the session's terminal
  area and no toast. No second event family: an older client of the same generation still gets the event and shows
  its toast. One generic code nests the provider's own coded error as its reason (as `launch_failed` does), so
  adapters keep raising the errors they raise today and the notice names the file or setting through that error's
  params.
- **D12. No version gate is added; a floor needs proof of when the capability came (P10).** An option or feature is
  passed regardless of version, and a CLI that refuses it is caught by the early exit (D6). A floor (a capability
  switched on from version X) is kept or added only where it is known that X introduced it: the release that added it
  says so, or a run of the release before X shows it missing. The oldest version that happened to be installed and
  tested is no such proof. Existing floors without that proof are removed. An unreadable version, or one newer than
  any measured, counts as having the capability. OpenCode's generation dispatch stays as such a floor: 2.0.0 switches
  the second generation on, an unreadable version takes the newest, and a wrong pick is caught by D6.
- **D13. A reading of Octoboard's own never refuses the user's own request.** The login-shell snapshot falls back to
  the last capture however old, then to the daemon's own environment. An account read as signed out no longer refuses
  a session the user opens from a window; a switch, a handoff's receiving session, a session an agent or a scheduled
  task opens, and every automatic choice keep refusing or skipping it, since nobody is at that terminal to sign in.

### Notices

Every notice below is a standing `session_notice` (D11): kept while its launch runs and its condition holds, shown in
the terminal banner, gone with the process. Names are proposals to settle in milestone 01; a code is frozen once
released.

| Code | Raised when | Params | Withdrawn before the process ends |
|---|---|---|---|
| `launch_enhancements_unavailable` | The enhanced launch could not be prepared and the session started bare | `agent`, and the failure nested as `reason`, `reason_code` and its own params | never |
| `launch_retried_plain` | The CLI exited as it started and the bare relaunch stayed up | `agent`, `detail` | never |
| `hooks_not_received` | A hook was due and none came within the bound | `agent` | when a hook arrives |
| `conversation_id_unknown` | The launch has no conversation id and no hook will report one | `agent` | when the id is learned |
| `conversation_record_not_found` | A resume went ahead though the record was not found | `agent` | never |
| `account_sign_in_unconfirmed` | A session was opened on an account last read as signed out | `agent`, `agent_id`, `account`, `config_dir?` | when a reading finds the account signed in |
| `shell_environment_unavailable` | The launch ran on a fallback environment | `detail`, `cause` | never |
| `session_git_hooks_unavailable` | A project session could not be given its git hook dispatchers | `cause`, `version?` | never |

A provider's own notice (a single enhancement left out of an otherwise enhanced or reduced launch) is a standing
`ProviderCode::notice` raised through `LaunchPlan::notice` or `further_launch_notices`. The ones providers raise today
for a degradation that lasts the launch become standing in milestone 02: `provider.opencode.version_unmeasured`, Qwen
Code's hooks-off notices and Antigravity CLI's settings-not-read notice.

### Milestones

| No. | Milestone | Depends on | May run beside |
|---|---|---|---|
| 01 | Notice codes and wording | — | — |
| 02 | Standing notices and the terminal banner | 01 | — |
| 03 | Launch tiers and the bare plan | 01, 02 | 08 |
| 04 | Early-exit relaunch | 03 | 07 |
| 05 | Hook absence | 03, after 04 | 09 |
| 06 | Status from the title when nothing reports | 05 | the parts of 10 |
| 07 | Resume without proof of absence | 03 | 04 |
| 08 | Opening on an account read as signed out | 01, 02 | 03 |
| 09 | Shell environment and git hook fallbacks | 03, after 04 | 05 |
| 10 | Per-agent degradations (eight parts) | 03, 04, 05 | 06; its parts beside each other |

Waves: 01; 02; then 03 with 08; then 04 with 07; then 05 with 09; then 06 with the eight parts of 10. The work
starts from the commit that holds the first round of beta.9 fixes. Files shared across a wave are listed in each
milestone's notes. 01 and 02 both edit the UI catalogs and the generated bindings, so they run in order; 04, 05 and 09
each touch `session/term` or `coordinator/launch.rs`, which is why 05 and 09 follow 04; no adapter directory is touched
by two milestones at once, which is why 10 comes last.

### Findings to milestones

| Finding | Becomes | Milestone |
|---|---|---|
| 1 Antigravity directory flag | bare fallback; the probe stays a probe | 03, 10 |
| 2 Antigravity earlier layout | bare fallback, a file in both layouts included | 03, 10 |
| 3 Antigravity user files | bare fallback, the notice naming the file | 03, 10 |
| 4 Antigravity `--settings` keys | reduced plan from the measured refusal line, else bare | 04, 10 |
| 5 Codex hook-trust flag | floor if a version is established, else reduced plan without hooks | 04, 10 |
| 6 Codex native program | launcher fallback, bare | 03, 10 |
| 7 Grok `--no-leader` | floor if a version is established, else reduced plan | 04, 10 |
| 8 Grok sandbox | bare fallback against the real home | 03, 10 |
| 9 Grok home not initialized | bare fallback against the pinned home; the switch's refusal stays | 03, 10 |
| 10 Copilot options | early exit to bare | 04, 10 |
| 11 Pi options and extension | early exit to bare; the extension made total | 04, 10 |
| 12 Claude options | no `--effort` floor (passed whatever the version); early exit to bare | 04, 10 |
| 13 Qwen system settings | bare fallback without the stand-in settings file | 03, 10 |
| 14 Qwen `node.exe` | launcher fallback, bare | 03, 10 |
| 15 shell snapshot | fallback environment and notice | 09 |
| 16 resume refused on a missing record | witness rule; resume anyway | 07 |
| 17 account read as signed out | notice, not a refusal, for the user's own open | 08 |
| 18, 19 messages held for life | input-box stand-in after the bound | 05, 10 |
| 20 status never moves | status from reports once hooks are absent, from the title where nothing reports | 05, 06, 10 |
| 21 conversation id never learned | notice (generic: any launch with no id and no hook to report one) | 05, 10 |
| 22 old `git` drops session hooks silently | notice | 09 |
| 23 Antigravity probe latency | no change: bounded and kept | — |
| "Non-blocking notice in the terminal area" | standing notices and the banner | 02 |
| OpenCode migration, generation, resume check | bare fallback; floor kept; only OpenCode's own answer refuses | 03, 10 |
| Pi record kept in an earlier records directory | the resume names that directory | 10 |

### Test strategy

- **Planning is tested without a process.** The tier decision is a function over a provider and a `LaunchSpec`, so the
  generic tests run it for every provider (`AgentId::ALL`, fixtures taken from the provider) and for the test-only
  provider's switches (a failing plan, a refusal, a reduced plan). A bare plan is proven infallible by building it
  over a hostile fixture: no scratch directory, unreadable user files, no version.
- **Each provider's exact bare and reduced arguments are its own tests**, in its adapter's files, never a row of a
  shared table.
- **The relaunch is tested at the coordinator with stand-in launches**, which already stand in for `term::launch`. The
  stand-in is chosen by the tier the request carries: one that prints a refusal and exits for the upper tier, an idle
  one for the lower. Cases: relaunched once and told; both tiers exit (two launches, then interrupted, no notice); a
  key typed before the exit (no relaunch); a title, a hook or a status report before the exit (no relaunch); ended on
  purpose inside the window (no relaunch); another operation holding the claim (no relaunch); an account switch
  following its launch through the relaunch.
- **Windows of time follow `docs/conventions/tests.md`, "Daemon tests".** The early-exit window, the hook bound and
  the title's settle are read through `#[cfg(test)]` getters that return `PATIENCE` unless a thread-local override is
  set; "the window ran out" ends on a `#[cfg(test)]` event, not a widened wait; "no relaunch happened" is read from
  what the code decided (the recorded launches, the session's status after `exit_settled`), never from a sleep.
- **Stand-in CLIs.** A CLI that rejects an option is `test_support::fake_agent` (the shared dispatcher, never a fresh
  executable): a body that prints a usage error and exits for any option outside its list and idles otherwise. On
  Windows the stand-in is native (`stand_in_running_steps`), not a Git Bash script.
- **What a real CLI prints is a capture.** A provider's reduced plan that reads a refusal line is tested against the
  raw bytes a real run wrote, committed under `apps/daemon/testdata/` with the CLI version, beside a near-miss capture
  (a fast exit that is no option refusal). A floor is committed with the run that established it. The title's reading
  of a turn is replayed from real title captures: a turn with tool calls, a gap between two steps, a cancel, a wait on
  the user.
- **Hook absence runs on a paused clock** where nothing waits on real I/O, with the bound taken from the production
  constant and the provider's behaviour taken from the test-only provider (`test_support::test_provider`).
- **Standing notices.** Daemon: raised, served on the session record to a connection that subscribes afterwards,
  withdrawn, gone with the process, restored by an adopting daemon. UI: jsdom tests of the banner (shown from the
  record, no toast for a standing event, dismissed per window, back for a notice raised again, the unknown-code
  fallback, the announcement made once), gallery scenarios, and the UI preview.
- **UI elsewhere.** `daemonCodes.test.ts` holds every code to a record and words; the provider modules' tests hold the
  words in every language; the Resume tab and the account pickers get jsdom tests of the new allowed-with-warning
  states.

### Documentation to update

Product docs (`docs/product/`), by what changes:

- The banner and what it replaces: `session-terminal.md`, `toasts.md`, `error-alerts.md` ("Notices to acknowledge"),
  `moving-focus-between-regions.md` (its stop in the F6 cycle), `values-a-newer-daemon-sends.md` (a standing notice
  with a code the window does not know).
- Launch tiers, what a bare launch carries and loses, the early-exit relaunch: `launching-agents.md`,
  `session-terminal-interrupted-and-archived-sessions.md`, `hub-orchestration.md`, `reporting-to-owners.md` (a bare
  session has no Octoboard tools), `per-agent-settings.md` (an approval mode is not applied to a bare launch).
- Each provider's former refusals and new notices: `per-agent-launch-specifics.md`, `supported-agents.md` where it
  states a floor.
- Status and holds without hooks: `session-statuses.md`, `program-status-reports.md`, `turns-hooks-do-not-report.md`,
  `message-delivery.md` (the holds stated as lasting the session's life), `folder-trust.md`.
- Resume: `archiving-and-resuming-sessions.md` ("When a conversation can no longer be resumed"), `deleting-sessions.md`
  ("Deleting archived sessions whose conversation is gone"), `resuming-a-conversation-by-its-session-id.md` (the
  "Nowhere" row), `requests-to-resume-a-conversation.md`, `what-every-session-reads.md` (`resumable`).
- Accounts: `opening-sessions.md` ("When the agent or the account cannot open a session"), `logging-accounts-in.md`,
  `account-quotas.md`.
- Git hooks: `branch-responsible-session.md`, `revert-gate.md`, `keeping-sessions-to-their-own-project.md`.

Other long-lived docs: `apps/daemon/PROTOCOL.md` (the `standing` flag and the session record's notices, the
`session_notice` codes, the refusals that no longer apply to a launch, the terminal's meta);
`docs/conventions/ui-standard.md` and `docs/conventions/ui-accessibility.md` (where a standing notice is shown, and
that it is neither a toast nor an acknowledged alert); `docs/conventions/error-codes.md` (the banner as a surface that
draws a code's remedy); `docs/structure/ui.md` and `docs/structure/daemon.md` (the new modules and entry points, "Where
new code goes" for a provider's refusal against its degradation); `docs/conventions/provider-neutral-core.md` (plan
cannot refuse; the bare plan is required); `docs/agent-cli-reference.md` (every measurement milestone 10 takes); and
`docs/memory/` for what the measurements turn up.

### Open

To confirm at the UI preview; neither changes the milestone split.

1. **The banner's place.** Whether it floats over the terminal's top rows, as the pane's existing bars do, or takes
   its own height above the terminal, and how several notices fold into one bar. Milestone 02 is written on the first:
   one bar in the top slot, the others behind a control.
2. **The Resume conversation tab when the ID is found nowhere.** The layout of the agent and account pickers and the
   warning that milestone 07 adds there.

## Milestone 01: Notice codes and wording

> Goal: every notice this plan raises exists as a code with its UI record and its words in every language, so the
> later milestones only raise them and never touch the catalogs.
> Completion criteria: the codes of "Notices" are declared, documented in `apps/daemon/PROTOCOL.md` and worded;
> `daemonCodes.test.ts` and the protocol binding and schema checks pass.

### Technical design

- [x] The eight core codes of "Notices" as `notice_code` constants (`apps/daemon/src/protocol/codes.rs`), names
      settled here.
- [x] `launch_enhancements_unavailable` carries the failure the way `launch_failed` does (`CodedError::nesting`,
      `nested_params`): `reason`, `reason_code` and the failure's own params, so a path or a setting it names reaches
      the client.
- [x] `shell_environment_unavailable` and `session_git_hooks_unavailable` carry a `cause` from a closed vocabulary
      (`earlier_capture`, `daemon`; `git_too_old`, `user_configuration`, `other`), each with a normaliser in the
      UI.
- [x] A UI record for each code (`packages/ui/src/errors/codes.ts`): `launch_enhancements_unavailable` with the
      `nested: "failureAndRemedy"` facet and no `failureNotice`; none is `acknowledgedNotice`; each names the remedy
      the banner draws, where there is a step to take.
- [x] Words `daemon.<code>` in every catalog: what is unavailable, and that the terminal itself works as usual. The
      two bare-launch notices also say that approval settings chosen in Octoboard are not applied and the CLI's own
      configuration decides. No agent is named outside a param.
- [x] The rows of `apps/daemon/PROTOCOL.md`, "`session_notice` codes", with when each is withdrawn.

### Implementation plan

- [x] Declare the codes and their PROTOCOL rows first; the UI check then fails until each has a record and words.
- [x] Write the English sentences, then the other catalogs, as one copy pass.

### Notes for the developer

- Reusable capabilities: the nested-failure facet and its wording in the UI's error catalog; the cause normalisers in
  the UI's error layer; `Notice` and `Code` in the daemon's protocol module.
- Development notes: the UI standard wins over the `knowledge-i18n-copy` skill where they disagree; the sentences are
  read in the banner, and as a toast by a window that predates it, so each stands on its own; a code declared here is
  raised by nothing until its milestone lands, which the checks allow.
- Reference docs: `docs/conventions/error-codes.md`, `docs/conventions/ui-standard.md`,
  `docs/conventions/i18n-in-code.md`, `docs/conventions/protocol-and-contracts.md`, `apps/daemon/PROTOCOL.md` ("Coded
  messages").

## Milestone 02: Standing notices and the terminal banner

> Goal: a degradation notice stays for as long as its condition holds and is shown, dismissibly, in the terminal area
> of the session it is about, to every client whenever it connects.
> Completion criteria: a standing notice raised for a running session is on that session's record for a client that
> connects afterwards, leaves it when withdrawn or when the process ends, and is restored by a daemon that adopts the
> launch; the window shows it as a banner in that session's terminal pane and not as a toast; the gallery holds the
> banner's states and the user has confirmed them at a UI preview.

### Technical design

Daemon, generic (`apps/daemon/src/protocol`, `apps/daemon/src/state`):

- [x] `Notice` says whether it is standing; the `session_notice` event carries it as an optional `standing` field,
      left out when false, so a released client reads it as today's notice.
- [x] `Session` gains `standing_notices`, a list of `{ code, params, message, at }` in the order raised, left out
      when empty: derived state the store never reads or writes, filled in by `AppState::wire_session` from a map on
      `AppState` keyed by session.
- [x] One entry point to raise a standing notice (keep it, broadcast the event, publish the session) and one to
      withdraw it by code and optional `detail` (drop it, broadcast `session_notice_withdrawn`, publish the session).
      Raising one already kept for the same code and `detail` replaces it.
- [x] The list belongs to the launch: it is dropped where the live session is removed, and wherever the record is.
- [x] A launch takes the standing notices its caller already knows (a reading made before the launch, such as the
      account's or the record's) beside the ones its plan gives: they travel with the launch request and come back in
      `Launch::notices`, which are raised through the entry point when the launch is registered, each as its `Notice`
      says.
- [x] The standing notices a launch has at its spawn are kept in the terminal's meta, and a daemon that adopts the
      launch restores them; what is raised after the spawn is raised again, by its watch or from what the meta says
      of the launch.
- [x] The provider launch notices that describe a degradation lasting the launch become standing
      (`provider.opencode.version_unmeasured`, Qwen Code's hooks-off notices, Antigravity CLI's settings-not-read
      notice); `provider.claude.workspace_untrusted` stays the acknowledged notice it is.

UI (`packages/ui/src/store`, `packages/ui/src/terminal`):

- [x] A `session_notice` with `standing` raises no toast and no alert; the banner is drawn from the session record,
      so it is the same for a window that connected before or after.
- [x] The banner is the pane's existing notice bar extended with a kind, in its top slot: the newest notice's
      sentence, a control that opens the list of all of them (each with its sentence, its remedy where its record
      names one, the nested failure's included, and its details), and a dismiss control.
- [x] Dismissal is the window's own, per session and per notice (`code`, `detail`, `at`), kept in the client store and
      not persisted: a notice raised again by a later launch shows again.
- [x] A code the window has no record for is shown by the daemon's `message`.
- [x] Accessibility: the sentence is spoken once, from a status announcer, when the notice arrives for the session on
      screen, and the visible sentence is hidden from assistive technology so nothing is said twice; the bar is a stop
      of the F6 cycle while it shows, its controls take `focusOnPress={false}`, and focus on a control goes back to the
      terminal when the bar goes.
- [x] The fixture daemon serves standing notices as the real one does.

### Implementation plan

- [x] Add the wire fields, regenerate the bindings and the schema, and write the PROTOCOL text: the event's field, the
      record's field, and that a standing notice is kept where a plain one is not.
- [x] Build the banner against gallery scenarios first: one notice; several; a nested reason with a remedy; a notice
      with an unknown code; a narrow pane; a right-to-left language; beside the upload bar and over an ended session's
      bar.
- [x] Preview it for the user and settle Open 1 before the later milestones raise anything into it.
- [x] Update the UI standard and its accessibility half in the same change.

### Notes for the developer

- Reusable capabilities: the terminal pane's notice bar and its slot; the status announcers; the region cycle; the
  window's notification routing for daemon broadcasts; the session record's other derived fields and their filling;
  the terminal's meta and the adoption that reads it; the nested-failure wording and the remedy drawing of the error
  alert.
- Development notes: the protocol only grows (an absent `standing` is false, an absent list is empty); a new wire
  type derives both renderings; per-session derived state lives in a map on `AppState` and is dropped by hand; every
  announcement is built to a passing case in `docs/voiceover-cases.md`; a solid semantic fill is for the window's
  full-width strips only, so the bar stays a floating surface with a warning mark; the bar's own labels need words in
  every catalog. Files shared with other milestones: the UI catalogs and the generated bindings (with 01).
- Reference docs: `docs/conventions/protocol-and-contracts.md` ("The envelope and field conventions", "Versions and
  compatibility"), `docs/conventions/daemon.md` ("State"), `docs/conventions/ui-standard.md`,
  `docs/conventions/ui-components.md`, `docs/conventions/ui-accessibility.md` ("Announcements"),
  `docs/conventions/tests.md` ("UI component tests", "The UI state gallery and its fixtures"),
  `packages/ui/src/gallery/README.md`, `docs/product/toasts.md`, `docs/product/session-terminal.md`.

## Milestone 03: Launch tiers and the bare plan

> Goal: a provider's plan can no longer refuse a launch. A failure to prepare the enhanced launch, or to find the
> program that carries its arguments, starts the CLI bare and tells the user; only what a plain terminal would also
> hit refuses.
> Completion criteria: for every provider a failing `plan` yields a bare launch with
> `launch_enhancements_unavailable` nesting the failure, and a `launch_refusal` refuses; every provider's bare plan is
> built over a hostile fixture; a bare launch takes the no-integration answers. Findings 1 to 3, 6, 8, 9, 13, 14 and
> OpenCode's pending migration no longer refuse.

### Technical design

Generic, in `apps/daemon/src/adapter/mod.rs`:

- [x] `pub enum LaunchTier { Enhanced, Reduced, Bare }`, ordered, serialisable for the terminal's meta.
- [x] `pub struct BarePlan { pub args: Vec<String>, pub shaped_prompt: Option<String>, pub first_message:
      Option<String> }`: the conversation to resume, the model and the task, in the CLI's own spelling, and nothing
      else.
- [x] `fn launch_refusal(&self, spec: &LaunchSpec<'_>) -> Result<()>` on `AgentProvider`, default `Ok(())`: what stops
      every launch of this session, a plain terminal's included. `fn check_launch(&self, spec: &LaunchSpec<'_>) ->
      Result<Vec<Notice>>`, defaulting to `launch_refusal` with nothing to tell, is what the tier decision asks: a
      provider whose check also finds something the user is told at every tier, a bare launch's included, overrides it.
- [x] `fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan>` keeps its signature and changes its contract: an
      error never refuses.
- [x] `fn bare_plan(&self, spec: &LaunchSpec<'_>) -> BarePlan` on `AgentProvider`, required, with no default.
- [x] `native_binary` and `terminal_binary` keep their signatures; their errors become a reason for the bare tier.
- [x] `LaunchSpec` gains `record_roots: &[PathBuf]`, the records directories the session's earlier launches selected
      (`Session::record_roots`), for milestone 10's Pi part.

Generic, in `apps/daemon/src/session/term` and `apps/daemon/src/session`:

- [x] The tier decision as one function over the provider and the spec, apart from the spawn, so it is testable:
      refusals, then `plan`, then on its error the bare plan with the notice.
- [x] `LaunchRequest` carries the highest tier the launch may use; `Launch` returns the tier used and the CLI version
      the launch started; both are kept in the terminal's meta and on `LiveSession` for a daemon that adopts it.
- [x] A per-launch integration record on `LiveSession`, in a module of its own (`session/integration.rs`), which
      every generic reader of a per-launch answer goes through. For a bare launch it gives the no-integration answers
      of D4 without asking the provider.
- [x] The pinned-directory checks (`pinned_config_dir`, `pinned_config_dir_export`) are made by the launch before
      planning, for every tier; the bare launch's environment is the snapshot, the account's export through the
      layout, and what generic code sets.
- [x] On a launcher that is a script shim (a platform fact, not an OS branch) a bare launch gives the task as the
      first message.
- [x] A bare launch's trust watch treats the start hook as not coming, the way an adopted session's does: its input
      box with no trust screen up releases what is held.
- [x] A bare launch of a new conversation records no conversation id, even for a provider that preallocates one: the
      CLI was not given it, and a later resume must not ask for a conversation that never existed.

Providers, each in `apps/daemon/src/adapter/<id>/`:

- [x] `bare_plan` for all eight and the test-only provider.
- [x] Every refusal that is genuine moves out of `plan` into `launch_refusal`: an account directory for Antigravity
      and OpenCode (`provider_unsupported`), a Codex account directory that cannot be created. Everything else
      `plan` raises stays where it is and now degrades, Antigravity's file in both layouts included.

### Implementation plan

- [x] Introduce the tier decision with the enhanced path unchanged, then switch the error path from refusing to bare.
- [x] Move the genuine refusals in the same change, so none degrades for a commit.
- [x] What the launch itself prepares for its helpers before planning (the scratch directory, the key file, the hook
      script) joins the enhanced tier: a failure there starts bare. Go through the other `?`s between the claim and
      the spawn the same way and list any that stays a refusal with its reason.
- [x] Route the existing per-launch answers (`status_from_reports`, the cancel that drops messages, the native stash,
      the tool-listing wait, the channel) through the integration record.
- [x] Add the test-only provider's switches: a failing plan, a refusal, a bare plan to read back.

### Notes for the developer

- Reusable capabilities: the standing notice entry point of milestone 02; the nesting of a failure into a notice, in
  the protocol module; the layout's configuration directory export; the adopted session's input-box stand-in in the
  trust module; the launch's notice list (`Launch::notices`); the terminal's meta and the adoption that reads it.
- Development notes: generic code names no agent, flag or variable; a refusal added to `launch_refusal` needs the
  sentence "a plain terminal would fail the same way" to be true; the bare plan's arguments are each provider's own
  test; the bare plan carries no approval mode; the owner of a bare session is told nothing by the daemon. Files
  shared with other milestones: `session/term/mod.rs`, `coordinator/launch.rs`, `adapter/mod.rs`, every adapter's
  `mod.rs`.
- Reference docs: `docs/conventions/principles.md` (P10), `docs/conventions/provider-neutral-core.md`,
  `docs/conventions/daemon.md`, `docs/conventions/daemon-processes-and-io.md` ("Processes"),
  `docs/conventions/tests.md`, `docs/structure/daemon.md` ("Running sessions", "Shared entry points").

## Milestone 04: Early-exit relaunch

> Goal: a CLI that exits as it starts, before showing any sign of an interactive program, is started again one tier
> down, once per tier, and the user is told when the lower tier stays up.
> Completion criteria: the coordinator cases of "Test strategy" pass, among them no relaunch after a typed key, after
> any startup evidence, after an end on purpose, and no third launch when bare exits too.

### Technical design

Generic:

- [x] `EARLY_EXIT_WINDOW`, 10 seconds, read through a `#[cfg(test)]` getter (`apps/daemon/src/coordinator`).
- [x] Startup evidence kept per launch, in a module of its own under `apps/daemon/src/session`: a hook or a helper
      call of this launch reached the daemon (the hook route, the session's MCP server, the status line, the channel),
      the program made a status report, or completed a terminal title. The alternate screen and bracketed paste do not
      count: Codex turns both on before it exits refusing a permissions profile, and Copilot switches the alternate
      screen on and off as it exits. A key a client typed counts separately; the terminal's own protocol replies do not.
- [x] The decision as a function: ended by itself (not ended on purpose, no handover or stop under way, still the
      session's current process, the record not archived or deleted), inside the window, no evidence, no client key,
      and a lower tier left.
- [x] `pub struct EarlyExit { pub output: String, pub status: Option<i32> }`: what the CLI wrote, as plain text within
      a bound, and its status where the backend tells it.
- [x] `fn reduced_plan(&self, spec: &LaunchSpec<'_>, exit: &EarlyExit) -> Option<LaunchPlan>` on `AgentProvider`,
      default `None`. The plan carries its own notice of what it leaves out.
- [x] `LaunchRequest` carries the early exit the launch follows; the tier decision of milestone 03 gains the reduced
      step: after an early exit of the enhanced tier it asks `reduced_plan`, and falls to bare on `None`.
- [x] What the launch was asked (task, conversation to resume, terminal appearance) is kept with the live session for
      the window, so the lower tier repeats the same request.
- [x] One function performs the lower-tier relaunch under a claim (`apps/daemon/src/coordinator/launch.rs`). The exit
      watcher calls it with a claim it takes once the exited process is unregistered; an operation that watches its
      own launch come up (`came_up`) calls it with the claim it holds. Whoever cannot take the claim does not relaunch.
- [x] A lower tier that stayed up is remembered per agent, program and version for 10 minutes, on `AppState`, so later
      launches start there and still tell the user.

### Implementation plan

- [x] The exit watcher decides before it records the exit. An early exit is not recorded as interrupted: the session
      goes back to starting, its owner is not told, and what was queued for it stays queued.
- [x] A lower tier's notices are raised once it has shown startup evidence or outlived the window. If it exits at
      start too, nothing is raised for it.
- [x] `launch_retried_plain` for a bare relaunch that stayed up; a reduced relaunch raises only its plan's notices.
- [x] Operations that wait for a session to come up or go down count a relaunch in progress as still coming up.
- [x] Log each relaunch with the session and the tiers.

### Notes for the developer

- Reusable capabilities: the exit watcher and `unregister_live`; the launch claim; `came_up`; the title observer, the
  status report record and the parsed terminal's modes for the evidence; the classifier of terminal protocol input;
  the standing notice entry point; the stand-in launches and `fake_agent` in the test support.
- Development notes: every check is made again under the claim; no relaunch once a stop has begun; the relaunch goes
  through the shared launch entry, so `admit_launch` and every refusal apply. Files shared with other milestones:
  `coordinator/exit_watch.rs`, `coordinator/launch.rs`, `session/term/mod.rs`, `session/mod.rs`.
- Reference docs: `docs/conventions/daemon.md` ("Modules and entry points", "State", "Timing and concurrency"),
  `docs/conventions/tests.md` ("Daemon tests"), `docs/product/archiving-and-resuming-sessions.md`.

## Milestone 05: Hook absence

> Goal: a session whose hooks never arrive works as a session of a provider without hooks: its status follows the
> program's own reports, the messages held for it are released once its input box is up, and the user is told.
> Completion criteria: on a paused clock, a launch whose start hook does not come within `HOOK_FALLBACK` releases its
> held messages at the input box with no trust screen up, takes its status from reports and raises
> `hooks_not_received`; a hook arriving later withdraws it; an idle session of an agent whose first hook comes with
> its first turn raises nothing. Proven with the test-only provider; findings 18 to 21 close for each real provider
> when its part of milestone 10 answers `first_hook_due`.

### Technical design

Generic:

- [x] `fn first_hook_due(&self, spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> FirstHook` on `AgentProvider`, with
      `pub enum FirstHook { Never, AtStart, WithFirstTurn }` and the default `Never`: asked once per launch after
      `plan`, like the other per-launch questions, and kept in the terminal's meta. `Never` is a provider without
      hooks, or a launch whose plan left them out. It is a fact of its own beside the trust screen's `StartSignal`,
      which says what ends the startup, not when hooks begin.
- [x] The integration record of milestone 03 gains the hooks' state: not expected (`Never`, and nothing below
      applies), expected, absent, present. A bare launch is absent from its start.
- [x] A hook is due at the launch for `AtStart`; for `WithFirstTurn`, once a prompt is known to have gone in or the
      terminal has shown a turn running.
- [x] The watch, in a module of its own under `apps/daemon/src/session`: a hook due and none within `HOOK_FALLBACK`
      (30 seconds) moves the launch to absent; the first hook moves it to present.
- [x] While absent, the launch takes the no-hooks answer for: the status source and what reports say beside hooks,
      the wait for a turn message, the confirmation of a write by a reported prompt, the report held for a completion
      notice, the cancel that drops held messages.
- [x] The trust hold: the adopted session's stand-in (input box drawn, no trust screen up) is generalised to "the
      start hook is not coming", for an adopted session, a bare launch and an absent one. A provider whose reader
      reads no input box is released blind after the bound, only while no screen has been sighted, as today's
      fallback is.
- [x] `hooks_not_received` when a launch moves to absent after a hook was due (not for one that started absent, whose
      launch notice already says); withdrawn when it moves to present.
- [x] `conversation_id_unknown` for a launch that is absent and has no conversation id recorded (a provider that
      learns the id from its first hook, or a bare launch of any provider): its next relaunch opens a new
      conversation. Withdrawn when the id is recorded.

### Implementation plan

- [x] Move the answers `LiveSession` fixes at construction behind the integration record, so they follow the state.
- [x] Start the watch where the launch is registered and where a launch is adopted.
- [x] Let a late hook take over without undoing what hook logic armed before: the state changes which source is
      read, never a turn or a prompt a hook is waiting on.
- [x] `Screen::hook_fallback` stays as each provider declares it; `None` no longer means "for the life of the
      session".

### Notes for the developer

- Reusable capabilities: the trust state's hold and its release tasks; the program status path a provider without
  integration already takes; the turn reading; the message queue's write confirmation and the untaken-message watch;
  the standing notice entry point and its withdrawal.
- Development notes: no branch on an agent; the default `Never` keeps every provider as it is until its part of
  milestone 10 answers; the bound comes from the trust module's constant; a program that makes no status report is
  milestone 06's; the watch is started again by a daemon that adopts the launch rather than its state being stored.
  Files shared with other milestones: `session/mod.rs`, `coordinator/launch.rs`, `coordinator/adopt.rs`,
  `adapter/mod.rs`.
- Reference docs: `docs/conventions/daemon.md` ("State"), `docs/conventions/daemon-processes-and-io.md` ("Writing
  text into a session"), `docs/product/message-delivery.md`, `docs/product/session-statuses.md`,
  `docs/product/program-status-reports.md`, `docs/product/folder-trust.md`.

## Milestone 06: Status from the title when nothing reports

> Goal: a session with no hooks whose program makes no status report still reads working while a turn runs and idle
> at its prompt, from its terminal title, without a title ever producing a report to an owner.
> Completion criteria: replaying real title captures through a launch in the absent state with no status report, the
> session reads working through a turn with tool calls and through a gap between two steps, idle after the turn and
> after a cancel, and keeps its turn open while a dialog or prompt of the agent's is on screen; no report is
> synthesised and the session is never marked paused by it; the first status report, or the first hook, takes over.

### Technical design

Generic, under `apps/daemon/src/session` beside the program status path, with the turn bookkeeping in
`apps/daemon/src/state`:

- [x] The source applies only where all hold: the launch's hooks are absent, the program has made no report of its
      root status record in this launch, and the provider's reader reads activity off its titles
      (`ScreenReader::title_activity`). The first report takes over for the rest of the launch, as it already does
      for the title's activity; a hook moving the launch to present ends it at once.
- [x] Working: a running title that is fresh (`TitleActivity::shows_running`) moves the session to working and opens
      a turn where none is open, as one already under way: no prompt is attributed to it.
- [x] Idle: the title back at the prompt after a running one closes the turn only when all hold: it has stayed there
      for the title's settle and no running title came meanwhile; no dialog of the agent's is up or has just gone
      (`TurnReading::settled_at_prompt`); the parsed screen, looked at once at that moment, shows no prompt the
      provider's interaction reader recognises; and, where the provider's reader reads an input box, the box is
      drawn.
- [x] A turn closed off the title is closed as `AppState::end_turn_off_title` closes one: no report is synthesised
      for an owner, no turn message is waited for, what the session itself reported is kept, and an end of the
      agent's own that still comes closes it as its real end. Unlike a cancel read off the title, it does not mark
      the session paused.
- [x] Never from the title: waiting on the user. A dialog the screen readers recognise raises the hand as it does
      today; nothing else does.
- [x] A running title that stops coming without a prompt title after it leaves the session working, as the turn
      reading already holds; the end of the process settles it.

Residual risk, accepted and to be written into the product docs:

- A wait on the user that no screen reader recognises, with the input box still drawn, reads as idle. Nothing is
  reported for it; the next running title reads working again and opens a new turn, so the turn count is off by one.
- A normal end and a cancel are not told apart: neither is reported to an owner or marked paused. An owner hears of
  the turn only through the session's own `report` call, which a bare session cannot make.
- A title the user's own configuration turned off, or a release that changes the glyphs, leaves the session reading
  idle, as a program that says nothing does.
- A running glyph the CLI shows outside a turn reads working until it stops.

### Implementation plan

- [x] Feed the title's reading into the status path the reports already drive, as a second source behind them, rather
      than a second state machine; only the closing differs.
- [x] Reuse the title cancel watch's settle and its test hooks; where both watches could act on one turn, the absent
      state decides which runs.
- [x] Replay captures of at least Grok Build's titles, taken from real runs, for each case of the completion
      criteria.

### Notes for the developer

- Reusable capabilities: the title activity record and its freshness; the turn reading and its settle; the turn
  bookkeeping's off-title end; the dialog sighting and the interaction readers; the input box reader.
- Development notes: no branch on an agent, the reader decides; a state written outside a hook never invalidates what
  a hook armed; a title's stamps are the wall clock's, so tests hold or zero the settle through the existing hooks
  rather than waiting. Files shared with other milestones: `session/program_status.rs`, `session/mod.rs` (after 05).
- Reference docs: `docs/conventions/daemon.md` ("State"), `docs/conventions/tests.md` ("What a test asserts", "Time in
  tests"), `docs/product/turns-hooks-do-not-report.md`, `docs/product/session-statuses.md`,
  `docs/product/reporting-to-owners.md`, `docs/agent-cli-reference.md` ("Terminal titles and the titles in an agent's
  own records").

## Milestone 07: Resume without proof of absence

> Goal: a conversation is refused a resume, forgotten or deleted with its archived session only on proof; a record
> that was merely not found is resumed with the CLI's own resume option and a notice.
> Completion criteria: without a witness for the installed version a missing record judges `Unknown` for every
> file-scanning provider, nothing is deleted by the archive cleanup, and the resume launches with
> `conversation_record_not_found`; with a witness the verdicts are as today, the refusal included. Opening on an ID
> found nowhere launches instead of refusing. Finding 16.

### Technical design

Generic, in `apps/daemon/src/records` and `apps/daemon/src/coordinator`:

- [x] A witness: the provider's reader recognised the record of a running session, noted with the CLI version that
      launch started. Kept per agent and version on `AppState`, not stored.
- [x] `judge` and `presence` answer `Unresumable` and `Absent` only when the store recognises its records, its
      absence is conclusive, the search was complete and a witness exists for the version now installed for the
      session's account. A version that cannot be read has no witness.
- [x] A provider whose records are answered by its own CLI (`RecordStore::rows`) needs no witness: the answer is the
      agent's.
- [x] `Judgement` says when the record was looked for whole and not found without proof, so the resume can tell the
      user; unreadable or linked locations stay silent as today.
- [x] `resume_session`: a proven absence refuses with `conversation_unresumable` as today; everything else launches,
      with `conversation_record_not_found` handed to the launch in the not-found case.
- [x] `open_session` with a conversation to resume no longer refuses `conversation_to_resume_not_found`: it launches
      with the resume option and the notice. `request_resume_session` keeps refusing an ID found nowhere.
- [x] The archive cleanup, the forgetting of an archived owner's conversation and the `resumable` field sessions read
      follow the same verdict, so they act only with a witness.

UI, in `packages/ui/src/dialogs`:

- [x] The Resume conversation tab's "Nowhere" answer shows the agent and account pickers and a warning under the ID
      field instead of an error, and its submit opens the session on the chosen agent. Its layout is confirmed at a
      UI preview (Open 2).

### Implementation plan

- [x] Note the witness where a running session's record is located; give the judgement the witnesses through the
      locator, so it stays a function of what it is handed.
- [x] Keep `Resumability`'s three answers and the record roots a session keeps (`Locator::alternate_roots`) as they
      are; the witness is one more condition at the point a missing record becomes proof.
- [x] Update each provider's resumability tests to both sides of the witness.
- [x] Add the tab's new state to the gallery.

### Notes for the developer

- Reusable capabilities: `Resumability`, `Judgement` and the locator in the records module; `RecordStore`'s
  recognition and conclusiveness answers; the conversation lookup the Resume tab already asks; the choice warning
  component.
- Development notes: a provider's record parsing never runs under a lock request handling takes; the witness is
  noted from the version the launch started, never the version installed at the time of the note, since a CLI updates
  itself under running sessions; a default method is the answer of a provider with none of the thing. Files shared
  with other milestones: `coordinator/resume.rs` (with 09).
- Reference docs: `docs/conventions/provider-neutral-core.md` ("Daemon"), `docs/conventions/principles.md` (P6),
  `docs/conventions/daemon-processes-and-io.md` ("Paths and files"),
  `docs/product/archiving-and-resuming-sessions.md`, `docs/product/deleting-sessions.md`,
  `docs/product/resuming-a-conversation-by-its-session-id.md`.

## Milestone 08: Opening on an account read as signed out

> Goal: the user's own request to open a session on an account Octoboard last read as signed out goes ahead and says
> so, instead of being refused on a reading that may be wrong.
> Completion criteria: `open_session` from a client on such an account opens the session and raises
> `account_sign_in_unconfirmed`, which is withdrawn when a reading finds the account signed in; the pickers let the
> account be chosen with a warning; automatic choices and every other gate behave as before. Finding 17.

### Technical design

- [x] The gate for a new session the user opens from a window no longer refuses a signed-out reading. A session
      opened for someone else (an agent's tool, a scheduled run), a switch, a handoff's receiving session and every
      automatic choice keep refusing or skipping it. Where the user's open and another's share a gate today
      (`AccountGate::NewSession`), they get one each.
- [x] The opening path hands the notice to the launch with the launch request (milestone 02), so it is raised with
      the launch's other notices and kept in the terminal's meta.
- [x] Where an account's reading is published, a reading that finds it signed in withdraws the notice from the
      running sessions on that account.
- [x] What agents are told of an account is unchanged.
- [x] UI: every picker that marks a signed-out account as unusable for an open the user makes (the session dialog,
      the Resume tab's locked agent field; not the session request forms, which a switch or handoff fills) allows it and shows a `ChoiceWarning` under the
      choice; the fixture daemon answers the same way.

### Implementation plan

- [x] Change the gate and the pickers in one change, with the places listed in the change description.
- [x] Keep `account_not_signed_in` and its record for the gates that still raise it.

### Notes for the developer

- Reusable capabilities: the account gates and refusals in the accounts choice module; the account's reading as the
  one channel for its sign-in state; the choice warning component; the opening-choice helpers in the UI; the standing
  notice entry point and its withdrawal.
- Development notes: a change to why an account can or cannot be used is made at every place that picks or accepts
  one, in the same change. Files shared with other milestones: none in its wave.
- Reference docs: `docs/conventions/daemon.md` ("Agent accounts"), `docs/conventions/ui-standard.md`,
  `docs/conventions/ui-components.md`, `docs/product/opening-sessions.md`, `docs/product/logging-accounts-in.md`.

## Milestone 09: Shell environment and git hook fallbacks

> Goal: a login-shell snapshot that cannot be taken no longer refuses a launch, and a project session that cannot be
> given its git hook dispatchers says so.
> Completion criteria: with the snapshot failing, a launch and a resume go ahead on the fallback environment and
> raise `shell_environment_unavailable`; with `git` older than the hooks need, a project session launches and raises
> `session_git_hooks_unavailable` with `cause: git_too_old`. Findings 15 and 22.

### Technical design

- [x] One entry point in `apps/daemon/src/env_shell` for the environment a launch runs with, which never fails for
      want of a snapshot: the capture the launch policy allows; else the last capture however old; else the daemon's
      own environment through `outside_session`. It says which it gave and why.
- [x] Every path that resolves something for a launch takes it from that entry point (the launch, a resume's checks,
      the default account's directory at an open), so they agree on one environment.
- [x] `shell_environment_unavailable` with the launch's notices. `binary_not_found` on a fallback environment stays a
      refusal and nests the snapshot's failure, so the alert says why the CLI was not found: a daemon the desktop
      application started may not have an agent's CLI on its own `PATH`.
- [x] The other requests that need the snapshot (a CLI update, the MCP inventory, opening in the agent's
      application) keep `shell_environment_timeout`.
- [x] `session_git_hooks::session_env` answers what it could not give and why, from a closed set of causes (a `git`
      older than the hooks need, a `GIT_CONFIG_PARAMETERS` of the user's that git rejects, anything else, such as a
      directory that cannot be written); the launch turns that into `session_git_hooks_unavailable`.

### Implementation plan

- [x] Carry the fallback marker with a snapshot a caller resolved beforehand, so the launch that uses it tells it.
- [x] Keep the probe results `session_git_hooks` remembers; add the cause to what is remembered.

### Notes for the developer

- Reusable capabilities: the snapshot cache and its policies; `outside_session`; the launch's notice list; the git
  runner and its probe limits.
- Development notes: code a person starts from a terminal never takes a snapshot; a per-launch path starts no
  process where a cached answer exists. Files shared with other milestones: `session/term/mod.rs` (two call sites,
  after 04), `coordinator/resume.rs` (with 07).
- Reference docs: `docs/conventions/daemon-processes-and-io.md` ("Processes", "Git from the daemon"),
  `docs/product/launching-agents.md`, `docs/product/branch-responsible-session.md`, `docs/product/revert-gate.md`.

## Milestone 10: Per-agent degradations

> Goal: each provider's findings are closed on the generic mechanisms, with what only the provider can know (which
> options a release may drop, what its CLI prints when it refuses one, which floor a real run established, when its
> first hook comes) living in that provider.
> Completion criteria: per part, the provider's tests of its bare plan, reduced plan and floors pass against
> committed captures, its codes are worded for the degradation they now report, and its section of
> `docs/agent-cli-reference.md` records each measurement.

Eight independent parts, one per provider: its adapter directory (`apps/daemon/src/adapter/<id>/`), its descriptor
(`apps/daemon/src/providers/<id>.rs`) and its presentation module (`packages/ui/src/providers/<id>.ts`). Each is
mergeable on its own and they may be built side by side.

### Technical design

Common to every part:

- [x] Measure, by a real run, what the CLI does with an option it does not know (exits or goes on, what it prints,
      whether before it draws anything), and commit the capture. Where it goes on, there is nothing to relaunch.
- [x] Reword the provider's codes that used to refuse a launch so they state the cause only; they are now shown as
      the reason inside `launch_enhancements_unavailable`. A released code keeps its meaning.
- [x] A standing provider notice for each single enhancement a reduced plan or a floor leaves out.
- [x] `first_hook_due` for the provider (milestone 05), from when its first hook or event was measured to come: as it
      starts, with its first turn, or never for a launch whose plan leaves its hooks out.

Antigravity CLI (findings 1 to 4):

- [x] The directory flag, earlier layout (a file in both layouts included) and unusable user file errors stay as
      raised; they now degrade to a bare launch against the user's own directory, the file named through the error's
      `path`.
- [x] `reduced_plan`: where the early exit holds the measured `--settings` refusal line, plan again without the key
      it names, with a notice naming that key; any other output gives nothing.
- [x] The directory flag probe stays as it is: bounded, kept per binary and version, now a reason to degrade.

Codex (findings 5, 6):

- [x] `--dangerously-bypass-hook-trust`: a floor if a run establishes the version that introduced it; otherwise
      `reduced_plan` leaves out the flag together with Octoboard's hooks (so Codex raises no hook review), answers
      that this launch has no status integration, and tells the user.
- [x] Measure whether Codex exits on a `-c` override of a key it does not know (the hook entries, the permission
      preset's keys); those it exits on join what the reduced plan leaves out.
- [x] A shim whose native program is not found is launched bare through the shim (milestone 03); the bare plan gives
      a task as the first message there.

Grok Build (findings 7 to 9):

- [x] `--no-leader`: a floor if a run establishes one; otherwise `reduced_plan` without it and a notice.
- [x] Under Grok's sandbox, and for a pinned home never run against, the plan's error stays and the launch is bare
      against the real or pinned home. The bare plan passes no role text, since the role names tools the session
      lacks.
- [x] `refuse_unlaunchable_account`, the switch's check before it copies a conversation, stays a refusal.

GitHub Copilot CLI (finding 10):

- [x] No reduced tier: an early exit goes to bare. The stash floor stays.

Pi (finding 11, and the record kept elsewhere):

- [x] The extension's exported function is made total, so a changed extension interface turns Octoboard's extension
      off instead of ending Pi; a run first establishes what Pi does when an extension throws while loading. Milestone
      05 then tells the user that no hooks arrive.
- [x] Measure whether `--session <id>` finds a record outside the directory the current environment selects, and
      whether `--session-dir` does. The plan and the bare plan then name the kept records directory that holds the
      conversation (`LaunchSpec::record_roots`), by the option, or by the variable for that launch if a measured
      version lacks the option.

Claude Code (finding 12):

- [x] `--effort` is passed whatever the version (no proof of the release that added it); a release that refuses it exits at start and the
      early exit goes to bare. Existing floors stay only with proof of the release that added the capability (D12).
- [x] No reduced tier: an early exit goes to bare.

Qwen Code (findings 13, 14):

- [x] An unusable system settings file keeps its error; the bare launch does not set the stand-in settings variable,
      so Qwen Code reads its own file.
- [x] `node.exe` not found for the Windows entry point is launched bare through the shim (milestone 03).

OpenCode:

- [x] A pending migration keeps its error and now degrades: the bare launch runs OpenCode, which migrates itself.
- [x] The generation dispatch is kept and written as a floor (D12).
- [x] The resume check before a second-generation launch refuses only on OpenCode's own answer that the conversation
      is not there, from `check_launch`: the one refusal there that rests on proof (milestone 07) rather than on
      what a plain terminal would hit, since OpenCode 2 starts a new session under an id it does not have. A database
      not found, a timeout or an unreadable answer goes ahead with `conversation_record_not_found`, which the check
      returns, so a bare launch tells it too.
- [x] Audit for silent fallbacks: establish by a run what OpenCode does when a plugin throws while loading and make
      the plugins' entry points total; confirm a private server that does not start leaves a working launch.

### Implementation plan

- [x] Take the measurements first, in a neutral directory, and commit each capture with its CLI version and a
      near-miss beside it.
- [x] Write each provider's tests in its own files; no row is added to a shared test's table.
- [x] Each part edits only its own section of the shared docs.

### Notes for the developer

- Reusable capabilities: each adapter's existing probes and measured refusal lines (Antigravity's settings and flag
  probes); the per-launch status integration answer a provider already gives; the shared reader for JSON a user wrote;
  the account CLI runner for a short query.
- Development notes: a fact about a CLI comes from a real run, never from memory or its help text; a floor only
  switches a capability on; no test of a provider reaches the real CLI with an action that spends anything; the five
  providers added for beta.9 are in no release, so their codes may change kind in place (check the remote's
  `desktop-v*` tags first); Claude Code's, Codex's and Grok Build's are released. Files shared across parts:
  `docs/agent-cli-reference.md`, `docs/product/per-agent-launch-specifics.md`.
- Reference docs: `docs/conventions/principles.md` (P7, P10), `docs/conventions/provider-neutral-core.md`,
  `docs/conventions/error-codes.md` ("Provider codes"), `docs/conventions/tests.md` ("What a test asserts", "Daemon
  tests", "Windows and Linux runs"), `docs/agent-cli-reference.md`.
