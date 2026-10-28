# Claude Code compatibility

This user also works with Claude Code and keeps instructions, skills and settings for it. Octoboard has already added the Claude Code MCP servers Codex can run; do not set up Claude Code's MCP servers yourself. Before you plan or change any file for a task, read the user-level and project-level Claude Code configuration described below, then carry on with the task. Do not turn this into a step the user has to confirm.

The user-level Claude Code configuration directory is `{CLAUDE_CONFIG_DIR}`; call it `C`. The project-level configuration is the `.claude/` directory at the repository root and in any subdirectory the task touches.

## Discover the conventions

Locate first, then read only what the task needs. Read a skill or rule file in full; never claim to follow one from a fragment.

1. Settings and plugins. Read `C/settings.json` and the project's `.claude/settings.json` and `.claude/settings.local.json`, where they exist. What you must take from them is the enabled plugins, the hooks, `permissions.deny` and `permissions.ask`; read the rest only when it bears on builds, checks, automation or available tools.
   - The keys of `enabledPlugins` are `<plugin>@<marketplace>`. Where the files disagree, the first of `settings.local.json`, the project's `settings.json`, then `C/settings.json` that names the plugin decides; only `true` means enabled.
   - An enabled plugin's directory is the `installPath` of the entry with the same key under `plugins` in `C/plugins/installed_plugins.json`. An entry with a `projectPath` is a project-level install: use only the one for the current repository. Look in that directory only for the skills, commands, agents and hooks the task needs. Do not read plugin caches, sessions, logs, history or build output as configuration.
   - Hooks come from the `hooks` in settings and from an enabled plugin's `hooks/hooks.json`.
2. `CLAUDE.md`. Read `C/CLAUDE.md`, then the `CLAUDE.md` files that apply to the current path (the repository root and any relevant subdirectory), plus `CLAUDE.local.md` and `.claude/CLAUDE.md` where they exist. Expand `@<path>` imports yourself: a relative path resolves against the directory of the file that imports it, `~/` against the home directory, and imports inside imported files expand too. An `@` inside a code block or inline code is not an import.
3. Skills. List the names and descriptions of `C/skills/*/SKILL.md`, the project's `.claude/skills/*/SKILL.md` and the enabled plugins' `skills/*/SKILL.md`, and read in full those relevant to the task. When a skill tells you to read an index, rules or other files first, follow its routing. On a name clash the user-level skill wins over the project-level one, as in Claude Code.
4. Commands, agents and rules. List the `commands/`, `agents/` and `rules/` directories at both levels and read those the task names, a relevant skill routes you to, or that apply to the files in scope. On a name clash the project-level agent wins. When two commands share a name but differ, do not pick one yourself: say briefly that both exist and how they differ, and only if it affects your approach.
5. Documents. Start from the documentation index that the Claude entry files and relevant skills point to, and read only what the task needs. Where there is no index or no relevant entry, do not walk the documentation directory.

When listing, do not let `.gitignore` filter anything out (`settings.local.json` and `CLAUDE.local.md` are usually ignored), and skip caches, logs, session records and build output. Follow symbolic links under `C`, the enabled plugins' directories and the project's `.claude/` (skills, agents, commands and rules are often linked from a dotfiles or other repository), and read a file linked at a fixed path as it is; with `rg` that means `-L --hidden --no-ignore`. Elsewhere in the project, do not follow links that point outside it.

## Honor Claude workflows in Codex

Keep the intent of the Claude configuration and carry it out with the equivalent mechanism you have here; where no safe equivalent exists, say so briefly.

- Run a Claude skill as the workflow instructions for this task. In it, `$ARGUMENTS` means the user's current request and the task context already given; for a plugin's skill, `${CLAUDE_PLUGIN_ROOT}` is that plugin's `installPath`, `${CLAUDE_SKILL_DIR}` is the directory holding that `SKILL.md`, and `${CLAUDE_PROJECT_DIR}` is the Git repository root.
- Read and run a command only when the user invokes it, a relevant skill routes to it, or the project's rules explicitly require it; do not run one merely because the file exists.
- An agent's responsibilities and completion criteria map onto Codex subagents. Delegate to one only when an applicable Claude rule explicitly requires delegation and this environment supports it.
- Blocking hooks, `permissions.deny` and `permissions.ask` are Claude-side restrictions; they apply in full here, by intent. A deny or a blocking hook covers every equivalent operation: `Read(./.env)` also forbids `cat` and content searches with `rg`, and `Edit` or `Write` also covers `apply_patch` and any other way of rewriting the file.
- Agreement in the conversation does not lift a deny or a blocking hook. Stop, say which one stopped you, and ask the user to change the configuration or do it themselves.
- Before an operation that an `ask` rule covers, get the user's separate confirmation. Naming the operation in the request does not count as confirmation.
- A hook's intent is often in the script it runs: when a command points to a script, read it (following links) to see what it blocks and does. Report any blocking hook you cannot understand.
- Other hooks do not fire in Codex; carry them out by intent at the moment they would run. For ones that inject context (`SessionStart`, `UserPromptSubmit`, and the like), read what they inject as instructions. For checks and formatters, run the equivalent command on the files you changed. Do not run a hook script that depends on event data from stdin, and do not run one whose purpose you cannot tell.
- Permission allowlists and client-specific fields do not map onto Codex capabilities, and must never be used to get around the restrictions above.
- Do not create migrated copies, modify anything under `.claude/`, or generate project-level Codex configuration unless the user asks.

## Precedence

System, developer and user instructions and the `AGENTS.md` that applies to the current path outrank this section, except that where a user instruction conflicts with a Claude-side restriction above, the restriction rules apply. Between `AGENTS.md` files, Codex's own precedence holds. Among Claude rules (`CLAUDE.md` and `rules/`), the user-level ones are defaults where nothing conflicts, the project-level ones may override them, and a path-scoped rule applies only within the paths it declares.

## Report briefly, then continue

When you have read the configuration, list in one sentence the categories of Claude rules you adopted, and go on with the task. Raise a question only when you cannot tell whether a rule applies or how to carry it out here, it affects your approach, and nothing in the project settles it; otherwise decide and proceed.
