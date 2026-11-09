# 05 Program status reports

> Goal: a program in a session's terminal can report its own status through OSC 7501, the Program Status Protocol.
> Completion criteria:
> - In a session whose agent has no status integration, a program writing OSC 7501 reports moves its session through
>   working, waiting for the user and idle.
> - A report that breaks a limit below, or carries an unknown state, is ignored.
> - A program in any session's terminal asking whether the protocol is supported gets an answer.
> - In a session whose hooks or agent signals give its status, OSC 7501 reports do not change it.
> - Reports read before the program's process ended are applied before the exit is recorded.
> - A shell prompt (OSC 133 A) and the end of the process forget the `working` and `blocked` records.

## Technical design

- [x] Parsing OSC 7501 reports out of terminal output:
  - A report is `OSC 7501 ; key=value:key=value… ST`, where `ST` is `ESC \` or `BEL`.
  - `OSC 7501 ; ? ST` asks whether the protocol is supported, and which states and kinds are.
- [x] The fields:
  - `state` is required: `idle`, `working`, `done`, `blocked` or `error`, or `clear`. An unknown state discards the
    whole report.
  - `id` is a `/`-separated path addressing a record. Without it, the report addresses the root record.
  - `kind` (only with `blocked`) is `permission`, `question` or `auth`.
  - `progress` (only with `working` or `blocked`) is 0 to 100.
  - `app` is a stable program identifier.
  - `title` and `msg` are base64 UTF-8 text.
- [x] The limits:
  - The whole sequence is at most 4096 bytes, and a key at most 16 bytes.
  - `msg` is at most 2048 bytes decoded, and `title` at most 192.
  - `app` is at most 32 bytes.
  - `id` is at most 128 bytes, with at most 32 per segment and 8 levels.
  - A terminal keeps at most 256 records.
- [x] Records by id. Each report replaces its record. `clear` removes the addressed record and its children, or every
      record without an id. The root record is the program's status.
- [x] A turn ends when the root record leaves `working` and `blocked`: a blocked program is held up within its turn,
      as an agent asking for a permission is. A turn starts when the root goes to either from neither.
- [x] The support query, answered by the daemon in every session's terminal.
- [x] The lifetime of records: a new shell prompt (OSC 133 A) and the end of the process forget every `working` and
      `blocked` record and keep `done`, `error` and `idle`; a terminal reset (RIS) forgets every record, a soft reset
      (DECSTR) none. The end of the process reports no move: it settles the status itself.
- [x] Reports already read when the process ends are applied before the exit is recorded, bounded as the hook queue's
      settling is; nothing applied after it reopens a turn or moves the status.
- [x] Mapping the reported states onto the wire status: `working` to `working`, `blocked` to `waiting_user`, `idle`
      and `done` to `idle` with `paused` false, and `error` to `idle` with `paused` true. Clients still tell done from
      awaiting instructions as today. Dormant and working in background never come from OSC 7501.
- [x] Where hooks or an agent's own signals give a session's status, they win over OSC 7501.

## Notes for the developer

- **Reusable capabilities**:
  - The daemon's continuous output parser.
  - The `AgentProvider` behavior interface, through which each agent's status reaches the rest of the daemon.
- **Development notes**: builds on daemon-side answering of terminal queries: the daemon answers the queries it can
  from its own parsed terminal, whether or not a client is attached, and takes them out of what the clients receive
  (`QueryScanner` in `terminal/queries.rs` cuts them out, `TerminalSnapshot::take_in` in
  `terminal/snapshot/answers.rs` answers them).
- **`TermFacts::host_answers_queries` and `TermFacts::host_waits_on_queries`** (`apps/daemon/src/platform/term/mod.rs`):
  where the host answers the device attributes, status and cursor reports itself (`host_answers_queries`, the
  system's console host on Windows), it does not answer OSC 7501, which reaches the daemon's parser on every host,
  so the support query is still the daemon's to answer there. Where the host holds the program while a query goes
  unanswered (`host_waits_on_queries`, the console host bundled on Windows), the support query is answered at once by
  the same path as every query the daemon answers, whether or not a client is attached.
- **Reference docs**: `docs/product/session-statuses.md`, `docs/agent-cli-reference.md`, and the Program Status
  Protocol specification, revision 0.4 (<https://www.superlogical.com/rex/docs/build/program-status>).

## Mid-task handoff

Every design item is implemented and its tests are written, but nothing has been compiled or run: by the user's
decision the machine compiles and tests nothing until every milestone of this round is written. The completion
criteria stay unticked until the run below passes.

- **Still to do**: compile the daemon and run `cargo test` for these tests, fix what fails, then tick the completion
  criteria and close the milestone:
  - `terminal::program_status::tests` (fields, limits, unknown states, records, clear, eviction, a prompt or an
    exit forgetting working and blocked records, reset);
  - `terminal::queries::tests::a_programs_status_reports_and_support_query_are_cut_out` (prompt starts included) and
    `a_status_report_is_held_back_up_to_its_limit_and_no_further`;
  - `terminal::snapshot::tests::the_program_status_support_query_is_answered_where_status_is_kept_on_every_host`,
    `status_reports_move_the_root_record_and_are_left_out_of_the_delivered_output` and
    `a_prompt_start_ends_working_and_blocked_records_and_still_reaches_the_clients`;
  - `session::program_status::tests` (the mapping, turns and pause, a session moved end to end, early reports,
    Claude Code, Codex and Grok Build sessions keeping reports from the clients without moving, a report still
    being applied at exit deciding the turn, a prompt ending a turn, a move after the process ended);
  - the exit watcher's and shutdown's existing tests (`coordinator::exit_watch::tests`,
    `state::process_exit::tests`), since both now wait for the reports;
  - `adapter::tests::every_real_agent_has_a_status_integration` and
    `the_test_provider_launches_without_any_optional_part`.
- **Decisions made without the user** (the user was unavailable; each picked as the plan and conventions support):
  - "Hooks or agent signals give its status" is decided per agent, not per session: an agent whose provider declares
    a status integration (`AgentProvider::has_status_integration`: Claude Code, Codex and Grok Build) never takes its
    status from OSC 7501; any other agent always does. A per-session rule (reports count until the first hook) was
    weighed and not taken: it would let a report flash a status before an agent's first hook, and no supported agent
    lacks hooks.
  - Every session's terminal keeps the reports, takes them out of what the clients receive, and answers the support
    query, whichever its agent: the protocol answers whether the terminal accepts reports, not whether they become
    the status (changed after an independent review; an earlier version answered only where the reports give the
    status). Shell terminals of the Terminal panel do not support it: the query passes through to the clients, which
    do not answer it. No agent shipped today lacks a status integration, so in a shipped build OSC 7501 moves no
    status; only the test provider exercises that path now.
  - The exit watcher waits for the reports read before the exit (`LiveSession::settle_program_status`) after the
    output is saved and the hook queue settled, then forgets the running records, then records the exit; shutdown
    waits for them beside the hook queue, and so does a session's own `report` before it credits the report. The wait
    shares the hook queue's bound; where it runs out, the turn reads as cut short and a move applied after the exit
    is refused. It counts only what the PTY reader has taken in, and the reader's drain at exit is itself bounded.
  - A report's `msg` is not passed into the owner's report of a turn that ended: the turn end carries no assistant
    text, as a hook backstop's does not.
  - A turn spans `working` and `blocked` (see the technical design above), and a turn starting marks the session as
    having a conversation.
  - Records follow the protocol's lifetime rules in a session's terminal too (changed after the same review): an
    OSC 133 A prompt start, with or without options, forgets `working` and `blocked` records and still reaches the
    clients; `idle` records stay, which the protocol allows. A prompt start longer than 128 bytes is not recognised.
