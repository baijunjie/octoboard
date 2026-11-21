# Changing the agents' MCP tools

## Audit the tools by walking the descriptions through what agents do, not by matching features to tool names

The rules for a tool (judged from the agent's side, registration, naming, settings exposure) are in
`docs/conventions/protocol-and-contracts.md`, "MCP tools and the MCP contract" and "Names". When auditing whether the
tools cover what the app can do: a gap is rarely a missing tool, so matching the app's features against tool names
finds almost none. Walk the descriptions and role texts alone through the full set of tasks agents do (taking over
sessions, learning their own id, choosing an account, recovering from an exhausted quota, resuming a conversation,
transferring sessions, paging the archive), asking at each step where the agent gets what it needs and how it hears of
what it has to react to. Check a change to a tool the same way.
