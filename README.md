# Octoboard

A desktop control board for orchestrating coding agents across multiple projects.

> **Status: early development.** The shell works — consoles, projects, sessions in real terminals, archiving and
> resuming — on macOS, built from source. The orchestration it exists for (a hub agent dispatching work across
> projects and reporting back) is not built yet, and there is no installable build.

## This is an AI-native project

Every line of code in this repository is written by AI. Humans set the direction, review the result, and decide what
gets built — they do not hand-write the code.

Because of that, the contribution model is unusual:

- **Pull requests are not accepted.** Any PR will be closed without review.
- **[Open an issue](https://github.com/baijunjie/octoboard/issues) instead.** Describe the bug, the missing
  capability, or the idea. Once an issue is accepted, an AI agent is assigned to implement it.

## What it is

In Octoboard, a **console** groups a set of projects and has its own **hub agent**. You hand a request to the hub
agent, and it decides which project the task belongs to, starts a dedicated agent session in that project's
directory, and hands the task over. When the session finishes it reports back to the hub, and the hub reports to you.
You can switch to any session at any moment to watch it or take over by typing into its terminal directly.

Octoboard is neither a new agent nor a new terminal. Every session is a native agent CLI process — Claude Code,
Codex, Grok Build — running in the project directory so that the project's own configuration (`CLAUDE.md` /
`AGENTS.md`, skills, hooks, permissions) stays in effect. Octoboard only adds a layer of organization, orchestration,
and observation on top, and is not tied to any agent vendor.

## License

[MIT](LICENSE)
