# Agent CLI versions and updates

Octoboard installs no agent: the user installs each agent's CLI. For an added agent whose CLI is installed, Octoboard
shows the version of the CLI that starts a session now, how it was installed and the latest version published for that
install, and updates the CLI when the user asks. All of it is shown in the agent's **Agent version** dialog, opened from
the **Version** entry of the agent's menu in Settings' Agent accounts section, and an available update is marked on
the agent's actions button and that entry (see "The agent's menu" and "The agent version dialog" in `docs/product/agent-accounts.md`).

## When versions are checked

Octoboard checks an agent's CLI only when a window asks: when Settings' Agent accounts section is opened, and again
each time the window receives a fresh snapshot from the daemon while the section is open, as after a reconnection.
Nothing is checked on a timer, and an update is never run unless the user asks for one. A window's request covers every
agent whose CLI is not known to be missing. An agent being checked or updated, or checked or updated within the last
**30 seconds**, is left alone, however many windows ask.

A check reads the user's login shell environment, as a launch does. When that environment cannot be read, the check
shows nothing new, and the next request tries again. What a check found is kept in the daemon's memory only, shared by
every window and never stored: a daemon started afresh knows nothing until a window asks again. A check that began
before an update of the same agent started is discarded, so it never shows the versions from before the update.

## How the install method is told

The install method is told from where the agent's binary, found on the login shell's `PATH`, lies once its links are
followed. It decides both the command that updates the CLI and where the latest version is read from:

| Install method | The binary lies | Updated by | Latest version read from |
|---|---|---|---|
| Its own installer | Claude Code: `~/.local/share/claude/versions` (on Windows `~/.local/bin/claude.exe`); Codex: `~/.codex/packages/standalone`; Grok Build: `~/.grok` | `<cli> update`, run with the CLI's own binary | Grok Build: `grok update --check --json`; Claude Code and Codex: the npm registry |
| npm | Under `node_modules/@anthropic-ai/claude-code` (Claude Code) or `node_modules/@openai/codex` (Codex) of any prefix | `npm install -g <package>@latest`, given the install's prefix where one can be told, using the `npm` on the shell's `PATH` | The npm registry |
| Homebrew | Under Homebrew's `Cellar` or `Caskroom`, in the cask `claude-code` (Claude Code), or the formula or cask `codex` (Codex) | `brew upgrade <name>`, with `--cask` for a cask, using the `brew` of the install's own Homebrew | Homebrew's own record of the formula or cask (`brew info`), as its metadata stands |
| Unknown | Anywhere else, or where nothing can be told | Not updated | Not looked up |

A path through pnpm's, Bun's or Yarn's directories is an unknown install, though it lies under `node_modules`. Grok
Build is recognised only from its own installer.

## Where the latest version comes from

Only the npm registry lookup reaches the network, over HTTPS, at `https://registry.npmjs.org/<package>/latest`, with a
time limit of **20 seconds**; `grok update --check` and `brew info` are given the same limit. The lookup goes out as the
user's own network does:

- through the proxy the login shell environment names, `HTTPS_PROXY`, `ALL_PROXY` or `HTTP_PROXY` (in upper or lower
  case, the first set in that order), with `NO_PROXY` exempting a host; HTTP proxies and SOCKS proxies (`socks://`,
  `socks4://`, `socks4a://`, `socks5://`, `socks5h://`) both work;
- trusting the system's certificate store, so a corporate certificate authority that inspects TLS is accepted;
- always to the public registry: a custom registry set in an `.npmrc` is not read.

The lookup is best-effort. When it fails, or the source answers with no version that can be read, no latest version is
shown and the reason is a warning mark on the Version row (see "The Version row" below); no error alert is raised and
nothing is blocked. An unknown install has neither a latest version nor a failure.

## When an update is available

An update is available when the latest version is newer than the installed one and the install method is not unknown.
Versions are compared by their first three numbers, `major.minor.patch`, wherever they stand in the text (`2.1.319`,
`v0.5.1`, `rust-v0.5.1`); anything after them is ignored. When either version has no such numbers, nothing is said about
whether the CLI is current.

## The Version row

The Agent version dialog holds one row, **Version**, with a "?" help tip ("About updating") saying that updating does
not stop running sessions: they keep the version they started with, and new sessions use the new one. The row's text
says, in this order of precedence:

| State | The row says |
|---|---|
| No check has reported yet | That the version is being checked, and nothing else. |
| An update running | That it is updating. |
| The installed version could not be read | That the installed version could not be read. |
| An update available, or a newer latest version under an unknown install | The installed version and that the latest one is available. |
| The latest version known and not newer | The installed version, and that it is up to date. |
| Otherwise (no latest version, or versions that cannot be compared) | The installed version alone. |

A line under it, in smaller text, names the install method: its own installer, npm or Homebrew. For an unknown install
it says instead that Octoboard cannot tell how the CLI was installed, and to update it the way it was installed.

**Update**, at the row's end, is offered when the install method is known, the installed version was read and the CLI
is not known to be current: the primary button when an update is available, a secondary one when whether the CLI is
current cannot be told (the latest version could not be looked up, say). There is no Update for an unknown install or a
CLI that is up to date. Pressing it starts the update at once, with no confirmation. The button then shows as pending
until the update ends — including when the dialog is closed and opened again, and in every window while an update
started from any of them runs.

**A warning mark** stands before the Update button when the latest version could not be looked up, and when the last
update failed or did not finish; the update's mark stays until the CLI is known to be current or the next update starts.
Both at once are one mark, its popover saying each reason in a paragraph of its own with the raw text under it (the
network error, the line the update printed), and without the remedy line the error alert carries (see "How an update
ends" below). How a mark opens is in "Failures, warnings and notes marked at their point" in
`docs/product/error-alerts.md`.

When an update this view saw running ends, assistive technology is told: a success as the version updated to (or that
the update finished, when the new version cannot be read), and a failure of an update another window started by its
reason. When the Update button or the mark goes while it holds keyboard focus, focus moves to the help tip.

## Updating

An update runs in the background with the command the install method names, under the login shell's environment, for
at most **10 minutes**; one still running then is stopped, and what it began may be left half done. Sessions of the
agent that are running keep their process and so their version; sessions started afterwards use the new one. Every
window shows the update as running while it runs.

The command's output, standard output and error together, is written to `~/.octoboard/agent-cli-updates/<agent>.log`,
named by the agent's id (`claude.log`, `codex.log`, `grok.log`), overwritten by each update of that agent and kept
across restarts; on macOS and Linux only the user can read it. A stop the daemon is asked for (`octoboardd stop`,
`POST /shutdown`, the shutdown request, a planned stop) waits for a running update, up to its own deadline: the ten
minutes and the updater's 6 s grace; a planned restart is refused while an update runs, since it would cut the update
off. A stop a service manager makes (`systemctl stop` or `restart`, the host shutting down) is cut at the unit's
`TimeoutStopSec` (90 s), after which the update is killed and may be left half installed; outside a unit the update's
tool outlives the daemon. A daemon started afterwards knows nothing of that update: it shows none running and accepts another,
and the earlier update's result shows only in the next check. Once the daemon has begun to shut down, no update is
started (see "Quitting" in `docs/product/application-lifecycle.md`).

When an update ends, Octoboard reads the login shell environment again, judges again whether the agent is installed
(an agent that no longer is shows as not installed, as in "The agent strip" in `docs/product/agent-accounts.md`), and
checks the CLI afresh, so the outcome comes with the new installed and latest versions. An update that ends after the
daemon began to shut down only records its outcome.

## How an update ends

- **Succeeded**: the row shows the new versions; no alert.
- **Failed**: the command could not be run, ended with a failure or printed an error. Its failure line is the line of
  its output that says what went wrong, trimmed and cut to 300 characters: for npm, the first line of npm's last error
  block that carries something (its code line, such as `npm error code EACCES`); for another tool, the last line
  starting `Error:` or `error:`, as Homebrew's; otherwise the last non-empty line.
- **Did not finish**: it was stopped at the ten-minute limit.

**In the window that asked for the update**, a failed or unfinished update raises an error alert, even when the Agent
version dialog, or Settings, has been closed since. Its title is plain, "Could not update the agent". Its body names
the agent and says the update failed, with a line of its own saying to fix the problem shown in the details or update
the CLI the way it was installed, and the failure line under **Error details**; for one that did not finish, it says the
update did not finish within ten minutes and was stopped and that the install may be partly updated, with a line of its
own saying to check the install and try again, and no Error details. The mark at the row stays as the lasting state.
**Every other window**, and a window that finds the failure when the dialog opens, shows the mark alone.

**A refused update** raises an error alert with the same title, saying why, and the Update button is offered again;
the two refusals particular to updates say it in their own sentences, with no remedy line. It is refused when the install is unknown or the `npm` or `brew` it needs is not
on the shell's `PATH` (`agent_cli_update_unsupported`: Octoboard cannot update the agent, and to update it the way it
was installed), when the agent's CLI is being updated already (`agent_cli_update_running`), when its binary is no longer
found (`agent_not_installed`), when the agent is not added (`agent_not_added`), when the login shell environment could
not be read in time (`shell_environment_timeout`), when the agent's provider knows no way to update its CLI
(`provider_unsupported`) and while the daemon shuts down (`daemon_shutting_down`).

## For programs on the daemon's host

A client asks for a check with the protocol request `check_agent_clis`, optionally naming one agent, and for an update
with `update_agent_cli`, naming the agent; both are answered at once, and each result follows as the event
`agent_cli_status_updated`, which carries the agent's whole status: the installed version, the install method
(`native`, `npm`, `homebrew` or `unknown`), the latest version or the reason it is missing (`agent_cli_latest_failed`
with its text, or `agent_cli_latest_unreadable`), when the check ended, and the last update (`running`, `succeeded` or
`failed`, with `agent_cli_update_failed` and its failure line, or `agent_cli_update_timed_out`). A snapshot replays every
status the daemon holds. When an update changes whether the agent is installed, `agent_availability_updated` follows.
