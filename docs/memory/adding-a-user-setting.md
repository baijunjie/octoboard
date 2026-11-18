# Adding a user setting

## Only turning a setting on that itself changes outside files keeps it out of an export

The rules a new setting follows are in `docs/conventions/protocol-and-contracts.md`: the agents' settings tools ("MCP
tools and the MCP contract"), the configuration export and how the setting appears in Settings and the import review
("The configuration document"), the versioned path and host facts ("The envelope and field conventions"); a provider's
own setting is in `docs/conventions/provider-neutral-core.md`, "Daemon". One condition of the export rule they leave
implicit: a setting that lets agents act without asking or spend quota, or that has Octoboard act on files outside
`~/.octoboard/` later as it runs, still travels; only one whose turning on itself changes files outside `~/.octoboard/`
(today an account's sharing of the default configuration) stays out. If you are unsure which it is, ask.
