# Desktop build, install and release scripts

The Node scripts of `@octoboard/desktop` that generate native resources, build the sidecar and the app, install it
locally, prepare version tags, produce the signed release and publish it for the in-app updates. They run through the
`apps/desktop/package.json` scripts, directly in that directory or with `pnpm --filter @octoboard/desktop` from the
repository root; the shared build pieces are in `bundle.mjs`. Credentials come from the
gitignored repo-root `.env.secret` (template `.env.secret.example`), read by `scripts/lib/env.mjs`; a variable already
in the environment wins.

## External interfaces

- `native-localizations.mjs` (`pnpm native:generate`) — generates the native permission resources from
  every `src-tauri/localizations/*.json` catalog and `src-tauri/Info.template.plist`, substituting the app name from
  `config/app.json`. Tauri runs it before development and builds; `src-tauri/build.rs` runs the same generator for
  Cargo-only builds. The ignored `src-tauri/Info.plist` supplies both Tauri development embedding and
  `bundle.macOS.infoPlist`; the ignored `src-tauri/generated-localizations/` holds the `.lproj/InfoPlist.strings` files
  mapped into bundle resources by `src-tauri/tauri.conf.json`. Its exported `generateNativeLocalizations` is tested by
  `native-localizations.test.mjs`.
- `build-daemon.mjs` (`pnpm build:daemon`) — builds `apps/daemon/` in release mode and copies it into
  `src-tauri/binaries/` as `octoboardd-<target triple>`, the name Tauri's `externalBin` needs. Tauri runs it as
  `beforeBuildCommand` (through `pnpm build:tauri`); `tauri dev` does not, so it must be run by hand after a daemon
  change. With `TAURI_ENV_TARGET_TRIPLE=universal-apple-darwin` or `--universal` it builds both architectures `--locked`
  and `lipo`-joins `octoboardd-universal-apple-darwin`. `pnpm build:daemon --universal` does not add the two Rust
  targets (`pnpm release` does), so on a machine without them run
  `rustup target add aarch64-apple-darwin x86_64-apple-darwin` first.
- `build-app.mjs` (`pnpm build:app`) — `tauri build` for local verification with every `APPLE_*` variable and the update
  signing key removed and the update archive turned off; fails if the `.app` carries a Developer ID authority or
  predates the build. Extra arguments go to `tauri build`, except ones that move the output.
- `install-app.mjs` (`pnpm install:macos`) — builds the `.app` signed with `APPLE_SIGNING_IDENTITY` (never notarized, no
  update key) and installs it into `OCTOBOARD_INSTALL_DIR` from `.env.local`. It verifies the built signature, refuses
  while Octoboard runs, refuses to replace a symlink or another application, stages the copy on the destination volume
  and keeps the previous bundle for rollback if the replacement fails.
- `release.mjs` (`pnpm release`) — the universal, signed and notarized release build (below).
- `release-desktop.mjs` (`pnpm release:version`) — prepares a version commit in an isolated worktree, checks it and
  pushes it with its annotated tag to start release CI (see "Releasing by tag"). Its version-file preparation,
  argument parsing and release orchestration are tested by `release-desktop.test.mjs`.
- `sign-update.mjs` (`pnpm sign-update`) — signs the update archive `pnpm release --no-sign-update` left with the update
  key and checks that a fresh `.sig` came out (below).
- `publish-release.mjs` (`pnpm publish-release`) — publishes what `pnpm release` built (below).
- `release-manifest.mjs` — what publishing decides without GitHub (which manifests a version goes to, asset names and
  addresses, version ordering, the manifest itself); tested by `release-manifest.test.mjs` through `pnpm test`.
- `bundle.mjs` — shared by the build scripts: paths, `tauriBuild`, `withoutAppleVariables`, `withoutUpdateKey`, the
  bundle lookup and the checks on a built `.app` (`assertLayeredIcon`, `assertProducedSince`, `appVersion`,
  `verifySignature`, `signatureReport`).
- `update-archive.mjs` — the update archive's lifecycle: `createUpdateArchive` packs the `.app` with the system `tar`
  without xattrs and refuses entries other than files, directories and symlinks (a hard link breaks the updater's
  unpack); `signUpdateArchive` signs it with `tauri signer sign`; `removeUpdateArchives` clears earlier builds'.
  Tested by `update-archive.test.mjs`.
- `actool/actool` — a shim for Xcode's `actool` put first on `PATH` by `tauriBuild`: the Node-hosted Tauri CLI starts
  `actool` with stdin closed, which crashes it, and the shim runs it with stdin on `/dev/null`. Temporary, to be removed
  once the upstream fix ships.

Building the layered app icon needs Xcode 26's `actool` as the selected developer directory (`xcode-select` or
`DEVELOPER_DIR`); `install-app.mjs` and `release.mjs` fail when the `.app` lacks `Assets.car` and `CFBundleIconName`,
`build-app.mjs` only warns.

## Release builds

`pnpm release` runs `tauri build --target universal-apple-darwin --locked` (which runs `build-daemon.mjs` and builds the
UI first, through `beforeBuildCommand`), producing one universal `.app` and `.dmg` under
`target/universal-apple-darwin/release/bundle`, then makes the update archive from the `.app` and signs it (below), and
verifies the result. It adds the two Rust targets with
`rustup target add` first (doing nothing when present); a `Cargo.lock` that no longer matches the manifests fails the
first compile instead of being rewritten. The script checks the environment before spending the build, in three modes:

- **Nothing set** — an unsigned build, with the signing-dependent verification skipped. It makes no update archive and
  needs no update key.
- **`APPLE_SIGNING_IDENTITY` (a "Developer ID Application" identity) plus `APPLE_API_KEY`, `APPLE_API_ISSUER` and
  `APPLE_API_KEY_PATH`** — signed, notarized and stapled, and, with the update key below, an update archive too.
- **`APPLE_SIGNING_IDENTITY` without the full API-key trio** — refused. Notarization uses an App Store Connect API key
  only: `APPLE_ID` or `APPLE_PASSWORD` present in the environment is refused, because a second credential set could
  notarize the `.app` and the `.dmg` under different identities.

A signed build also needs the update signing key, `TAURI_SIGNING_PRIVATE_KEY` (the key file's path or the key itself;
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for a key made with a password), and is refused without it. It is a different key
from Apple's: it signs `Octoboard.app.tar.gz`, the archive the in-app update downloads, and the application checks that
signature against `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` before it installs anything. The key pair
comes from `pnpm --filter @octoboard/desktop tauri signer generate -w <path>` (an empty password is allowed); the public
half is the file's `.pub`. Never commit the private key and keep a backup: with the key lost, no installed copy can ever
be updated again, and with it leaked, anyone can ship an update to every one of them. So `tauri build` never holds it:
`createUpdaterArtifacts` is off in `tauri.conf.json` (it would make the build sign the archive and fail without the key)
and the scripts drop the key variables from the build's environment, so a build script that reads the environment cannot
get the key. After the build `release.mjs` packs the `.app` into `Octoboard.app.tar.gz` itself and signs that with
`tauri signer sign`, as the one step that reads the key. `pnpm release --no-sign-update` stops after packing, for a
caller that signs in a step of its own with `pnpm sign-update` (the release workflow does) and so does not need the key
until then. The script first deletes the update archives an earlier build left (their names carry no version), then
checks that the archive and, unless signing was left out, its `.sig` exist next to the `.app`.

Verification after the build: the `.app` has the layered icon and the nested `octoboardd` sidecar's signature is
reported; for a signed build, `codesign --verify --deep --strict` and Gatekeeper's verdict on the `.app`, then the
`.dmg` is notarized and stapled on its own (the Tauri CLI notarizes only the `.app`, so a downloaded `.dmg` would
otherwise be refused), `stapler validate` and Gatekeeper's verdict on the `.dmg`. These `spctl` verdicts are on files
this machine just built, which carry no quarantine attribute, so they prove little about a download; a real Gatekeeper
test needs a copy that was quarantined on its way.

## Publishing a release

`pnpm --filter @octoboard/desktop publish-release (--notes "text" | --notes-file path) [--dry-run]` publishes what the
last signed `pnpm release` built, for the version in `apps/desktop/package.json`. The manifest model it writes to (the
fixed-tag `updater` release, the channels, the manifest contents, asset naming, a channel never moving backwards) is
described in `docs/product/in-app-updates.md`, section "Where updates come from"; this section is the procedure.

1. It refuses unless `gh` is logged in, the working tree is clean and the universal build in the bundle directory is
   this version's, signed and notarized: the `.app` says it is `<version>`, the `.dmg` is named for it and passes
   `stapler validate`, and the update archive and its signature are no older than the `.app`. A release
   `desktop-v<version>` that already exists with exactly this build's assets, the same archive size and the same
   signature is an earlier run that failed part-way, and this run resumes from step 3; any other (other assets, a
   rebuild of the same version, a draft) is refused, since a published version is never replaced: raise the version
   instead.
2. It creates the release `desktop-v<version>` on the current commit (which must be pushed), marked prerelease when the
   version has a suffix, with the `.dmg` and the update archive and its signature.
3. It creates the `updater` release if it is missing (as a prerelease, so it is never the repository's latest), then
   uploads the manifests the version goes to with `--clobber`. A manifest that already announces a newer version is left
   alone and reported. A manifest that is not valid is refused, with nothing created yet. Immutable releases must stay
   off for the repository: the `updater` release is rewritten by every release, and the script refuses when it is
   immutable (it reads the `isImmutable` field, which needs a `gh` recent enough to report it).

`--dry-run` prints the plan and the manifest without calling GitHub.

## Releasing by tag

Pushing a tag `desktop-v<version>` runs `.github/workflows/release.yml`, which does the two steps above on a macOS
runner after the application checks (`.github/workflows/quality.yml`) pass; the manual `pnpm release` and
`publish-release` path stays available.

### Prepare and push a version

Run from the clean main checkout on `main`, with dependencies installed and Git access to `origin`:

```sh
pnpm --filter @octoboard/desktop release:version 0.1.0-beta.2 --notes-file /tmp/octoboard-release-notes.md --dry-run
pnpm --filter @octoboard/desktop release:version 0.1.0-beta.2 --notes-file /tmp/octoboard-release-notes.md
```

Supply exactly one nonempty notes source: `--notes "text"` or `--notes-file path`. A relative notes path is resolved
from the repository root, even when pnpm runs the command in the desktop package; absolute paths work too. Keep an
uncommitted notes file outside the tracked tree or in an ignored location so the main checkout stays clean. The tag
message is the same text shown on the release page and in the application's update notice; Markdown headings are
preserved.

The version must be a semantic version strictly newer than the current one, without build metadata. The command
refuses an existing local or remote version tag, release branch or release worktree. `.worktrees/` must be ignored
(for example in `.git/info/exclude`). `--dry-run` checks the inputs and queries the remote refs, then prints the plan
and notes without changing files or refs; it does not fetch or run the checks.

The command:

1. Fetches `origin/main`, fast-forwards a behind-only local `main`, and refuses divergent history. It creates
   `release/desktop-v<version>` in `.worktrees/desktop-v<version>` from main.
2. Updates `apps/desktop/package.json`, the root `Cargo.toml` and only the two workspace package versions in
   `Cargo.lock`. `cargo metadata --locked` validates that lockfile without updating external dependencies. It installs
   dependencies with `pnpm install --frozen-lockfile`, stages exactly the three version files and makes one version
   commit through the normal Git hooks.
3. Rebases onto main, runs `pnpm verify --quick`, fast-forwards main and removes the release worktree and branch. It
   then runs the full checks selected by the changes since the fetched remote main, including other unpushed main
   commits. It stops if main changes while preparation or verification is running.
4. Creates the annotated version tag on the checked commit and atomically pushes that exact commit to remote main
   together with its tag. Git hooks stay enabled, and a refused push updates neither remote ref.

Successful command completion means the tag was pushed, not that the application has been published. Follow the
**Release** workflow in GitHub Actions through its checks, build, signing, notarization and publishing steps before
announcing the release.

Failures leave the current state for inspection; the command never resets history or force-removes a worktree. Before
the merge, inspect the named release branch and worktree, correct the cause, and finish or clean up that attempt before
starting another. After the merge, the version commit stays on main; the same `release:version` command cannot be
rerun because the version must increase. Correct the failure, check main with `pnpm verify --base origin/main --head
<release-commit>`, create the annotated tag only if it is absent, and atomically push the checked commit and tag after
confirming any existing local tag has the intended commit and notes. Do not delete or replace a published tag to retry.

### Release CI

The workflow:

1. `validate` runs `.github/release-notes.sh`: the tag must equal `desktop-v<version>` from `apps/desktop/package.json`
   and be an annotated tag with a non-empty message. The message, minus any signature, is the release notes, which the
   in-app update notice shows too. It also checks that the tagged commit is on `main`, that the repository secrets
   below are set, and that `Cargo.lock` matches the manifests (`cargo metadata --locked`), which the `--locked` build
   would otherwise find only after the checks.
2. `checks` runs `quality.yml`; then `release` builds, signs and notarizes with
   `pnpm --filter @octoboard/desktop release --no-sign-update`, signs the update archive in a step of its own
   (`sign-update`), the only step given the update key (no build step has it in its environment; code planted for
   later steps of the same runner could still read it), and publishes with `publish-release --notes-file`, the only one
   given the workflow's own token (for `gh`; checkouts keep no credential in `.git/config`). The signing certificate and
   the Developer ID G2 intermediate go into a keychain that exists for that run only, and the API key is written to a
   temporary file; both are removed right after the build. `validate` learns whether the secrets are set from boolean
   expressions, without their values reaching a step.
3. The whole workflow is in one concurrency group, so releases run one at a time. GitHub keeps one pending run, so a
   third tag pushed while one runs replaces the waiting one, which must then be re-run.

Repository secrets: `APPLE_CERTIFICATE` (base64 of the Developer ID Application `.p12`, private key included),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_PRIVATE_KEY`
(the contents of the API key's `.p8`), `TAURI_SIGNING_PRIVATE_KEY` (the key's contents) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (not set for a key without a password).

**The version is kept in two places**: `version` in `apps/desktop/package.json`, which `src-tauri/tauri.conf.json` reads
(its `version` is the path `../package.json`) and the release scripts read, and the workspace `version` in the root
`Cargo.toml`, which both crates take (`version.workspace = true`) and the daemon reports in the configuration export and
to the agent CLIs. `src-tauri/build.rs` fails every build and test of the shell when the two differ, when
`tauri.conf.json` names a version of its own, or when the daemon's manifest does not take the workspace version.

For manual tag preparation, raise the version in `apps/desktop/package.json` and in the root `Cargo.toml`, run
`cargo check` so that `Cargo.lock` records it, check and push that commit to `main`, write the notes to a file, then
`git tag -a desktop-v<version> --cleanup=whitespace -F <notes-file>` and `git push origin desktop-v<version>`.
`--cleanup=whitespace` keeps the notes' Markdown heading lines, which the default `--cleanup=strip` drops; the file
rather than the editor keeps Git's `#` template lines out of them. Push tags one at a time: GitHub creates no push
event when more than three tags are pushed at once.

When a run fails after `gh release create` but before the manifests are uploaded, re-running it rebuilds with a
different signature, and `publish-release` refuses it. Delete the release but keep the tag
(`gh release delete desktop-v<version>`, without `--cleanup-tag`) and re-run the failed jobs. The script's own messages
still describe the manual flow.
