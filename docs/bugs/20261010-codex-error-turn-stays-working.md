> Severity: Major

# A Codex turn that ends in an error leaves the session working

## Symptom

When a Codex turn ends in an error — observed with the account's usage limit reached — the session keeps reading as
*working* although Codex is back at its prompt.

## Reproduction steps

Precondition: a Codex account whose usage limit is reached, so that every turn ends at once with "You've hit your usage
limit".

1. Open a Codex project session in any project (unbound is enough), and wait for it to read *awaiting instructions*
   with Codex at its prompt.
2. Type any prompt (e.g. `Say hello.`) and press Enter.
3. Codex prints "■ You've hit your usage limit. …" and returns to "› Ask Codex to do anything" within about a second.
4. Look at the session's status in the sidebar, or the `session_upserted` events on the daemon's control socket: it
   moved to *working* and stays there. Further prompts change nothing.

Codex may open an "Approaching rate limits — Switch to gpt-6-luna?" choice after the first failure; Escape closes it,
and the failures after it behave the same.

## Expected vs. actual

- Expected: once the turn has ended with an error and Codex is back at its prompt, the session reads *awaiting
  instructions*. This rests on a position the user confirmed on the spot (2026-10-10): Octoboard should notice that a
  Codex turn ended with an error even though Codex fires no `Stop`, and return the session to awaiting instructions.
  Context in `docs/product/sessions.md`, "Session statuses": *working* is "The agent is executing a turn", *awaiting
  instructions* is "The process is running and sitting at its prompt"; the "What the statuses are derived from" list
  of cases where the status stays at its last reported value does not include this one.
- Actual: the session stays *working* for as long as it was watched (over five minutes, until the daemon was stopped).

## Environment

- Codex 0.160.0, ChatGPT login with the usage limit reached; model `gpt-6.1-sol` (the same after switching to
  `gpt-6-luna`).
- macOS (Darwin 25.2.0), Apple Silicon; `octoboardd` built from branch `feat/lead-sessions` at `8034d53`, run both
  inside the packaged app and standalone from a shell, each with a throwaway `HOME`.
- Other error causes (network failure, server errors) were not tried.

## Scope of impact

- Every Codex session whose turn ends in an error: the sidebar and focus mode show it working when it is idle.
- For a session bound to an owner: whether the owner receives a synthesised report for such a turn is Unknown (the one
  bound case observed was a turn whose report was suppressed for another reason).
- Workaround: Unknown; a further prompt does not correct it.

## Leads

- Verified: with the injected hook script wrapped to log every invocation, a usage-limit turn fires only
  `UserPromptSubmit` (payload fields `turn_id`, `hook_event_name`, `model`, `permission_mode` besides the base ones) and
  no `Stop` or any other event.
- Verified: Codex's own rollout file (`<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<timestamp>-<thread id>.jsonl`) records
  each such turn as `task_started` followed by `task_complete` with `last_agent_message: null` and an `error` object
  whose `message` is the usage-limit text, about 0.7 s apart.
- Verified: in one run inside the app, the first usage-limit turn of a session did return to *awaiting instructions*
  (status events working then idle within the same millisecond); every later failed turn in that run, and every
  failed turn in the standalone run, stayed *working*. What ended that first turn is Unknown.
- Verified: `docs/agent-cli-reference.md` ("Codex") lists `Stop` and `Interrupt` as Codex's turn-ending events; Codex
  has no `StopFailure` counterpart there.

## Acceptance criteria

- [ ] Following the reproduction steps, the session returns to *awaiting instructions* once Codex has printed the
      error and is back at its prompt.
- [ ] The same holds for every further prompt that fails the same way.
- [ ] A Codex turn that ends normally (with `Stop`) still moves the session to *awaiting instructions* as before.
