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
steps within a project can also be delegated to an agent from another provider when needed.

Octoboard stores its application data locally and does not upload your code or conversations to servers operated
by its developer. Your configured agents and tools may send data to their providers, and collaboration may share
task information with multiple providers. See the [privacy policy](https://octoboard.dev/privacy/) for details.

The full introduction is on [octoboard.dev](https://octoboard.dev).

**Status: early development.** The macOS app, for Apple silicon and Intel Macs on macOS 11 or later, is still in
development. Only the first prototype features are in place, and the details are still being refined.

In the desktop app, the window, the daemon, and the projects share one machine. Linux is planned as a server
form: the daemon would run on a headless machine you own, with the interface opened in a browser on another
machine. There is no Linux desktop app planned. The server form is designed for one person accessing their own
daemon; multiple users sharing one daemon are outside this design. Server deployment and remote host support
are planned and are not available yet.

| Form | Status |
|---|---|
| macOS app (Apple silicon and Intel, macOS 11 or later) | Early prototype, still in development |
| Linux server — the daemon on a headless server, the interface in a browser on the network | Planned |
| Windows app | Planned |
| iOS app | Planned |
| Android app | Planned |

## Develop

You need macOS, Node.js with pnpm, a Rust toolchain, and the agent CLIs you want to use. Octoboard launches the agents
from your shell environment; those agents are yours, including one you point at a model you host yourself. How a launch
is configured is in [Launching agents](docs/product/launching-agents.md).

```sh
pnpm install                                   # Install the workspace (and, in the main checkout, the git hooks).
pnpm --filter @octoboard/desktop build:daemon  # Build the daemon sidecar once (and again after daemon changes).
pnpm --filter @octoboard/desktop tauri dev     # Start the UI dev server and open the desktop window on it.
```

Each part has its own entry doc with the rest of its first-time flow:

| Part | What it is | Entry doc |
|---|---|---|
| `apps/desktop/` | The macOS desktop application (Tauri 2 shell) | [apps/desktop/README.md](apps/desktop/README.md) |
| `apps/daemon/` | `octoboardd`, the Rust daemon that owns agent processes and data | [apps/daemon/README.md](apps/daemon/README.md) |
| `packages/ui/` | The React + HeroUI interface, also runnable in a browser | [packages/ui/README.md](packages/ui/README.md) |
| `apps/web/` | The public website at octoboard.dev (Nuxt) | [apps/web/README.md](apps/web/README.md) |

The git hooks format staged files on commit and run the affected checks on push; `pnpm verify --dry-run` shows which
checks a change selects and `pnpm verify` runs them. How the checks are scoped is in
[scripts/verify/README.md](scripts/verify/README.md).

Where to read next: the [documentation index](docs/README.md) answers which doc to open for which question, the
[project map](docs/project-map.md) gives the technology stack and directory map, and
[architecture](docs/architecture.md) explains why it is built this way.

## This is an AI-native project

Every line of code in this repository is written by AI. Humans set the direction, review the result, and decide
what gets built — they do not hand-write the code.

- **Pull requests are not accepted.** Any PR will be closed without review.
- **[Open an issue](https://github.com/baijunjie/octoboard/issues) instead.** Describe the bug, the missing
  capability, or the idea. Once an issue is accepted, an AI agent is assigned to implement it.

## License

[MIT](LICENSE)
