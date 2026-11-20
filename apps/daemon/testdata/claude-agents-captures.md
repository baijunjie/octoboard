# Claude Code `agents --json` captures

`claude_agents_2_1_329_holding.json` and `claude_agents_2_1_329_stopped.json` are the output of `claude agents --json
--all` on Claude Code 2.1.329, entries filtered to one working directory, every field as Claude Code printed it.

The first was taken while a background session (`claude --bg`) held a conversation: it lists finished and stopped
background sessions (no `pid`), the running background session holding the conversation (`pid`, short `id`, and the
conversation's own id as `sessionId`), and an interactive session, which has a `pid` but no `id` field. The second was
taken right after `claude stop <short id>` ended that background session: its entry is still listed, without its
`pid`.

`adapter/claude/background_sessions.rs` replays both to find the background session to stop before a resume.
