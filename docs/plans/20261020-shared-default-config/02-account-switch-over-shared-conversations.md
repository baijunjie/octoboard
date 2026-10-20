# 02 Account switch over shared conversations

> Goal: switching a session between accounts whose conversation files are the same files is safe and copies nothing.
> Completion criteria: daemon tests show that a switch between two accounts whose conversation directories resolve to
> the same place leaves the conversation intact and copies nothing, Codex's thread record included; a switch between
> accounts that do not share still copies exactly as today.

## Technical design

- [ ] Before copying, the account switch compares where the source and the target conversation files actually resolve
      to; when they are the same, the copy is skipped, item by item.
- [ ] For Codex, the update of the thread's `rollout_path` in the target follows the same rule, and is not needed where
      the state database itself is shared.

## Notes for the developer

- **Reusable capabilities**: the existing account switch and its per-agent copy of the conversation.
- **Development notes**: this lands before any account can be linked (03), so no switch can ever copy a file onto
  itself.
- **Reference docs**: `docs/product/switching-session-accounts.md`.
