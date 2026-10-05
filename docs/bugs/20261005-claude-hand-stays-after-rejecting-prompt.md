> Severity: Moderate

# A Claude Code session keeps its raised hand after the user rejects a permission prompt

## Symptom

After the user rejects a permission prompt in a Claude Code session, the agent returns to its empty input line, but Octoboard
still shows the session as waiting for the user (raised hand on the session, project and console rows, Dock badge still
counting it).

## Reproduction steps

1. Open a Claude Code session in any project and switch it out of auto mode (Shift+Tab until the bottom bar reads
   `manual mode on`).
2. Send a task that needs a permission decision, e.g. `执行 touch ~/octoboard-hand-test-hand3.txt`. The session raises its
   hand.
3. Answer that prompt with **Yes**. Claude Code runs the command, then immediately asks permission for the
   `octoboard – Report Tool` MCP call; the hand stays up, which is expected since a prompt is open.
4. Answer that second prompt with **No**. Claude Code prints `Interrupted · What should Claude do instead?` and shows an
   empty input line.
5. Wait ten seconds or more and look at the sidebar.

## Expected vs. actual

- Expected: once the prompt is rejected the session is no longer waiting on a permission decision, so the raised hand is
  released at every row level and the Dock badge drops by one. Basis: `docs/product/sessions.md:113` defines *Waiting for
  the user* as "waiting on a permission decision"; the user confirmed on the spot that a rejected prompt must not leave the
  hand up. The process is still running at that point, so the status table (`:114-115`) puts the session in *awaiting
  instructions*, not *Interrupted* (which means no process is running); the user's own wording for the expected state was
  "interrupted".
- Actual: the session row, its project row and the Dock badge (reads 3, unchanged) keep the raised hand indefinitely while the
  agent sits idle at its prompt.

## Environment

- Octoboard packaged build `/Applications/MyApp/Octoboard.app`, built from commit `e257db7`; macOS 26.3.1.
- Claude Code 2.1.289, session in `manual mode`, project `eslint-config-share`; three sessions had raised their hands at
  once at the time (Dock badge 3).
- Grok Build 1.0.46 for the comparison below.

## Scope of impact

Any Claude Code session whose permission prompt is rejected. The stale hand and Dock badge count remain until something
else moves the session's status. Whether sending a new prompt clears it: Unknown.

## Leads

- Verified: the same rejection in a Grok Build session releases the hand (reported by the user from their own test; not
  re-observed in this session). Only Claude Code is affected.
- Inferred, not checked against a captured payload: rejecting a prompt in Claude Code ends the turn without a `Stop` hook
  event, so nothing moves the session off `waiting_user` (`daemon/src/hooks.rs` maps `Stop` to idle and `PermissionDenied`
  to working in the Claude-side mappings; whether Claude Code sends either on a rejection is the open question).

## Acceptance criteria

- [ ] After rejecting a permission prompt in a Claude Code session, the session leaves *Waiting for the user* and the raised
      hand disappears from the session row, its project row and its console row.
- [ ] The Dock badge count drops by one when that happens, and no notification is left pending for it.
- [ ] A later permission prompt in the same session still raises the hand and fires a fresh notification.
- [ ] Grok Build and Codex behaviour on a rejected prompt is unchanged.
