# 03 Shared session directories

> Goal: an account can opt to share its session directories with the default account; switching a session between
> them then moves nothing, and every Octoboard feature that reads agent records keeps working for that account.
> Completion criteria: daemon tests cover turning sharing on (with merge), off, and account removal; switching between
> sharing accounts copies nothing; usage, titles, presence and trash-on-delete work for a sharing account; the setting
> survives configuration export and import; Settings shows the option; the UI preview is confirmed by the user.

## Technical design

- [ ] A per-account "share sessions with the default account" setting for Claude Code, Codex and Grok Build
      accounts, stored with the account and carried by the configuration export.
- [ ] A resolved records root per account: where its session records really live, used by record lookup, the record
      index, usage collection, presence and trash-on-delete instead of refusing linked directories.

## Implementation plan

- [ ] Turning sharing on: merge the account's existing records into the default account's directories (conversations
      present in both resolved by keeping the most recently written), then replace the account's directories with
      links to the default's, per the directory list in the overview.
- [ ] Turning sharing off and removing the account: remove the links Octoboard created, leaving the account with
      empty session directories; never delete the default account's records.
- [ ] Re-create the links on configuration import and at daemon start when the setting is on and a link is missing.
- [ ] The account switch treats two accounts sharing a directory as needing no copy (it already no-ops on the same
      file; extend it to the resolved records root).
- [ ] Settings → Agent accounts: the option per account, with what it shares stated per the help tip rule.
- [ ] Document the exception to "never edit the user's files" where that rule is stated, and the behaviour in the
      product docs.

## Notes for the developer

- **Prerequisites**: milestone 1 (complete migration, used by the merge) and milestone 2's measured answers.
- **Reusable capabilities**: the relocation copy; the agent record index; the configuration export; Grok Build's
  per-session home, which already links `sessions/`.
- **Reference docs**: `docs/agent-cli-reference.md` ("Moving a conversation to another config directory", "What a
  probe touches"), `docs/product/sessions.md`, `docs/product/settings.md` ("Agent accounts"),
  `docs/product/configuration-export-and-import.md`.
