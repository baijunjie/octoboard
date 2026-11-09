# Compatibility between agents

What a session's launch is given while one of its agent's compatibility switches is on, so that the session loads
another agent's configuration as well as its own: Claude compatibility for Codex and Grok Build, Codex compatibility
for Claude Code and Grok Build. The switches themselves, their defaults and who may read them are in "Claude
compatibility" and "Codex compatibility" in `docs/product/per-agent-settings.md`; whether a switch is on for a given
launch — the session's own value, else its project's or its console's, else the agent-level one — is in "Which value a
launch uses" in `docs/product/compatibility-switches-in-launch-settings.md`, and "on" below means on as resolved there.
The rest of each agent's launch is in `docs/product/per-agent-launch-specifics.md`.

What a switch gives is worked out afresh on every launch, a resume and an account switch included, from the files as
they are then; a running process keeps what it was started with. Nothing is written into any of Claude Code's, Codex's
or Grok Build's own files, the user's or the project's, for it. A server Octoboard cannot pass safely is left out whole
while the others are still passed, and nothing in the window says which were left out. The instructions a launch is
given are followed by the agent as instructions: Octoboard does not check what the agent reads or how it follows them.

## Claude compatibility for Codex

While Codex's Claude compatibility is on (see "Claude compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, gives Codex two things; while it is off, it gives neither. Nothing is
written into any of Claude Code's or Codex's files for it.

**A "Claude Code compatibility" section** is appended to the developer instructions Octoboard already passes (after the
role description). It names the user-level Claude Code configuration directory — the directory of Claude Code's default
account as the launch's shell environment resolves it: a `CLAUDE_CONFIG_DIR` the user's shell exports, else
`~/.claude` — and the project-level one, the `.claude/` directory at the repository root and in any subdirectory the
task touches. It tells Codex, before planning or changing any file, to read at both levels what the task needs: the
settings files and the plugins they enable, the `CLAUDE.md` files with their `@` imports (and `CLAUDE.local.md` and
`.claude/CLAUDE.md`), skills, commands, agents and rules, and the documentation the entry files point to; that Claude
Code's MCP servers have already been added, so it is not to set them up itself; to apply `permissions.deny`,
`permissions.ask` and blocking hooks in full by intent, never lifted by agreement in the conversation, to carry out
other hooks by intent at the moment they would run, and not to use permission allowlists to get around those
restrictions; not to modify `.claude/` or create Codex configuration unless asked; that system, developer and user
instructions and the applicable `AGENTS.md` outrank the section, except that a Claude-side restriction holds against a
conflicting user instruction; and to say in one sentence which kinds of Claude rules it adopted and carry on. Codex
follows this as instructions: Octoboard does not check what it reads or how it follows it.

**Claude Code's MCP servers** for the session's working directory are passed as `-c mcp_servers.<name>.…` overrides.
They are gathered as Claude Code gathers them, by server name, a later scope replacing an earlier one:

| Scope | Where it is read | Taken when |
|---|---|---|
| User | `mcpServers` at the top of Claude Code's global config file: `.claude.json` inside a `CLAUDE_CONFIG_DIR` the user's shell exports, else `~/.claude.json` | Always |
| Project | `mcpServers` of each `.mcp.json` from the filesystem root down to the working directory, the nearest one winning | Only once approved: `enableAllProjectMcpServers` is true, or the server is listed in `enabledMcpjsonServers`, and it is not listed in `disabledMcpjsonServers`, in any of the user-level `settings.json` (in the user-level directory above), the working directory's `.claude/settings.local.json` and the working directory's entry under `projects` in the global config file. The project's shared `.claude/settings.json` approves nothing, so a repository cannot approve its own servers; managed settings are not read, so a server approved only there stays off |
| Local | `mcpServers` of the working directory's entry under `projects` in the global config file | Always |

A server that the working directory's entry lists in `disabledMcpServers` is dropped from every scope. Plugins'
servers are not passed. A server is also left out when:

- its name is `octoboard` (Octoboard's own), or is not made only of letters, digits, `-` and `_`;
- a server of the same name is defined in the `config.toml` of the Codex home the launch uses, or in a project
  `.codex/config.toml` — the working directory's and each one above it up to the repository root (the nearest directory
  holding `.git`; the working directory's alone outside a repository). Codex's own definition then stands. When one of
  these files exists but cannot be read or parsed, no Claude Code server is passed at all;
- it uses a transport other than stdio and streamable HTTP (`"type": "http"`) — SSE, for one — or has a
  `headersHelper`, or a field of the wrong type, or no `command` (stdio) or `url` (HTTP);
- a `${VAR}` in it has no `:-default` and `VAR` is not set in the launch's shell environment;
- its `url` has a `${VAR}` that the environment fills, carries credentials (`user:pass@`) or has a query string;
- an `env` entry has a name that cannot be a variable's, or an `env` or header value holds a NUL byte;
- an `env` entry names `TERM`, a `GIT_CONFIG_*` or an `OCTOBOARD_*` variable, at any value;
- an `env` entry names a variable that steers Codex itself — `CODEX_*`, `OPENAI_*`, `RUST_LOG`, `RUST_BACKTRACE`,
  `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` in either letter case, `SSL_CERT_FILE`, `SSL_CERT_DIR`,
  `NODE_OPTIONS`, `DYLD_*`, `LD_*` — at a value other than the one the launch already gives it;
- an `env` entry would give a variable a value other than the one the user's shell, the launch or a server passed
  before it gives it.

**What reaches Codex's command line.** A stdio server's `command`, `args` and `cwd`, with their variables expanded, and
an HTTP server's `url` are on Codex's own argv, which other users of the machine can list; `command` and `args` become
the server's own argv when Codex starts it anyway, as under Claude Code. Header values and `env` values are kept off
it: each header value is put in the Codex process's environment under a variable of Octoboard's own,
`OCTOBOARD_CLAUDE_MCP_<n>_<m>_TOKEN`, which the server's `env_http_headers` names; each `env` entry is passed by name
through the server's `env_vars`, and an entry the user's shell does not already hold at that value is set, under its
own name, on the Codex process as a whole rather than on the server alone.

## Claude compatibility for Grok Build

Grok Build has Claude Code support of its own, for Claude Code's skills, rules, agents, MCP servers and hooks. While
Grok Build's Claude compatibility is on (see "Claude compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, sets `GROK_CLAUDE_SKILLS_ENABLED`, `GROK_CLAUDE_RULES_ENABLED`,
`GROK_CLAUDE_AGENTS_ENABLED`, `GROK_CLAUDE_MCPS_ENABLED` and `GROK_CLAUDE_HOOKS_ENABLED` to `true`, which outrank a
`false` for any of them in Grok's configuration file or the user's shell — except `GROK_CLAUDE_MCPS_ENABLED` in the case
below. While it is off, none of them is set by Octoboard, and Grok follows its own configuration and whatever the
user's shell exports; Grok's own Claude Code support is on unless its configuration turns it off, so a Grok Build
session may still load Claude Code's configuration with the switch off. Nothing is written into the user's Grok
configuration for it.

**Claude Code's MCP servers when Claude Code reads another config file.** Grok always reads Claude Code's MCP servers
from `$HOME/.claude.json` and ignores `CLAUDE_CONFIG_DIR`. When the global config file Claude Code itself reads — the
`.claude.json` inside a `CLAUDE_CONFIG_DIR` the user's shell exports — is another file than `$HOME/.claude.json`,
Octoboard gives Grok Claude Code's servers itself: it adds them, gathered as for Codex (see "Claude compatibility for
Codex" above: user, approved project and local scope, less the disabled ones), to the session's copy of Grok's
`config.toml` (see "The per-session Grok home" in `docs/product/per-agent-launch-specifics.md`), and sets
`GROK_CLAUDE_MCPS_ENABLED` to `false`, the other four variables staying `true`. The variable is set to `false` only once
the servers are in the copy: a copy that cannot be read, parsed or replaced is left as it is and the variable stays
`true`, so Grok goes on reading `$HOME/.claude.json` itself. Where Claude Code reads `$HOME/.claude.json`, or the shell
exports no `CLAUDE_CONFIG_DIR`, nothing is added and Grok reads the servers itself.

The servers are written as Grok's own `[mcp_servers.<name>]` tables under the rules of "Codex compatibility for Grok
Build" below — what Grok defines itself stands, a literal `env` or header value goes into the Grok process's
environment, as `OCTOBOARD_CLAUDE_MCP_<n>_<m>`, rather than into the file — except that a `${VAR}` reference in a
Claude Code server is left to Grok to expand rather than settled against the launch's environment. An SSE server is
written with its `url` and `headers` like a streamable HTTP one.

Grok's own reading of a project's `.mcp.json` is left on in both cases, and Grok does not apply Claude Code's approvals
of project servers there, so a project's `.mcp.json` server can reach a Grok Build session from there as well, whether
or not Claude Code has approved it.

## What Codex offers

While Codex compatibility is on for Claude Code or Grok Build, the launch reads Codex's configuration from the Codex
home of Codex's default account as the launch's shell environment resolves it: a `CODEX_HOME` the user's shell
exports, else `~/.codex`. Codex's files are read, never written.

### Codex's MCP servers

The servers are the `[mcp_servers]` tables of the Codex home's `config.toml`, layered under the project's
`.codex/config.toml` files from the repository root (the nearest directory holding `.git`; the working directory alone
outside a repository) down to the working directory, the nearest winning and tables merging as Codex merges them.

**A project directory's file counts only where Codex trusts that directory.** Each directory from the repository root
down to the working directory is decided on its own, as Codex 0.166 decides it, and before any of the project's files is
read, so a broken file in an untrusted directory changes nothing. The directory's own entry
`projects."<directory>"` in the Codex home's `config.toml` decides; without one, the entry of the repository's top (for
a linked worktree, the worktree's own top); without that, the entry of the main repository's root. The first of these
entries that has a `trust_level` decides, and only `"trusted"` trusts; an entry without a `trust_level` is passed over,
and no entry at all is untrusted. So:

| Recorded in the Codex home's `config.toml` | Layers that count |
|---|---|
| Repository trusted, a subdirectory untrusted | The repository root's only |
| Repository trusted, a subdirectory with an entry that has no `trust_level` | Both |
| Repository untrusted, a subdirectory trusted | The subdirectory's only |
| A linked worktree trusted, its main repository not recorded | The worktree's, at every level |
| The main repository trusted, a linked worktree of it untrusted | None |
| Only a directory above the repository trusted | None |

The home's own servers count whatever the trust. When a file in play — the home's `config.toml` or a trusted
directory's file — exists but cannot be read or parsed, no Codex server is passed at all.

**A server is left out whole** when:

- it is `enabled = false`;
- it has a `cwd`, `enabled_tools` or `disabled_tools`, which the consuming agent's configuration cannot express;
- it is neither stdio (`command`) nor streamable HTTP (`url`), or has a field of the wrong type;
- its `command`, an `args` entry or its `url` holds `${`;
- a literal `env` or `http_headers` value holds `${`, which the consuming agent would expand where Codex does not;
- an `env_vars` or `env_http_headers` entry names something that cannot be an environment variable;
- its `bearer_token_env_var` names a variable the launch's shell environment does not hold;
- or the consuming agent's own rules leave it out (see "Codex compatibility for Claude Code" and "Codex compatibility
  for Grok Build" below).

**Variables the server reads from the user's environment** are passed as references the consuming agent expands from
its own environment: each `env_vars` name as an `env` entry `${NAME}`, each `env_http_headers` header as `${NAME}`, and
`bearer_token_env_var` as an `Authorization: Bearer ${NAME}` header. An `env_vars` entry or an `env_http_headers`
header whose variable the launch's shell environment does not hold is dropped, as Codex would not forward it, and the
rest of the server is still passed.

**Literal `env` and `http_headers` values stay off the agent's command line** and out of any file: each goes into the
consuming agent's process environment as `OCTOBOARD_CODEX_MCP_<n>_<m>`, and the server refers to that variable. `<n>`
is the server's position among all of Codex's servers, sorted by name with the left-out ones included, and `<m>` the
entry's position within its `env` or headers. Like the rest of the agent's environment, these variables are visible to
the commands the session runs. A server's `command`, `args` and `url` are on the agent's command line or in its
configuration as they are.

### Codex's skills and custom prompts

**Skills** are the directories holding a `SKILL.md` directly under these roots, nearest first: the `.codex/skills` and
`.agents/skills` of the working directory and of each directory above it up to the repository root — a directory's only
where Codex trusts that directory, as for its `config.toml` layer above, so an untrusted repository offers no skill —
then the Codex home's `skills`, then `~/.agents/skills`. A skill is named by its directory, a name is offered once, at
its nearest place, and directories whose names start with a dot are skipped. A skill the home's `config.toml` turns off
(a `[[skills.config]]` entry with `enabled = false` naming its `SKILL.md`) is left out, and a farther skill of the same
name is then offered in its place; with the home's `config.toml` unreadable, no skill is offered. Codex itself differs:
it loads a project's skills with or without trust, finds a `SKILL.md` at any depth under a root, names a skill by its
front matter, lists two skills of one name both, and reads the `[[skills.config]]` entries of a project's own
`config.toml`, which are not read here.

**Custom prompts** are the Markdown files directly in the Codex home's `prompts` directory, each named by its file.

### The Codex compatibility instructions

A "Codex compatibility" section is added to the consuming agent's role description (after it). It names the
user-level Codex configuration directory (the Codex home above) and the project-level configuration — the
`AGENTS.override.md` and `AGENTS.md` files from the repository root down to the paths the task touches, and the
`.codex/` directories on the way — and tells the agent:

- that Codex's MCP servers and skills have already been added, so it is not to set them up itself, and how to use
  Codex's custom prompts: as the commands the agent lists them as where it does (see "Codex compatibility for Claude
  Code" below), otherwise by reading a prompt's file when the user asks for it by name; a prompt's named `$KEY`
  placeholders are not filled in, so the agent asks the user for their values or takes them from the request;
- before planning or changing any file, to read what the task needs: the user-level `AGENTS.override.md`, else
  `AGENTS.md`, then each directory's `AGENTS.override.md`, else its `AGENTS.md`, from the repository root down to each
  path the task touches; the `rules/*.rules` files of the user-level and project `.codex/` directories; the hooks
  (`hooks.json` and the `[hooks]` tables of `config.toml`); the `[agents]` roles only when a rule asks for delegation;
  and the documentation the instruction files point to;
- to carry out Codex's configuration by intent with its own mechanisms: a rule that forbids a command prefix acts as a
  deny and one that prompts as an ask, both applying in full, and a blocking hook as a restriction, none of them lifted
  by agreement in the conversation; other hooks to be carried out by intent at the moment they would run; an `[agents]`
  role to map onto a subagent only when a Codex rule requires delegation; Codex's `approval_policy` and `sandbox_mode`
  not adopted, the session's own permission mode applying;
- not to modify `.codex/` or create migrated copies or project-level configuration of its own unless asked;
- that system, developer and user instructions and the agent's own instruction files outrank the section, except that
  a Codex-side restriction holds against a conflicting user instruction, and that between `AGENTS.md` files the nearer
  one wins and an `AGENTS.override.md` replaces the `AGENTS.md` beside it;
- to say in one sentence which kinds of Codex rules it adopted and carry on with the task.

## Codex compatibility for Claude Code

While Claude Code's Codex compatibility is on (see "Codex compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, gives Claude Code three things from what Codex offers (see "What Codex
offers" above); while it is off, it gives none of them.

**Codex's MCP servers** are added to the launch's `--mcp-config`, beside Octoboard's own server. A server given that way
would replace a server of the same name Claude Code defines itself, so a Codex server is also left out when Claude Code
defines its name: in the global config file the session's Claude Code reads (user and local scope; see "Claude Code" in
`docs/product/per-agent-launch-specifics.md` for which file that is) or in a `.mcp.json` from the working directory up,
approved and enabled or not. When one of those files exists but cannot be read or parsed, no Codex server is added. A
server named `octoboard`, or whose name holds anything but ASCII letters, digits, `-` and `_`, is left out too.

**Codex's skills and custom prompts** are given as a Claude Code plugin named `codex-compat`, a directory of the
session's own passed with `--plugin-dir`, and only when there is at least one skill or prompt to give. It holds a link
to each skill's directory, and a link to each prompt's file, or a copy of it on a host that makes no file links. Skills
are invoked as `codex-compat:<skill>` and prompts as `/codex-compat:<name>`, `<name>` being the prompt's file name
without `.md`. The plugin is not named `codex`, the name of the Claude Code plugin OpenAI ships.

**The Codex compatibility instructions** (see "The Codex compatibility instructions" above) are appended to the role
description Octoboard passes with `--append-system-prompt`.

## Codex compatibility for Grok Build

While Grok Build's Codex compatibility is on (see "Codex compatibility" in `docs/product/per-agent-settings.md`), every
launch, a resume and an account switch included, gives Grok Build Codex's MCP servers and skills in the session's copy
of Grok's `config.toml` (see "The per-session Grok home" in `docs/product/per-agent-launch-specifics.md`), and the Codex
compatibility instructions; while it is off, it gives none of them. What is added to the copy is never carried back to
the user's own `config.toml`, a linked one included. A copy that cannot be read or parsed is left as it is, and the
launch goes on without Codex's servers and skills; one that cannot be replaced is left as it was.

**Codex's MCP servers** become `[mcp_servers.<name>]` tables of the copy: `command`, `args` and `env` for a stdio
server, `url` and `headers` for a remote one. Grok expands `${NAME}` and `${NAME:-default}` in `args`, `env` values and
header values, so the references of "Codex's MCP servers" above are written as they are, and the
`OCTOBOARD_CODEX_MCP_<n>_<m>` variables go into the Grok process's environment. What may stay in the copy, which is
readable by its owner alone, is the literal text of a value that mixes text with a reference and the default of a
`${NAME:-default}`. A Codex server is also left out when:

- Grok defines its name itself: in the copy of the user's configuration, Octoboard's own `octoboard` server included,
  or in a project's `.grok/config.toml` from the working directory up to the repository root. When such a project file
  exists but cannot be read or parsed, no Codex server is added;
- Claude Code has a server of that name while Grok Build's Claude compatibility is also on, whether Octoboard gives
  Claude Code's servers (see "Claude compatibility for Grok Build" above) or Grok reads them itself: Claude Code's
  server keeps the name;
- its name holds anything but ASCII letters, digits, `-` and `_`.

**Codex's skills** are linked, one directory each, into a directory of the session's own, which is added to the copy's
`[skills] paths` after the paths the user's configuration already lists (the `[skills]` table is created where there is
none). The skills under an `.agents/skills` root, the project's and `~/.agents/skills`, are left out of it, since Grok
reads those roots itself. Codex's custom prompts are not given as commands, since Grok does not load commands from a
skills path; the instructions tell the agent to read a prompt's file when the user asks for it by name.

**The Codex compatibility instructions** are appended to the role description Octoboard passes with `--rules`. Grok
keeps the `--rules` a session was first started with, so turning the switch on or off changes a resumed session's MCP
servers and skills but not its instructions.
