# 09 Final confirmation

> Goal: confirm what the earlier milestones built and could not verify when they closed, and that nothing later
> depends on.
> Completion criteria: every item below is confirmed, and any defect it turns up is fixed here.

## Program status reports

- [ ] The push run (`pnpm verify`) passes the tests that verify the program-status criteria no shipped build can show
      (every shipped agent has a status integration of its own, so only the test provider takes its status from
      OSC 7501): `terminal::program_status::tests`; `terminal::queries::tests::a_programs_status_reports_and_support_query_are_cut_out`
      and `a_status_report_is_held_back_up_to_its_limit_and_no_further`;
      `terminal::snapshot::tests::the_program_status_support_query_is_answered_where_status_is_kept_on_every_host`,
      `status_reports_move_the_root_record_and_are_left_out_of_the_delivered_output` and
      `a_prompt_start_ends_working_and_blocked_records_and_still_reaches_the_clients`; `session::program_status::tests`;
      `coordinator::exit_watch::tests` and `state::process_exit::tests`; and
      `adapter::tests::every_real_agent_has_a_status_integration` and
      `the_test_provider_launches_without_any_optional_part`.

## Checked writes

- [ ] The push run passes `coordinator::checked_write_tests`, `coordinator::settings`, `state::field_versions`,
      `config_document::import` and `server::control`, and the full UI suite; and the full gallery sweep before that
      push passes, since the shared fixture `snapshotState` changed.
- [ ] A native read of the checked-write strings in the 16 non-English catalogs (`packages/ui/src/i18n/messages/`),
      the implementers' drafts: `daemon.write_conflict` (with `.summary` and `.recovery`), `dialog.changedElsewhere`,
      `configuration.import.conflict`, `configuration.import.refreshFailedTitle`,
      `configuration.import.refreshFailedChooseAgain`, `configuration.import.reviewUpdated`, and the
      `configuration.import.result.*.conflict` keys with their `.remedy`.
- [x] A VoiceOver listening pass of the checked-write announcements and alerts, recorded in `docs/voiceover-cases.md`:
      SA-Q separately heard `ConsoleDialog`, `ProjectDialog`, `AccountDialog` and `CloneDirSetting` field replacements;
      SA-R heard `ImportConfigurationDialog` read the review again and retry successfully; ALERT-2 heard the real
      `write_conflict` alert and import-submit refusal; ALERT-3 heard the re-read failure, repeated failure and Choose file
      recovery after fixing the action and announcement delay (KNW-13 and KNW-14 retain the earlier failures); and
      ALERT-4 heard both the application-settings and account-settings conflict results. The cases state their measured
      attended focus relations and the variants outside their reuse scope.
