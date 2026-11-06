> Severity: Moderate

# Account login can return focus to another account

## Symptom

After a login succeeds and its row's Log in button disappears before the dialog closes, keyboard focus can land on
the default account's actions button instead of the account that was logged in.

## Reproduction steps

1. Check out `main` at `ce42447be` in an isolated worktree.
2. Install actual workspace dependencies with `pnpm install --offline --frozen-lockfile`.
3. In `packages/ui`, run
   `pnpm exec vitest run src/settings/accounts/AgentAccountsSection.quotasAndLogin.test.tsx`.
4. Observe the case named "puts focus on the row's actions button once a login from its own button succeeds, the
   button going before the dialog closes". It models a signed-out `claude-work` account becoming signed in before
   its login dialog closes.

The failure occurred on the first isolated baseline run (24 tests passed, one failed), and twice in the related test
suite on the session-menu branch. A separate four-file run on that branch passed the same test, so reproduction is
intermittent. Manual reproduction in the packaged application has not been verified.

## Expected vs. actual

- Expected: focus returns to the actions button of `claude-work`, as required by
  `docs/product/logging-accounts-in.md`, "The login dialog".
- Actual: the active element is named "Actions for Default (Claude Code)" instead of
  "Actions for claude-work (Claude Code)" at the assertion in the test.

## Environment

- macOS, Node.js 26.10.0, pnpm 10.33.0, Vitest 5.0.3 and the repository's jsdom test environment.
- Baseline: `main` at `ce42447be`, with local dependency installation and no source modifications.
- The fixture has a signed-out owned Claude Code account and a successful login whose quota update removes its
  Log in button before the dialog closes.

## Scope of impact

The test establishes incorrect focus restoration in this event ordering. Its frequency and impact in the packaged
application are unknown. Keyboard navigation can reach the intended account again.

## Leads

- Verified: the same assertion fails on the unchanged `main` baseline and the session-menu branch.
- Verified: the failing case uses the row's Log in button and does not open an actions menu.
- Inferred: `useLoginFocusReturn` in `AccountLoginButton.tsx` preserves focus if it has already moved to a control
  other than the disappearing opener or the document body. React Aria restoration and the section's own refocus may
  race before that check, leaving focus on the default account's actions button.

## Acceptance criteria

- A successful login returns keyboard focus to the logged-in account's actions button whether its Log in button
  disappears before or after the dialog closes.
- The existing before/after tests pass consistently in isolation and in the related UI test suite.
- Cancelling a login and deliberate focus moves retain their documented behavior.
