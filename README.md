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

Run these commands from the repository root after installing the workspace:

```sh
pnpm format       # Apply ESLint layout fixes and format both Rust crates.
pnpm lint         # Check JavaScript, TypeScript, TSX and Vue; warnings fail the check.
pnpm format:check # Check ESLint rules and Rust formatting without changing files.
pnpm typecheck
```

ESLint checks source code and scripts across the workspace. ESLint Stylistic uses two-space indentation,
double quotes and semicolons; Vue template rules handle the website's single-file components. Rust formatting
uses the root `rustfmt.toml`; CI runs rustfmt from Rust 1.86.0. Generated files, dependencies and build output
are excluded. CSS, JSON, Markdown and other file types are outside the ESLint formatting scope.

Use `pnpm lint:fix` separately to apply available lint fixes, then inspect the changes and run the relevant
tests. For routine edits, format only the files or Rust packages involved in the task:

```sh
pnpm exec eslint path/to/file.tsx --fix --fix-type layout
cargo fmt -p octoboardd
```

The Code quality workflow checks ESLint and Rust formatting on pushes to `main` and pull requests. Formatting
does not replace type checking, tests or code review.

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
