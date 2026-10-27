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
