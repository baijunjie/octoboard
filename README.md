# Octoboard

<p align="center">
  <img src="assets/branding/logo-brand-h.png" alt="Octoboard — The Programming Terminator" width="640" />
</p>

<p align="center">Programming ends here. Power to rival the Terminator.</p>

Octoboard is a local, open source command center for coding agents. It runs on your own machine and is built
for products spanning multiple projects or repositories, each with its own development rules and tools.

A feature can touch server-side services, administration tools, merchant operations tools, staff operations
tools, and customer-facing apps. Developing them all in one agent session mixes their contexts and can miss
project-specific instructions. Separate sessions preserve that context, but leave you relaying tasks and results.
Octoboard brings two goals together:

- **Agent management across projects.** A coordinating agent breaks down requests, assigns tasks, tracks
  dependencies, and brings project reports together.
- **Quality in every project.** Each project runs an independent native session in its own directory, keeping
  its instructions, Skills, hooks, and tools. Open any conversation to review the work, add context, or steer it.

Choose **Claude Code**, **Codex**, or **Grok Build** independently for the coordinator and each project. Each
keeps its own toolchain, while Octoboard passes tasks, relevant context, and reports between sessions. Individual
steps within a project can also be delegated to an agent from another brand when needed.

Octoboard stores its application data locally and does not upload your code or conversations to servers operated
by its developer. Your configured agents and tools may send data to their providers, and collaboration may share
task information with multiple providers. See the [privacy policy](https://octoboard.dev/privacy/) for details.

The full introduction is on [octoboard.dev](https://octoboard.dev).

**Status: early development.** The macOS app for Apple Silicon is still in development. Only the first
prototype features are in place, and the details are still being refined.

In the desktop app, the window, the daemon, and the projects share one machine. Linux is planned as a server
form: the daemon would run on a headless machine you own, with the interface opened in a browser on another
machine. There is no Linux desktop app planned. The server form is designed for one person accessing their own
daemon; multiple users sharing one daemon are outside this design. Server deployment and remote host support
are planned and are not available yet.

| Form | Status |
|---|---|
| macOS app (Apple Silicon) | Early prototype, still in development |
| Linux server — the daemon on a headless server, the interface in a browser on the network | Planned |
| Windows app | Planned |
| iOS app | Planned |
| Android app | Planned |

## Develop

Install the agent CLIs you want to use. Octoboard launches them from your shell environment. Those agents are
yours, including one you point at a model you host yourself. How a launch is configured is in
[Launching agents](docs/product/launching-agents.md).

Build and run the desktop app from the
[desktop development guide](apps/desktop/README.md#development): install the workspace, build the daemon sidecar,
then start Tauri.

### Code quality

Install the workspace; in the main working copy with the main branch (the remote's default branch) checked out,
this also installs the repository hooks (staged formatting on commit, verification on push). CI, linked
worktrees and other checkouts skip it, as does a clone whose `core.hooksPath` hands hooks to another hook system,
which then calls `sh .githooks/hook.sh <pre-commit|pre-push> "$@"`; a hook that cannot be installed is reported as
a warning. After hook changes land in the main branch, run `pnpm install` there again to refresh them;
`pnpm hooks:install` installs them explicitly. The revert gate is not one of these hooks: it is Octoboard's
built-in gate ([revert gate](docs/product/revert-gate.md)), which applies when its `revert_gate` git workflow
setting is on (off by default) and only to git run inside Octoboard project sessions:

```sh
pnpm install
```

Before each commit, the hook formats only staged JavaScript, TypeScript, TSX, Vue and Rust files. It formats
staged content and preserves separate unstaged edits; conflicts stop the commit without changing either version.
CSS, JSON, Markdown and other file types are outside this formatting scope. ESLint fixes layout only;
non-formatting lint, type checks, builds and tests run before pushing, not after routine development or local merges.

Before each push, the hook runs read-only checks for every affected package, using the destination's advertised
remote commit and the exact commit being pushed. This includes all unpushed local merges and commits. A new branch
is measured from its merge base with the live remote target branch (or remote HEAD); an unavailable baseline
selects the full set. Rust changes run the Rust checks of both crates; tag pushes are not verified (release CI
checks the tag); a checkout that predates the hook scripts skips that stage with a warning. Multiple affected
packages are combined: an ordinary UI change does not run Rust or website checks. Shared protocols, imported code,
configuration and dependency inputs also select their consumers; unknown inputs conservatively select everything.
Checks require the pushed commit checked out in a clean working tree, so a different checkout cannot certify it.

Manual entry points:

```sh
pnpm format:staged          # The same staged formatter the commit hook runs.
pnpm verify --dry-run      # Show checks for changes from the freshly fetched upstream/remote target.
pnpm verify                # Run those checks without rewriting files.
pnpm verify --base <ref>   # Use an explicit remote baseline, including non-ancestor force-push tips.
pnpm verify --full         # Check every package.
pnpm verify --release      # Check the shipping application, without website or gallery-only suites.
```

Verification stops at the first failure and reuses successful checks for identical content and toolchain versions;
`--fresh` forces them to run again. UI tests and Rust tests queue for a machine-wide slot across worktrees. Website
checks build the site before its SEO tests read the generated output. Build configuration changes also select the
relevant UI build. Broad manual commands (`pnpm format`, `pnpm format:check`, `pnpm typecheck`) remain available,
but are not routine development steps.

The Code quality workflow runs application lint, production UI type checks, application UI tests, desktop script
tests, Rust formatting and both Rust crates' tests unconditionally before a release and on request. It excludes
website checks and standalone gallery/website-preview tests; application tests may still use their shared fixtures.
The release job then builds, signs and notarizes the application. Formatting does not replace code review.

The desktop shell is [Tauri 2](https://github.com/tauri-apps/tauri) around a
[React](https://github.com/facebook/react) and [HeroUI](https://github.com/heroui-inc/heroui) interface. A
[Rust](https://github.com/rust-lang/rust) daemon, `octoboardd`, owns agent processes and application data.
Terminals render with [xterm.js](https://github.com/xtermjs/xterm.js). The UI talks to the daemon over
WebSocket/HTTP only. Where to read next: [project map](docs/project-map.md),
[architecture](docs/architecture.md), [documentation index](docs/README.md).

## This is an AI-native project

Every line of code in this repository is written by AI. Humans set the direction, review the result, and decide
what gets built — they do not hand-write the code.

- **Pull requests are not accepted.** Any PR will be closed without review.
- **[Open an issue](https://github.com/baijunjie/octoboard/issues) instead.** Describe the bug, the missing
  capability, or the idea. Once an issue is accepted, an AI agent is assigned to implement it.

## License

[MIT](LICENSE)
