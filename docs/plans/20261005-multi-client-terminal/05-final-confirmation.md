# 05 Final confirmation

> Goal: confirm what the earlier milestones built and could not verify when they closed, and that nothing later
> depends on.
> Completion criteria: every item below is confirmed, and any defect it turns up is fixed here.

- [ ] The push run (`pnpm verify`) passes the tests that verify the flow-control and query-answering criteria, which
      no manual run covered whole:
  - flow control: `terminal::fan::tests::a_client_that_falls_past_the_cap_gets_a_fresh_paint_and_the_others_every_byte`
    and `server::terminal::tests::a_client_that_stops_reading_through_a_flood_stays_connected_and_resumes_on_a_fresh_paint`;
  - queries: `server::terminal::tests::the_daemon_answers_the_programs_queries_in_the_most_recently_active_clients_colours`,
    `server::tests` (no `CSI 6n` reaching a client, the routing of a passed-through answer), `terminal::queries`,
    `terminal::display`, `terminal::snapshot::tests`, `terminal::snapshot::paint::tests`, `terminal::query_log`,
    `terminal::reply_route`, `terminal::pty::tests`, `protocol::bindings` and `session::trust::tests`;
  - the size rule: `terminal::activity`, `terminal::fan`,
    `server::terminal::tests::the_terminal_takes_the_size_of_the_client_that_typed_last` and
    `server::terminal::tests::a_registration_settles_at_its_deadline_and_lets_its_client_go`.
