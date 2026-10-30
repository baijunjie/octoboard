# Changing the agents' MCP tools

## Judge an MCP tool from the agent's side: its description against its result, its reach against the UI's

Applies when adding or changing a tool of the Octoboard MCP server (`apps/daemon/src/mcp/`) or a role text an agent is
launched with, and when auditing whether the tools cover what the app can do. An agent knows only what the tool
descriptions and its role text say and what the results return, so a gap is rarely a missing tool, and matching the
app's features against tool names finds almost none. Check three layers:

- The tool exists for the operation.
- Its parameters and result match those of the same operation in the UI and the client protocol
  (`apps/daemon/PROTOCOL.md`), and its description matches its real result word by word: every field it names is
  returned, and nothing it tells the agent to use (the agent's own session id, an account's standing) is data no tool
  gives it.
- A walkthrough reading only the descriptions and role texts, through the tasks agents actually do — taking over
  interrupted sessions, learning their own id, choosing an account, recovering from an exhausted quota, resuming a
  conversation, transferring sessions, paging the archive — asking at each step where the agent gets what it needs and
  how it hears of what it has to react to.

## A tool that waits on the user, or tells its caller its outcome by message, is registered to run on the call

Applies when adding an MCP tool to `apps/daemon/src/mcp/`. `mcp::exec::call` runs every tool on a task of its own, so a
call runs to its end whether or not the agent cancels it or its connection goes; an ordinary tool needs nothing more.
Two kinds must instead be named in `apps/daemon/src/mcp/mod.rs`, or they silently lose their cancel path:

- A tool whose call waits in front of the user (a request the user approves) and must be withdrawn when the agent
  cancels it goes into `WITHDRAWN_ON_CANCEL`: the stdio bridge closes the connection on a cancel only for those, and
  `exec::call` keeps those on the connection's own task, so the connection closing drops the call and withdraws it.
- A tool that runs its operation through `session_request::run_to_the_end`, so that the caller is told the outcome by
  message when the call is gone, goes into `runs_on_the_call`: wrapped in the outer task, the call's reply receiver
  outlives the connection, the reply never fails, and the caller is never told.
