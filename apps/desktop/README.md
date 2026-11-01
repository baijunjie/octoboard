# Octoboard desktop application

The macOS desktop application: a Tauri 2 shell around the Octoboard interface. It opens the window and the native menu,
starts the `octoboardd` daemon as a sidecar, keeps running behind a menu bar icon when you choose that, and installs its
own updates. The interface itself is [`packages/ui/`](../../packages/ui/README.md), bundled into the application at
build time; it talks to the daemon ([`apps/daemon/`](../daemon/README.md)) only over the daemon's own protocol.

It runs on macOS 11 or later; the release build is one universal bundle for Apple silicon and Intel.

## Develop

This is the `@octoboard/desktop` package of the repository's pnpm workspace. Install from the repository root, then run
the scripts from this directory:

```sh
pnpm install          # From the repository root.
pnpm build:daemon     # Build the daemon sidecar; needed once, and again after daemon changes.
pnpm tauri dev        # Start the UI dev server (port 5174) and open the window on it.
```

Without the sidecar build, any build that includes the Tauri crate fails at compile time, saying the
`binaries/octoboardd-<target triple>` resource does not exist. The Rust tests of both crates run from the repository
root with `pnpm test:rust`, which needs no sidecar.

## Build and install

- `pnpm build:app` — builds the application without signing, to check a change.
- `pnpm install:macos` — builds, signs and installs the application locally. Put `APPLE_SIGNING_IDENTITY` (a signing
  certificate in your keychain) in the gitignored repository-root `.env.secret`, copy
  [`.env.local.example`](../../.env.local.example) to `.env.local` and set `OCTOBOARD_INSTALL_DIR` to an absolute
  directory, quit any running Octoboard, then run it. It needs Xcode 26 selected as the developer directory.
- `pnpm release` and `pnpm publish-release` — the distributable build and its publication for in-app updates.

How each script works, the release signing modes and the tag-driven release are in
[`scripts/README.md`](scripts/README.md).

## Read next

- [What the application does from launch to quit](../../docs/product/application-lifecycle.md) and [in-app
  updates](../../docs/product/in-app-updates.md)
- [The shell's structure: directories, IPC surface and where new code goes](../../docs/structure/desktop.md)
- The [documentation index](../../docs/README.md)
