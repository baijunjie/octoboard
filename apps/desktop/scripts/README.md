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
  Cargo-only builds on macOS (other targets list no such resources). The ignored `src-tauri/Info.plist` supplies both Tauri development embedding and
  `bundle.macOS.infoPlist` in `tauri.macos.conf.json`; the ignored `src-tauri/generated-localizations/` holds the `.lproj/InfoPlist.strings` files
  mapped into bundle resources by `src-tauri/tauri.macos.conf.json`. Its exported `generateNativeLocalizations` is tested by
  `native-localizations.test.mjs`.
- `build-daemon.mjs` (`pnpm build:daemon`) — builds `apps/daemon/` in release mode and copies it into
  `src-tauri/binaries/` as `octoboardd-<target triple>` (with `.exe` for a Windows target, which Tauri's
  `beforeBuildCommand` announces through `TAURI_ENV_TARGET_TRIPLE`), the name Tauri's `externalBin` needs. Tauri runs it as
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
- `release.mjs` (`pnpm release`) — picks the host's release build: `release-macos.mjs`, the universal, signed and
  notarized macOS build, or `release-windows.mjs`, the x64 NSIS installer with its signing modes (below).
- `release-desktop.mjs` (`pnpm release:version`) — prepares a version commit in an isolated worktree, checks it and
  pushes it with its annotated tag to start release CI (see "Releasing by tag"); `--platforms` makes a version for one
  platform. Its version-file preparation, argument parsing and release orchestration are tested by
  `release-desktop.test.mjs`.
- `tag-message.mjs` — prints the notes or the platforms of a tag message read from standard input, for
  `.github/release-notes.sh`.
- `sign-update.mjs` (`pnpm sign-update`) — signs the update artifact `pnpm release --no-sign-update` left (the archive on
  macOS, the installer on Windows) with the update key and checks that a fresh `.sig` came out (below).
- `publish-release.mjs` (`pnpm publish-release`) — publishes what `pnpm release` built, for both platforms or the one
  `--platforms` selects: the universal macOS build from the bundle directory, the Windows installer from a directory
  (below).
- `release-manifest.mjs` — what publishing decides without GitHub (the platform selection and the tag message's
  `Platforms:` line, which manifests a version goes to for each platform, asset names and addresses, what an existing
  release needs, version ordering, the manifest itself and its platform entries); tested by `release-manifest.test.mjs` through
  `pnpm test`.
- `windows-release.mjs` — the Windows release's pieces: the signing mode and stages, the `--config` that makes
  `tauri build` call the sign command, the `signtool` checks, the look inside the installer and the check of the
  files `publish-release` takes. `sign-file.mjs` is what `tauri build` calls per file in `command` mode. Tested by
  `windows-release.test.mjs`, which covers what needs no `signtool` or 7-Zip.
- `bundle.mjs` — shared by the build scripts: paths (macOS's universal bundle directory and Windows's), `tauriBuild`
  and `runTauri` (the CLI is run as `node tauri.js`, which also works on Windows), `withoutAppleVariables`,
  `withoutUpdateKey`, the
  bundle lookup and the checks on a built `.app` (`assertLayeredIcon`, `assertProducedSince`, `appVersion`,
  `verifySignature`, `signatureReport`).
- `update-archive.mjs` — the update archive's lifecycle (`signUpdateArchive` also signs the Windows installer): `createUpdateArchive` packs the `.app` with the system `tar`
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

`tauri build` on macOS reads `tauri.conf.json` with `tauri.macos.conf.json` merged over it (RFC 7396; arrays replace).
That merged configuration gives the `.app` the same `Info.plist`, `.lproj` resources, entitlements, icons and DMG
layout as the former single configuration file, and a release build from it is expected to sign, notarize and staple as
described here.

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

## Windows release builds

`pnpm release` on Windows (x64 only, `x86_64-pc-windows-msvc`; arm64 comes later) runs `rustup target add`, then
`tauri build --target x86_64-pc-windows-msvc --bundles nsis --locked`. `beforeBuildCommand` in
`tauri.windows.conf.json` fetches the ConPTY files and builds the daemon and the UI first. The output is
`target/x86_64-pc-windows-msvc/release/bundle/nsis/Octoboard_<version>_x64-setup.exe`; stale installers and `.sig` files
are deleted first. The NSIS installer is the update artifact itself: the updater downloads it and runs it. The update
key variables are left out of the build's environment as on macOS, `--no-sign-update` leaves the update signature to
`pnpm sign-update`, and without it `TAURI_SIGNING_PRIVATE_KEY` is required and the script writes `<installer>.sig` with
it, after the checks below and so after the last step that could change the installer's bytes. Nothing may touch the
installer afterwards.

```sh
# Unsigned installer, no update key (what the `windows-bundle` job of Code quality builds)
pnpm --filter @octoboard/desktop release --no-sign-update
# Unsigned installer and its update signature (TAURI_SIGNING_PRIVATE_KEY in .env.secret or the environment)
pnpm --filter @octoboard/desktop release
```

**Code signing** is the mode `WINDOWS_SIGNING=none|command|signpath` (default `none`; an environment variable or
`.env.secret` locally, a repository variable in CI). The first Windows release ships unsigned, so Windows SmartScreen
warns on the download; SignPath Foundation is the intended provider later. Nothing in `tauri.windows.conf.json` names a
provider.

- **`none`** — nothing is signed.
- **`command`** — one run: `WINDOWS_SIGN_COMMAND`, a command line with `%1` where a file's path goes (double quotes group
  words, no other escapes), is run once per file `tauri build` signs. Its first word must name an executable (`.exe`):
  `sign-file.mjs` starts it without a shell (`execFileSync`), and Node refuses to start a `.cmd` or `.bat` file that
  way, so a `WINDOWS_SIGN_COMMAND` naming a batch signer is refused: its sign step fails, and with it the build. The script passes `bundle.windows.signCommand`
  through `--config`, pointing at `sign-file.mjs`, which reads the command line from the environment (Tauri replaces
  every `%1` in the arguments, the command's own among them) and skips Microsoft's `conpty.dll` and `OpenConsole.exe`.
  The provider's own credentials are environment variables of the build. Locally:
  `WINDOWS_SIGNING=command WINDOWS_SIGN_COMMAND='<signer> <options> %1' pnpm --filter @octoboard/desktop release`.
- **`signpath`** — SignPath signs only uploaded GitHub artifacts and each request is approved by hand, so there are
  three runs with a submission between them, `--stage` each: `build` (`tauri build --no-bundle`, then our own
  `Octoboard.exe` and `octoboardd.exe` are left in `target/signpath/binaries/`), `bundle --signed <dir>` (checks the
  signed files, puts them back, `tauri bundle --bundles nsis --no-binary-patching`, the patching of the shell would
  break its signature; the installer is left in `target/signpath/installer/`) and `finish --signed <dir>` (checks the
  signed installer, puts it in the bundle directory, checks the result). The conpty files are in neither submission.
  The NSIS uninstaller is generated inside `makensis` and so is not signed. Add `--no-sign-update` to each when the
  update is signed by `pnpm sign-update` afterwards.

After the build, `release` looks inside the installer (extracted with 7-Zip, `7z.exe`, which a runner has; without it
the check is skipped for `none` and fails otherwise). Outside `none`, `signtool verify /pa /v` must accept the
installer, `Octoboard.exe` and `octoboardd.exe`, or the release fails. In every mode it checks that the installer's
`conpty.dll` and `OpenConsole.exe` files are byte-identical to the files `fetch-conpty.mjs` took from the pinned
package (failing if not) and reports, without failing, who signed each file.

What a signed build is expected to do, in either signing mode:

- `command`: Tauri hands `bundle.windows.signCommand` `Octoboard.exe`, `octoboardd.exe`, the NSIS uninstaller and the
  `setup.exe`, and never `conpty.dll` or `OpenConsole.exe` (`sign-file.mjs` would skip them in any case). The check on
  the installer is what reports whether the shell and the sidecar in it are signed.
- `signpath`: our own PE files are signed in the first round and the installer in the second; the uninstaller is not
  signed (above).
- In both, `signtool verify /pa /v` passes on the installer and on `Octoboard.exe` and `octoboardd.exe` inside it, and
  the ConPTY files in it are byte-identical to the package's and keep Microsoft's signature.
- The look inside the installer expects exactly one of each of those four files, found by the tail of its path in what
  7-Zip extracts, and fails the release otherwise.

## Publishing a release

`pnpm --filter @octoboard/desktop publish-release (--notes "text" | --notes-file path) [--platforms macos|windows|macos,windows] [--windows-dir <dir>] [--dry-run]`
publishes what the last signed `pnpm release` built, for the version in `apps/desktop/package.json`. A release carries
both platforms unless `--platforms` selects one, and a platform that is not selected is neither needed nor touched: its
files are not looked for and its manifests are not read or written. The manifest model it writes to (the fixed-tag
`updater` release, one pair of manifests per platform, the channels, the manifest contents, asset naming, a manifest
never moving backwards) is described in `docs/product/in-app-updates.md`, section "Where updates come from"; this
section is the procedure.

macOS is the universal build in `target/universal-apple-darwin/release/bundle` and is checked with macOS tools, so a
macOS publish runs on a Mac. Windows comes as `--windows-dir <dir>`, required with Windows (and refused without it): `<dir>` holds
`Octoboard_<version>_x64-setup.exe` and its `.sig`, as the Windows build and `sign-update` left them (the installer
named for the version, the `.sig` present and not older than it). A Windows-only publish needs no macOS tool and runs on
any machine.

1. It refuses unless `gh` is logged in, the working tree is clean and each selected build is this version's. macOS:
   signed and notarized, the `.app` says it is `<version>`, the `.dmg` is named for it and passes `stapler validate`,
   and the update archive and its signature are no older than the `.app`. The release `desktop-v<version>` is then
   compared with the selected platforms: one that has all of a platform's files, the same update file size and the
   same signature, is an earlier run that failed part-way, and that platform resumes from step 3; one that has none of a
   platform's files gets them added in step 2; any other (some of a platform's files, files that belong to neither
   platform, a different build of the same version, a draft, an immutable release when files must be added) is
   refused, since a published file is never replaced: raise the version instead. A platform that is not selected is
   not compared.
2. It creates the release `desktop-v<version>` on the current commit (which must be pushed), marked prerelease when the
   version has a suffix, with the selected platforms' files (macOS: the `.dmg` and the update archive; Windows: the
   installer), each update file with its signature. When the release exists already, it adds the files of the
   platform that are missing and leaves the rest as it is, so a version can be published for the second platform later.
3. It creates the `updater` release if it is missing (as a prerelease, so it is never the repository's latest), then
   uploads each selected platform's manifests the version goes to with `--clobber`: `beta.json` and `stable.json` for
   macOS, `beta-windows.json` and `stable-windows.json` for Windows, the beta one always and the stable one unless the
   version has a prerelease suffix. Until a platform's first stable release, a prerelease goes to its stable manifest
   too: while that manifest is missing or announces a prerelease, it is written like the beta one, and once it announces
   a stable version it takes stable versions only again (reported when a prerelease leaves it alone). This is a
   one-time exception for the first betas, `0.1.0-beta.1` to `0.1.0-beta.3`, which follow Stable by default and read
   only the stable manifests (see "Update channels" in `docs/product/in-app-updates.md`); it ends with the first stable
   release newer than what the stable manifest announces, through the usual ordering. A manifest that already
   announces a newer version is left alone and reported. A manifest that is not valid is refused, with nothing created
   yet; once a stable manifest has announced a stable version, restore it with the latest stable release rather than
   deleting it, since a missing stable manifest takes prereleases again. Immutable releases must stay off for the
   repository: the `updater` release is rewritten by every release, and the script refuses when it is immutable (it
   reads the `isImmutable` field, which needs a `gh` recent enough to report it).

A manifest carries only its own platform's entries, so publishing one platform never writes the other's manifests: its
users are offered nothing for that version until it is published for them.

`--dry-run` prints the plan and the manifests without calling GitHub. As it reads no manifest, a prerelease's plan names
the stable manifest as written only while it announces no stable version yet.

## Releasing by tag

Pushing a tag `desktop-v<version>` runs `.github/workflows/release.yml`, which validates the tag, builds the macOS
and Windows releases in parallel on their own runners and publishes them from a job of its own. Application quality
checks run locally before creating the tag; release CI does not repeat them. The manual `pnpm release` and
`publish-release` path stays available.

By default a tag releases both platforms. Two things select one: `release:version --platforms macos|windows` ends the
tag's message with a line `Platforms: macos` (or `windows`), which `validate` reads and `.github/release-notes.sh`
leaves out of the notes (no such line, or `Platforms: macos,windows`, is both); and running the **Release** workflow
by hand (**Run workflow**) takes the `tag` of an existing version tag and a `platforms` input (`both`, `macos` or
`windows`), which wins over the tag's own line. The second is how a platform a version's tag left out is released
later, from the same tag: the versioned release gets that platform's files added and its manifests written.

### Prepare and push a version

Run from the clean main checkout on `main` on macOS, with dependencies installed, the Rust toolchain available and
Git access to `origin`:

```sh
pnpm --filter @octoboard/desktop release:version 0.1.0-beta.2 --notes-file /tmp/octoboard-release-notes.md --dry-run
pnpm --filter @octoboard/desktop release:version 0.1.0-beta.2 --notes-file /tmp/octoboard-release-notes.md
```

Supply exactly one nonempty notes source: `--notes "text"` or `--notes-file path`. A relative notes path is resolved
from the repository root, even when pnpm runs the command in the desktop package; absolute paths work too. Keep an
uncommitted notes file outside the tracked tree or in an ignored location so the main checkout stays clean. The tag
message is the same text shown on the release page and in the application's update notice; Markdown headings are
preserved. `--platforms macos|windows` releases one platform only: the tag message then ends with a blank line and
`Platforms: macos` (or `windows`), which is not part of the notes. Without it the version is for both platforms.

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
3. Rebases onto main, runs `pnpm verify --quick`, fast-forwards main and removes the release worktree and branch.
4. Runs `pnpm verify --release --fresh --head <release-commit>` locally on the version commit, regardless of the
   changed files or previous passes. This checks identity, application lint, production UI types, application UI
   tests, desktop script tests, Rust formatting, clippy and both Rust crates' tests. Website and gallery-only checks
   are excluded. It then runs `pnpm verify --base <fetched-remote-main> --head <release-commit>` for every unpushed
   main change, reusing the release checks' passes where the step commands match and adding any other affected checks.
   Both runs require that exact commit and a clean checkout; the command stops if main changes during preparation or
   verification.
5. Creates the annotated version tag on the checked commit and atomically pushes that exact commit to remote main
   together with its tag. Git hooks stay enabled, and a refused push updates neither remote ref.

Successful command completion means the tag was pushed, not that the application has been published. Follow the
**Release** workflow in GitHub Actions through its validation, build, signing, notarization and publishing steps before
announcing the release.

Failures leave the current state for inspection; the command never resets history or force-removes a worktree. Before
the merge, inspect the named release branch and worktree, correct the cause, and finish or clean up that attempt before
starting another. After the merge, the version commit stays on main; the same `release:version` command cannot be
rerun because the version must increase. Correct the failure, run `pnpm verify --release --fresh --head <release-commit>`
on clean main, then `pnpm verify --base origin/main --head <release-commit>`. Create the annotated tag only if it is
absent, and atomically push the checked commit and tag after confirming any existing local tag has the intended commit
and notes. Do not delete or replace a published tag to retry. The one exception is a tag no release was created from
(nothing under its name on the releases page, and the `updater` release's manifests still naming the previous version)
whose fix changes the code or the workflow files: run the two checks above on the fixed commit on main, re-create the
annotated tag there from the same message (`git tag -f -a desktop-v<version> <commit> --cleanup=whitespace -F
<message-file>`, keeping any `Platforms:` line) and push it with `--force`, which starts the workflow again from the tag.

### Release CI

The workflow:

1. `validate` runs `.github/release-notes.sh`: the tag must equal `desktop-v<version>` from `apps/desktop/package.json`
   and be an annotated tag with a non-empty message. The message, minus any signature and its `Platforms:` line, is the
   release notes, which the in-app update notice shows too. The platforms are the `platforms` input of a manual run, else
   the tag message's `Platforms:` line, else both; they are the job's output. It also checks that the tagged commit is
   on `main`, that the repository secrets below are set (the update key always; the Apple ones only for macOS), and
   that `Cargo.lock` matches the manifests (`cargo metadata --locked`), which the `--locked` build would otherwise
   find after the build starts. With Windows it checks the signing mode too: the repository variable
   `WINDOWS_SIGNING` (`none`, `command` or `signpath`; unset is `none`) and what that mode needs. A manual run checks
   out the tag it names, and every job builds that tag.
2. After `validate` passes, the Windows build and `macos` (`macos-26`) run in parallel, each only when its platform is
   selected; neither waits for the other. The Windows build follows `WINDOWS_SIGNING`, which `validate` hands on as
   an output:
   - `none` and `command`: the job `windows` (`windows-latest`) is one `release --no-sign-update`.
   - `signpath`: each SignPath request waits for an approval given by hand, and a GitHub-hosted job is ended after
     six hours, so the route is five jobs that pass artifacts on, each with its own limit well under that.
     `windows-signpath-build` runs `--stage build` and uploads the executables (`unsigned-binaries`);
     `windows-signpath-sign-binaries` (`ubuntu-latest`) submits them with
     `signpath/github-action-submit-signing-request`, waits at most four hours (`wait-for-completion-timeout-in-seconds`,
     inside a 270-minute job) and uploads what comes back (`signed-binaries`); `windows-signpath-bundle` builds the tree
     again from the same commit (from the shared dependency cache), puts the signed executables in place with
     `--stage bundle` and uploads the installer twice, as `unsigned-installer` for SignPath and as `unsigned-nsis` under
     the bundler's name; `windows-signpath-sign-installer` submits and waits the same way (`signed-installer`); and
     `windows-signpath-finish` fetches the ConPTY files the installer is checked against, puts the signed installer in
     the bundle directory under that name with `--stage finish`, and signs the update. The two submitting jobs carry
     `actions: read` beside `contents: read`, as SignPath reads the artifact and the run with the job's token.

     Design assumptions of this route: each submission is made from a Linux job other than the one that uploaded the
     artifact (SignPath's documentation shows upload and request in one job), and SignPath's origin check is expected
     to accept an artifact of another job of the same run. What SignPath hands back is unpacked into the submitting
     job's output directory as the files themselves: `Octoboard.exe` and `octoboardd.exe` for the first round, and
     the one signed `*-setup.exe` for the second, which `finish` finds by that suffix. The `unsigned-nsis` artifact is
     there only to give the installer's file name under the bundler's naming; `finish` runs on a fresh runner, and
     the signed installer replaces what the bundler would have produced.

   Either way `sign-update` signs the installer in a step of its own, the only one given the update key, and the
   installer and its `.sig` are uploaded as the artifact `windows-installer`. The Windows build jobs restore the
   `windows-release` dependency cache.
3. `macos` restores the universal-build Rust dependency cache, then builds, signs and notarizes with
   `pnpm --filter @octoboard/desktop release --no-sign-update`. It signs the update archive in a step of its own
   (`sign-update`), the only step given the update key (no build step has it in its environment; code planted for
   later steps of the same runner could still read it). The signing certificate and the Developer ID G2 intermediate go
   into a keychain that exists for that run only, and the API key is written to a temporary file; both are removed
   right after the build. `validate` learns whether the secrets are set from boolean expressions, without their values
   reaching a step. It then packs `bundle/macos` and `bundle/dmg` of the universal bundle directory into one tar (which
   keeps symlinks, permissions and modification times that an artifact's zip would drop) and uploads it as the artifact
   `macos-bundle`.
4. `publish` needs `validate`, `macos`, `windows` and `windows-signpath-finish`, and runs when `validate` succeeded and
   each selected platform's build succeeded (for Windows, the job of the signing mode's route): an unselected
   platform's jobs, or the route not taken, are skipped, which does not stop it, and a selected one that failed or was
   cancelled does. It downloads the selected platforms' artifacts (the macOS tar is unpacked into
   `target/universal-apple-darwin/release`) and runs
   `node apps/desktop/scripts/publish-release.mjs --notes-file ... --platforms <selection>` with `--windows-dir` for
   Windows. It is the only job whose token may write (`contents: write`, for `gh`; every other job reads, and
   checkouts keep no credential in `.git/config`) and has no update key. It installs nothing (the script uses Node's
   built-in modules only) and runs on
   `macos-26` when macOS is selected, whose tools check the disk image, and on `ubuntu-latest` otherwise.
5. The whole workflow is in one concurrency group, so releases run one at a time. GitHub keeps one pending run, so a
   third run started while one runs and one waits replaces the waiting one, which must then be re-run.

Before the first release, or after dependency or toolchain changes, run **Warm Rust caches** manually on `main` to
prepare compatible dependency caches. Warming uses an unsigned universal build and an unsigned Windows installer, with
no signing credentials, and never publishes an application; its `windows` job fills the `windows-release` profile that
the Windows jobs of `release.yml` and `windows-bundle` restore. Tag runs restore main's caches; different version tags cannot restore one another's caches.
Local release checks still run before tagging. The [Rust cache module](../../../.github/actions/rust-cache/README.md)
documents the command, profiles, GitHub retention limits and the measurements to inspect before judging a speed change.

The **Code quality** workflow (`.github/workflows/quality.yml`) remains available on request for independent hosted
frontend, macOS native, Linux compilation and Windows checks; its `windows-bundle` job builds the unsigned Windows
installer (`WINDOWS_SIGNING=none`, no update key), the way to see `tauri build` pass on Windows without a release. It is not called by release CI and does not block publication.
The local macOS release checks do not cover Linux compilation; run that workflow when Linux verification is needed.

Repository secrets: `APPLE_CERTIFICATE` (base64 of the Developer ID Application `.p12`, private key included),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_PRIVATE_KEY`
(the contents of the API key's `.p8`), `TAURI_SIGNING_PRIVATE_KEY` (the key's contents) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (not set for a key without a password).

Windows code signing, by mode of the repository variable `WINDOWS_SIGNING` (none of it is needed for `none`):
`command` needs the variable `WINDOWS_SIGN_COMMAND` (the provider's own credentials go into the `windows` job's build
step `env` when a provider is chosen); `signpath` needs the secret `SIGNPATH_API_TOKEN` and the variables
`SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG`,
`SIGNPATH_BINARIES_CONFIGURATION_SLUG` and `SIGNPATH_INSTALLER_CONFIGURATION_SLUG` (the artifact configurations of the
two submissions: a zip holding `Octoboard.exe` and `octoboardd.exe`, and a zip holding the installer).

**The version is kept in two places**: `version` in `apps/desktop/package.json`, which `src-tauri/tauri.conf.json` reads
(its `version` is the path `../package.json`) and the release scripts read, and the workspace `version` in the root
`Cargo.toml`, which both crates take (`version.workspace = true`) and the daemon reports in the configuration export and
to the agent CLIs. `src-tauri/build.rs` fails every build and test of the shell when the two differ, when
`tauri.conf.json` names a version of its own, or when the daemon's manifest does not take the workspace version.

For manual tag preparation, raise the version in `apps/desktop/package.json` and in the root `Cargo.toml`, run
`cargo check` so that `Cargo.lock` records it, and commit the changes on `main`. Before creating the tag, run
`pnpm verify --release --fresh --head <release-commit>`, then
`pnpm verify --base origin/main --head <release-commit>`, on that exact commit with a clean checkout. Push the checked
commit to `main`, write the notes to a file, then run
`git tag -a desktop-v<version> --cleanup=whitespace -F <notes-file>` and `git push origin desktop-v<version>`.
`--cleanup=whitespace` keeps the notes' Markdown heading lines, which the default `--cleanup=strip` drops; the file
rather than the editor keeps Git's `#` template lines out of them. Push tags one at a time: GitHub creates no push
event when more than three tags are pushed at once.

When a run fails after `gh release create` but before the manifests are uploaded, re-running only the failed jobs
reuses the artifacts the build jobs already uploaded, so `publish` finds the same build and resumes; a re-run of
the build jobs rebuilds with a different signature, and `publish-release` refuses it. Delete the release but keep the
tag (`gh release delete desktop-v<version>`, without `--cleanup-tag`) and re-run the failed jobs. When one platform's
build fails, `publish` does not run and nothing is published: fix the cause and re-run the failed jobs, or release the
passing platform alone by running the workflow by hand for it and the other one later. The script's own messages still
describe the manual flow.

Every route above assumes the tag's commit still has the workflow files main has. GitHub refused the run's own token
the release of 0.1.0-beta.3 (`HTTP 403: Resource not accessible by integration` from `gh release create`, a re-run
alike) when a run by hand, started from main, published a tag whose workflow files main had since changed; others
report the same refusal of a release or a tag on a commit whose workflow files differ from main's, as it would need the
`workflows` permission, which the token never holds. So a fix to the workflow files cannot be released from an
existing tag, by re-running its jobs or by hand: move the tag (above) or release a new version. Whether the refusal
also reaches an upload to an existing release, as when a platform is added later, has not been seen.
