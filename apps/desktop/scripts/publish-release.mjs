#!/usr/bin/env node
// Publishes the builds `pnpm release` made, for the in-app updates, the install script and the website to find: creates
// the versioned GitHub release `desktop-v<version>` with each platform's files (macOS: the disk image and the signed
// update archive; Windows: the installer; Linux: the daemon's archive for each architecture with its `.sha256`), each
// update file with its signature (each checked first against the update key's public key in `tauri.conf.json`, bound to
// this version and naming its file, so that one no client would accept is never published), then writes the manifests
// to the `updater` release: per platform, `beta.json` and `stable.json` for macOS, `beta-windows.json` and
// `stable-windows.json` for Windows and `beta-linux.json` and `stable-linux.json` for Linux, the beta one always and
// the stable one unless the version has a prerelease suffix.
// Until a platform's first stable release, a prerelease goes to its stable manifest too: the first betas follow Stable
// by default and read only that one (`stableTakesPrereleases` in `release-manifest.mjs`). Run `pnpm release` first
// (on Linux, once on a host of each architecture), with signed builds. Outward-facing, so it is a separate, explicit
// step: nothing else calls it.
//
//   pnpm --filter @octoboard/desktop publish-release (--notes "text" | --notes-file path)
//     [--platforms <macos,windows,linux or some of them>] [--windows-dir <dir>] [--linux-dir <dir>] [--dry-run]
//
// A release carries every platform unless `--platforms` selects some, and a platform not selected is neither needed
// nor touched: its files are not looked for and its manifests are not read or written. The macOS build is
// `target/universal-apple-darwin/release/bundle`, checked with macOS tools (`PlistBuddy`, `stapler`), so a macOS
// publish runs on a Mac. The Windows build comes as `--windows-dir`, required with Windows: a directory holding the
// installer `Octoboard_<version>_x64-setup.exe` and its `.sig`, as the Windows build made the installer and
// `sign-update` or the release workflow's `sign` job signed it. The Linux builds come as `--linux-dir`, required with
// Linux: a directory holding, for each architecture, the archive `octoboardd_<version>_linux_<arch>.tar.gz` with its
// `.sha256` and `.sig`, as `release-linux.mjs` made them and `sign-update` or the release workflow's `sign` job signed
// them.
//
// The version is the one in `apps/desktop/package.json` (which `tauri.conf.json` reads), which must be the one of
// the builds. The release's tag is created on the current commit, which has to be pushed. `--dry-run` prints what
// would be published and calls nothing outside this machine.
//
// The versioned release is shared by the platforms: a later run for another platform adds its missing files to the
// release a first one created, and a published file is never replaced. The release is created or extended before
// the platform's manifests are written, so a manifest never names a file that is not there yet. A run that failed
// after creating the release resumes from the manifests when run again (if it is the same build), and a manifest that
// already announces a newer version than this one is left alone, so a channel never moves backwards.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseArgs } from "node:util";
import { gitChecked } from "../../../scripts/lib/git.mjs";
import { appDir, appVersion, findBundle, repoRoot, universalBundleDir as bundleDir } from "./bundle.mjs";
import { linuxReleaseFiles } from "./linux-release.mjs";
import {
  buildManifest,
  dmgName,
  HOLDER_FLOOR_FILE,
  isPrerelease,
  manifestFiles,
  manifestNames,
  manifestVersion,
  MANIFEST_RELEASE_TAG,
  parsePlatforms,
  planManifestWrites,
  planReleaseAssets,
  readHolderFloor,
  releaseAssetUrl,
  releaseAssets,
  releaseLayout,
  repositorySlug,
  tag,
} from "./release-manifest.mjs";
import { verifyUpdateSignature } from "./update-signature.mjs";
import { windowsReleaseFiles } from "./windows-release.mjs";

const { values: options } = parseArgs({
  options: {
    notes: { type: "string" },
    "notes-file": { type: "string" },
    platforms: { type: "string" },
    "windows-dir": { type: "string" },
    "linux-dir": { type: "string" },
    "dry-run": { type: "boolean", default: false },
  },
});
if ((options.notes === undefined) === (options["notes-file"] === undefined)) {
  throw new Error('Give the release notes once, as --notes "text" or --notes-file path.');
}
const platforms = parsePlatforms(options.platforms);
const selected = (platform) => platforms.includes(platform);
for (const [platform, option, name] of [["windows", "windows-dir", "Windows"], ["linux", "linux-dir", "Linux"]]) {
  if (selected(platform) !== (options[option] !== undefined)) {
    throw new Error(
      selected(platform)
        ? `Give the directory of the ${name} build as --${option} <dir>, or leave ${name} out with --platforms.`
        : `--${option} belongs to the ${name} platform, which --platforms leaves out.`,
    );
  }
}
const notes = (options.notes ?? readFileSync(options["notes-file"], "utf8")).trim();
if (!notes) throw new Error("The release notes are empty.");
const dryRun = options["dry-run"];

const tauriConfig = JSON.parse(readFileSync(path.join(appDir, "src-tauri", "tauri.conf.json"), "utf8"));
const { productName } = tauriConfig;
const { version } = JSON.parse(readFileSync(path.join(appDir, "package.json"), "utf8"));
const appConfig = JSON.parse(readFileSync(path.join(repoRoot, "config", "app.json"), "utf8"));
const prerelease = isPrerelease(version);
const repo = repositorySlug(appConfig.repositoryUrl);

// What each selected platform publishes, under the names `releaseLayout` gives: `files` go out as they are, under
// `name` (copied there when the file's own name differs), `updates` are the files the manifest points to, each with the
// manifest keys it serves and its signature, published as `signatureName` (and, on Linux, its SHA-256), and `assets`
// all of their names, as `planReleaseAssets` knows them.
const publishing = {};
const names = { productName, version };
// The platform's `releaseLayout` with each name's source file from `sources` and each update's signature content.
const describe = (platform, sources, signatures, digests = {}) => {
  const layout = releaseLayout(platform, names);
  return {
    files: layout.files.map((name) => ({ name, file: sources[name] })),
    updates: layout.updates.map(({ name, keys, signature }) => ({
      name,
      keys,
      file: sources[name],
      signatureName: signature,
      signature: signatures[name],
      sha256: digests[name],
    })),
    assets: releaseAssets(platform, names),
  };
};

if (selected("macos")) {
  // The files `pnpm release` made, which must be this version's, signed and notarized: the archive's name carries no
  // version, so a leftover of an earlier build would be published under the new version's name.
  const macosDir = path.join(bundleDir, "macos");
  const app = findBundle(macosDir, ".app");
  const dmg = findBundle(path.join(bundleDir, "dmg"), ".dmg");
  const archive = findBundle(macosDir, ".app.tar.gz");
  const signatureFile = findBundle(macosDir, ".app.tar.gz.sig");
  const rebuild = "Run `pnpm release` again.";
  if (appVersion(app) !== version) {
    throw new Error(`${app} is a ${appVersion(app)} build, and ${version} is the version in apps/desktop/package.json. ${rebuild}`);
  }
  if (path.basename(dmg) !== dmgName(productName, version)) {
    throw new Error(
      `${dmg} is not named ${dmgName(productName, version)}: it is not a ${version} build (the version in ` +
        "apps/desktop/package.json), or the bundler names disk images differently now, which the website's macOS " +
        `download relies on. ${rebuild}`,
    );
  }
  for (const file of [archive, signatureFile]) {
    if (statSync(file).mtimeMs < statSync(app).mtimeMs) {
      throw new Error(`${file} is older than ${app}, so it is left over from an earlier build. ${rebuild}`);
    }
  }
  try {
    execFileSync("xcrun", ["stapler", "validate", dmg], { stdio: "ignore" });
  } catch {
    throw new Error(`${dmg} is not notarized and stapled, which a published disk image has to be. ${rebuild}`);
  }
  const [update] = releaseLayout("macos", names).updates;
  publishing.macos = describe(
    "macos",
    { [path.basename(dmg)]: dmg, [update.name]: archive },
    { [update.name]: readFileSync(signatureFile, "utf8") },
  );
}
if (selected("windows")) {
  const windows = windowsReleaseFiles(path.resolve(options["windows-dir"]), productName, version);
  const [update] = releaseLayout("windows", names).updates;
  publishing.windows = describe(
    "windows",
    { [update.name]: windows.installer },
    { [update.name]: readFileSync(windows.signature, "utf8") },
  );
}
if (selected("linux")) {
  const linux = linuxReleaseFiles(path.resolve(options["linux-dir"]), version);
  const sources = {};
  const signatures = {};
  const digests = {};
  for (const { archive, sha256File, signature, sha256 } of linux) {
    sources[path.basename(archive)] = archive;
    sources[path.basename(sha256File)] = sha256File;
    signatures[path.basename(archive)] = readFileSync(signature, "utf8");
    digests[path.basename(archive)] = sha256;
  }
  publishing.linux = describe("linux", sources, signatures, digests);
}

// Every update signature is checked against the update key's public key before anything is published: one made with
// another key, for other bytes or for another version would make every client refuse the update.
for (const platform of platforms) {
  for (const update of publishing[platform].updates) {
    verifyUpdateSignature({
      signatureText: update.signature,
      file: update.file,
      publicKeyText: tauriConfig.plugins.updater.pubkey,
      name: path.basename(update.file),
      version,
    });
  }
}

const pubDate = new Date().toISOString();
// Read from the tree, which is the release commit: a publish refuses uncommitted changes below.
const holderFloor = readHolderFloor(repoRoot);
if (holderFloor === undefined) throw new Error(`${HOLDER_FLOOR_FILE} has no FLOOR constant to publish.`);
for (const platform of platforms) {
  const entries = {};
  for (const update of publishing[platform].updates) {
    const entry = { url: releaseAssetUrl(appConfig.repositoryUrl, version, update.name), signature: update.signature, sha256: update.sha256 };
    for (const key of update.keys) entries[key] = entry;
  }
  publishing[platform].manifest = buildManifest({ version, notes, pubDate, platform, holderFloor, entries });
  publishing[platform].manifests = manifestFiles(platform);
}
const assetsOf = (platform) => publishing[platform].assets;

console.log(`version ${version} (${prerelease ? "prerelease" : "stable"}), platforms: ${platforms.join(", ")}`);
console.log(`release ${tag(version)} gets ${platforms.flatMap(assetsOf).join(", ")}`);
for (const platform of platforms) {
  const { manifests, manifest } = publishing[platform];
  const { beta, stable } = manifestNames(platform);
  console.log(
    prerelease
      ? `${platform} manifests ${beta}, and ${stable} while that announces no stable version yet, go to the "${MANIFEST_RELEASE_TAG}" release, unless one announces a newer version:`
      : `${platform} manifests ${manifests.join(", ")} go to the "${MANIFEST_RELEASE_TAG}" release, unless one announces a newer version:`,
  );
  console.log(JSON.stringify(manifest, null, 2));
}
if (dryRun) {
  console.log("--dry-run: nothing was published.");
  process.exit(0);
}

const gh = (...args) => execFileSync("gh", ["--repo", repo, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
const git = (...args) => gitChecked(repoRoot, ...args);
// A release's draft and immutable flags and its assets' names and sizes, or null when there is no such release.
// Only "release not found" says there is none; anything else (no network, no login) is not an answer.
const releaseInfo = (releaseTag) => {
  try {
    return JSON.parse(gh("release", "view", releaseTag, "--json", "assets,isDraft,isImmutable"));
  } catch (error) {
    if (/release not found/i.test(String(error.stderr ?? ""))) return null;
    throw error;
  }
};
// The content of a release's asset, or undefined when it has none by that name.
const downloadAsset = (releaseTag, name) => {
  try {
    return gh("release", "download", releaseTag, "--pattern", name, "--output", "-");
  } catch (error) {
    if (/no assets match/i.test(String(error.stderr ?? ""))) return undefined;
    throw error;
  }
};

try {
  execFileSync("gh", ["auth", "status"], { stdio: "ignore" });
} catch {
  throw new Error("The GitHub CLI is not installed or not logged in. Install `gh` and run `gh auth login`.");
}
if (git("status", "--porcelain")) {
  throw new Error("The working tree has uncommitted changes. Publish from the commit the build was made from.");
}
const neverReplaced = (detail) =>
  new Error(
    `The release ${tag(version)} already exists ${detail}, and a published version is never replaced. Raise the ` +
      "version in apps/desktop/package.json and the root Cargo.toml, run `cargo check` so that Cargo.lock " +
      "records it, commit and push, run `pnpm release` and publish again. " +
      `Delete the release (\`gh release delete ${tag(version)} --cleanup-tag\`) only if nobody has downloaded it.`,
  );

// A platform whose files are all in the release already, the same update file and the same signature, is the one an
// earlier run of this script uploaded before it failed: it resumes from the manifests. Any other release is a
// published version. A rebuild of the same version signs differently, and manifests carrying its signature over the
// old file's address would fail every client's verification.
const existing = releaseInfo(tag(version));
const plan = planReleaseAssets({
  platforms,
  assets: Object.fromEntries(platforms.map((platform) => [platform, assetsOf(platform)])),
  existing: existing === null ? null : existing.assets.map((asset) => asset.name),
  productName,
  version,
});
if (existing !== null) {
  if (existing.isDraft) {
    throw new Error(
      `The release ${tag(version)} exists as a draft, which clients cannot download from. Publish it ` +
        `(\`gh release edit ${tag(version)} --draft=false\`) or delete it (\`gh release delete ${tag(version)} --cleanup-tag\`), then publish again.`,
    );
  }
  if (plan.conflict !== null) throw neverReplaced(plan.conflict);
  for (const platform of plan.resume) {
    for (const update of publishing[platform].updates) {
      const size = existing.assets.find((asset) => asset.name === update.name).size;
      if (
        size !== statSync(update.file).size ||
        downloadAsset(tag(version), update.signatureName)?.trim() !== update.signature.trim()
      ) {
        throw neverReplaced(`with a different ${platform} build of this version`);
      }
    }
  }
  if (plan.add.length > 0 && existing.isImmutable) {
    throw new Error(
      `The release ${tag(version)} is immutable, so the ${plan.add.join(" and ")} files cannot be added to it. ` +
        "Turn off immutable releases for the repository.",
    );
  }
}

// The manifests are read before anything is created, so one that cannot be read stops the run with nothing half done.
const updaterRelease = releaseInfo(MANIFEST_RELEASE_TAG);
if (updaterRelease?.isImmutable) {
  throw new Error(
    `The "${MANIFEST_RELEASE_TAG}" release is immutable, and its manifests are rewritten by every release. Turn off ` +
      "immutable releases for the repository.",
  );
}
const current = Object.fromEntries(
  platforms.flatMap((platform) => publishing[platform].manifests).map((name) => {
    const content = updaterRelease === null ? undefined : downloadAsset(MANIFEST_RELEASE_TAG, name);
    try {
      return [name, content === undefined ? undefined : manifestVersion(content)];
    } catch (error) {
      throw new Error(`${name} on the "${MANIFEST_RELEASE_TAG}" release is not valid (${error.message}). Fix or delete it, then publish again (a stable manifest, once it has announced a stable version, is restored with the latest stable release rather than deleted: missing, it would take prereleases again).`, { cause: error });
    }
  }),
);

const staging = mkdtempSync(path.join(os.tmpdir(), "octoboard-publish-"));
try {
  // The files go out under the names the manifests use, the signatures written beside them.
  const stage = (platform) => {
    const { files, updates } = publishing[platform];
    const staged = files.map(({ name, file }) => {
      if (path.basename(file) === name) return file;
      const copy = path.join(staging, name);
      copyFileSync(file, copy);
      return copy;
    });
    const signatures = updates.map((update) => {
      const signature = path.join(staging, update.signatureName);
      writeFileSync(signature, update.signature);
      return signature;
    });
    return [...staged, ...signatures];
  };
  const notesFile = path.join(staging, "notes.md");
  writeFileSync(notesFile, `${notes}\n`);

  if (plan.create) {
    console.log(`creating the release ${tag(version)}:`);
    gh(
      "release", "create", tag(version), ...platforms.flatMap(stage),
      "--title", `${productName} ${version}`,
      "--notes-file", notesFile,
      "--target", git("rev-parse", "HEAD"),
      ...(prerelease ? ["--prerelease"] : []),
    );
  } else {
    for (const platform of plan.resume) console.log(`the release ${tag(version)} already has this build's ${platform} files: resuming from the manifests.`);
    if (plan.add.length > 0) {
      console.log(`adding the ${plan.add.join(" and ")} files to the release ${tag(version)}:`);
      gh("release", "upload", tag(version), ...plan.add.flatMap(stage));
    }
  }

  // Created once, as a prerelease so it is never the repository's "latest" release, which is the newest versioned one.
  if (updaterRelease === null) {
    console.log(`creating the "${MANIFEST_RELEASE_TAG}" release:`);
    gh(
      "release", "create", MANIFEST_RELEASE_TAG,
      "--title", "Update manifests",
      "--notes", "What the application's in-app updates, the install script and the website read: the manifests name the newest version of each channel, one set per platform. Rewritten by every release.",
      "--prerelease",
    );
  }
  const written = [];
  for (const platform of platforms) {
    const { write, skipped } = planManifestWrites(version, platform, current);
    for (const { name, announced, reason } of skipped) {
      console.log(
        reason === "newer"
          ? `not writing ${name}: it announces ${announced}, which is newer than ${version}.`
          : `not writing ${name}: it announces the stable version ${announced}, so it takes no prerelease any more.`,
      );
    }
    if (write.length === 0) continue;
    const manifestPaths = write.map((name) => {
      const file = path.join(staging, name);
      writeFileSync(file, `${JSON.stringify(publishing[platform].manifest, null, 2)}\n`);
      return file;
    });
    console.log(`writing the ${platform} manifests ${write.join(", ")}:`);
    gh("release", "upload", MANIFEST_RELEASE_TAG, ...manifestPaths, "--clobber");
    written.push(...write);
  }
  console.log(
    written.length > 0
      ? `published ${tag(version)}; ${written.join(" and ")} now announce it.`
      : `published ${tag(version)}; no manifest announces it, as every manifest it could go to has a newer or a stable version.`,
  );
} finally {
  rmSync(staging, { recursive: true, force: true });
}
