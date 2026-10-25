#!/usr/bin/env node
// Publishes the build `pnpm release` made, for the in-app updates to find: creates the versioned GitHub release
// `v<version>` with the disk image and the signed update archive, then writes the manifests the application
// reads (`beta.json`, and `stable.json` too unless the version has a prerelease suffix) to the `updater`
// release. Run `pnpm release` first, with a signed build. Outward-facing, so it is a separate, explicit step:
// nothing else calls it.
//
//   pnpm --filter @octoboard/desktop publish-release (--notes "text" | --notes-file path) [--dry-run]
//
// The version is the one in `tauri.conf.json`, which must be the one of the universal build in
// `target/universal-apple-darwin/release/bundle`. The release's tag is created on the current commit, which has to
// be pushed. `--dry-run` prints what would be published and calls nothing outside this machine.
//
// The versioned release is created before the manifests are written, so a manifest never names an archive that
// is not there yet. The build is universal, so one archive serves both platforms of the manifest. A run that
// failed after creating the release resumes from the manifests when run again (if it is the same build), and a
// manifest that already announces a newer version than this one is left alone, so a channel never moves backwards.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseArgs } from "node:util";
import { appDir, appVersion, findBundle, universalBundleDir as bundleDir } from "./bundle.mjs";
import {
  archiveName,
  buildManifest,
  isPrerelease,
  manifestFiles,
  manifestVersion,
  MANIFEST_RELEASE_TAG,
  planManifestWrites,
  PLATFORMS,
  releaseAssetUrl,
  repositorySlug,
  tag,
} from "./release-manifest.mjs";

const { values: options } = parseArgs({
  options: {
    notes: { type: "string" },
    "notes-file": { type: "string" },
    "dry-run": { type: "boolean", default: false },
  },
});
if ((options.notes === undefined) === (options["notes-file"] === undefined)) {
  throw new Error('Give the release notes once, as --notes "text" or --notes-file path.');
}
const notes = (options.notes ?? readFileSync(options["notes-file"], "utf8")).trim();
if (!notes) throw new Error("The release notes are empty.");
const dryRun = options["dry-run"];

const repoRoot = path.dirname(path.dirname(appDir));
const config = JSON.parse(readFileSync(path.join(appDir, "src-tauri", "tauri.conf.json"), "utf8"));
const appConfig = JSON.parse(readFileSync(path.join(repoRoot, "config", "app.json"), "utf8"));
const { version, productName } = config;
const prerelease = isPrerelease(version);
const repo = repositorySlug(appConfig.repositoryUrl);

// The files `pnpm release` made, which must be this version's, signed and notarized: the archive's name carries no
// version, so a leftover of an earlier build would be published under the new version's name.
const macosDir = path.join(bundleDir, "macos");
const app = findBundle(macosDir, ".app");
const dmg = findBundle(path.join(bundleDir, "dmg"), ".dmg");
const archive = findBundle(macosDir, ".app.tar.gz");
const signatureFile = findBundle(macosDir, ".app.tar.gz.sig");
const rebuild = "Run `pnpm release` again.";
if (appVersion(app) !== version) {
  throw new Error(`${app} is a ${appVersion(app)} build, and ${version} is the version in tauri.conf.json. ${rebuild}`);
}
if (!path.basename(dmg).includes(`_${version}_`)) {
  throw new Error(`${dmg} is not a ${version} build, and ${version} is the version in tauri.conf.json. ${rebuild}`);
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
const signature = readFileSync(signatureFile, "utf8");

const archiveAsset = archiveName(productName, version);
const manifest = buildManifest({
  version,
  notes,
  pubDate: new Date().toISOString(),
  url: releaseAssetUrl(appConfig.repositoryUrl, version, archiveAsset),
  signature,
});
const manifests = manifestFiles(version);
const assetNames = [path.basename(dmg), archiveAsset, `${archiveAsset}.sig`];

console.log(`version ${version} (${prerelease ? "prerelease" : "stable"}), universal: ${PLATFORMS.join(", ")}`);
console.log(`release ${tag(version)} gets ${assetNames.join(", ")}`);
console.log(`manifests ${manifests.join(", ")} go to the "${MANIFEST_RELEASE_TAG}" release, unless one announces a newer version:`);
console.log(JSON.stringify(manifest, null, 2));
if (dryRun) {
  console.log("--dry-run: nothing was published.");
  process.exit(0);
}

const gh = (...args) => execFileSync("gh", ["--repo", repo, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
const git = (...args) => execFileSync("git", args, { cwd: repoRoot, encoding: "utf8" }).trim();
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
      "version in tauri.conf.json (and in the package and crate manifests), run `pnpm release` and publish again. " +
      `Delete the release (\`gh release delete ${tag(version)} --cleanup-tag\`) only if nobody has downloaded it.`,
  );

// A release that has this build's assets, the same archive and the same signature is the one an earlier run of this
// script created before it failed: it resumes from the manifests. Any other is a published version. A rebuild of the
// same version signs differently, and manifests carrying its signature over the old archive's address would fail
// every client's verification.
const existing = releaseInfo(tag(version));
if (existing !== null) {
  const names = existing.assets.map((asset) => asset.name);
  if (existing.isDraft) {
    throw new Error(
      `The release ${tag(version)} exists as a draft, which clients cannot download from. Publish it ` +
        `(\`gh release edit ${tag(version)} --draft=false\`) or delete it (\`gh release delete ${tag(version)} --cleanup-tag\`), then publish again.`,
    );
  }
  if ([...names].sort().join("\n") !== [...assetNames].sort().join("\n")) {
    throw neverReplaced(`with other assets (${names.join(", ") || "none"})`);
  }
  const archiveSize = existing.assets.find((asset) => asset.name === archiveAsset).size;
  if (archiveSize !== statSync(archive).size || downloadAsset(tag(version), `${archiveAsset}.sig`)?.trim() !== signature.trim()) {
    throw neverReplaced("with a different build of this version");
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
  manifests.map((name) => {
    const content = updaterRelease === null ? undefined : downloadAsset(MANIFEST_RELEASE_TAG, name);
    try {
      return [name, content === undefined ? undefined : manifestVersion(content)];
    } catch (error) {
      throw new Error(`${name} on the "${MANIFEST_RELEASE_TAG}" release is not valid (${error.message}). Fix or delete it, then publish again.`, { cause: error });
    }
  }),
);

const staging = mkdtempSync(path.join(os.tmpdir(), "octoboard-publish-"));
try {
  // The archive and its signature go out under the names the manifest uses.
  const stagedArchive = path.join(staging, archiveAsset);
  const stagedSignature = `${stagedArchive}.sig`;
  copyFileSync(archive, stagedArchive);
  writeFileSync(stagedSignature, signature);
  const notesFile = path.join(staging, "notes.md");
  writeFileSync(notesFile, `${notes}\n`);

  if (existing !== null) {
    console.log(`the release ${tag(version)} already has this build's assets: resuming from the manifests.`);
  } else {
    console.log(`creating the release ${tag(version)}:`);
    gh(
      "release", "create", tag(version), dmg, stagedArchive, stagedSignature,
      "--title", `${productName} ${version}`,
      "--notes-file", notesFile,
      "--target", git("rev-parse", "HEAD"),
      ...(prerelease ? ["--prerelease"] : []),
    );
  }

  // Created once, as a prerelease so it is never the repository's "latest" release, which is the newest versioned one.
  if (updaterRelease === null) {
    console.log(`creating the "${MANIFEST_RELEASE_TAG}" release:`);
    gh(
      "release", "create", MANIFEST_RELEASE_TAG,
      "--title", "Update manifests",
      "--notes", `What the application's in-app updates read: ${manifests.join(", ")} name the newest version of each channel. Rewritten by every release.`,
      "--prerelease",
    );
  }
  const { write, skipped } = planManifestWrites(version, current);
  for (const { name, announced } of skipped) {
    console.log(`not writing ${name}: it announces ${announced}, which is newer than ${version}.`);
  }
  if (write.length > 0) {
    const manifestPaths = write.map((name) => {
      const file = path.join(staging, name);
      writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
      return file;
    });
    console.log(`writing the manifests ${write.join(", ")}:`);
    gh("release", "upload", MANIFEST_RELEASE_TAG, ...manifestPaths, "--clobber");
  }
  console.log(
    write.length > 0
      ? `published ${tag(version)}; ${write.join(" and ")} now announce it.`
      : `published ${tag(version)}; no manifest announces it, as every channel has a newer version.`,
  );
} finally {
  rmSync(staging, { recursive: true, force: true });
}
