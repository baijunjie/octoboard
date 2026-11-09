// What `publish-release.mjs` decides without touching GitHub: which manifests a version goes into, the names
// and addresses of its assets, and the manifest itself, in the static format of Tauri's updater. Kept apart from
// the script so `release-manifest.test.mjs` can check it.

import { readFileSync } from "node:fs";
import path from "node:path";

// The release that holds the manifests, under one fixed tag whatever version they announce. The application
// reads them from there (`UpdateChannel` and the platform halves in `src-tauri/src/updater/`).
export const MANIFEST_RELEASE_TAG = "updater";

// Semver, with a prerelease suffix (`-beta.1`) and build metadata optional.
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

export function assertVersion(version) {
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new Error(`"${version}" is not a semantic version (for example 0.2.0 or 0.2.0-beta.1).`);
  }
}

// Whether a version has a prerelease suffix, which keeps it off the stable channel (but see `stableTakesPrereleases`).
export function isPrerelease(version) {
  assertVersion(version);
  return version.includes("-");
}

// The platforms a release can carry, in the order they are listed everywhere. A release carries both unless one is
// asked for.
export const RELEASE_PLATFORMS = ["macos", "windows"];

// The platforms a selection such as `macos` or `macos,windows` names, in the order of `RELEASE_PLATFORMS`; no
// selection (undefined) is both. Anything else, an empty or a repeated name included, is refused.
export function parsePlatforms(selection) {
  if (selection === undefined) return [...RELEASE_PLATFORMS];
  const names = String(selection).split(",");
  const unknown = names.filter((name) => !RELEASE_PLATFORMS.includes(name));
  if (unknown.length > 0 || new Set(names).size !== names.length) {
    throw new Error(`"${selection}" is not a platform selection; give ${RELEASE_PLATFORMS.join(", ")} or both separated by a comma.`);
  }
  return RELEASE_PLATFORMS.filter((platform) => names.includes(platform));
}

const PLATFORMS_TRAILER = /^Platforms: (\S+)$/;

// The line that ends an annotated tag's message when the version carries one platform only; null for both, which is
// what a message without the line means.
export function platformsTrailer(platforms) {
  const selected = parsePlatforms(platforms.join(","));
  return selected.length === RELEASE_PLATFORMS.length ? null : `Platforms: ${selected.join(",")}`;
}

// Splits an annotated tag's message into the release notes and the platforms it selects: a last non-blank line
// `Platforms: <selection>` is the selection and is not part of the notes (a selection that does not parse is an
// error, not notes); without it the notes are the whole message and the platforms are both.
export function readTagMessage(message) {
  const lines = message.trimEnd().split("\n");
  const match = PLATFORMS_TRAILER.exec(lines.at(-1));
  if (!match) return { notes: message.trimEnd(), platforms: [...RELEASE_PLATFORMS] };
  return { notes: lines.slice(0, -1).join("\n").trimEnd(), platforms: parsePlatforms(match[1]) };
}

// The manifests of each platform's updates: macOS keeps the names the first releases published and the installed
// applications read, which must go on serving Mac updates; Windows has a pair of its own.
const MANIFEST_NAMES = {
  macos: { beta: "beta.json", stable: "stable.json" },
  windows: { beta: "beta-windows.json", stable: "stable-windows.json" },
};

// A platform's manifests by channel: the beta one, which every release updates, so beta users are offered stable
// releases too, and the stable one, which a release without a prerelease suffix updates as well (and a prerelease
// while `stableTakesPrereleases` holds).
export function manifestNames(platform) {
  const names = MANIFEST_NAMES[platform];
  if (!names) throw new Error(`"${platform}" is not a release platform.`);
  return names;
}

// Every manifest of a platform, the ones a release may be written to: `planManifestWrites` decides which it is.
export function manifestFiles(platform) {
  const { beta, stable } = manifestNames(platform);
  return [beta, stable];
}

// The one-time exception for the first betas: 0.1.0-beta.1 to 0.1.0-beta.3 follow the Stable channel by default and
// read only the stable manifests, so until a platform's first stable release a prerelease goes to its stable manifest
// too. A stable manifest takes prereleases while it is missing or announces a prerelease (`announced`, undefined when
// there is none); once it announces a stable version, it takes stable versions only again.
export function stableTakesPrereleases(announced) {
  return announced === undefined || isPrerelease(announced);
}

// Orders two semantic versions: negative when `a` comes before `b`, zero when they are the same release, positive
// when `a` comes after. A prerelease comes before its release (`1.0.0-rc.1` < `1.0.0`), prerelease identifiers are
// compared one by one (numbers as numbers, and below any text), and build metadata does not count.
export function compareVersions(a, b) {
  assertVersion(a);
  assertVersion(b);
  const parse = (version) => {
    const [core, prerelease] = version.split("+")[0].split(/-(.*)/s);
    return { numbers: core.split(".").map(Number), identifiers: prerelease === undefined ? null : prerelease.split(".") };
  };
  const left = parse(a);
  const right = parse(b);
  for (let i = 0; i < 3; i += 1) {
    if (left.numbers[i] !== right.numbers[i]) return left.numbers[i] - right.numbers[i];
  }
  if (left.identifiers === null || right.identifiers === null) {
    return (left.identifiers === null ? 1 : 0) - (right.identifiers === null ? 1 : 0);
  }
  const isNumber = (identifier) => /^\d+$/.test(identifier);
  const length = Math.min(left.identifiers.length, right.identifiers.length);
  for (let i = 0; i < length; i += 1) {
    const [x, y] = [left.identifiers[i], right.identifiers[i]];
    if (x === y) continue;
    if (isNumber(x) && isNumber(y)) return Number(x) - Number(y);
    if (isNumber(x) !== isNumber(y)) return isNumber(x) ? -1 : 1;
    return x < y ? -1 : 1;
  }
  return left.identifiers.length - right.identifiers.length;
}

// The version a manifest's text announces; throws when it is not JSON or has no semantic version.
export function manifestVersion(text) {
  let version;
  try {
    version = JSON.parse(text).version;
  } catch {
    throw new Error("not JSON");
  }
  if (typeof version !== "string" || !SEMVER.test(version)) throw new Error("\"version\" is not a semantic version");
  return version;
}

// Splits a platform's manifests (`manifestFiles`) into the ones `version` is written to and the ones skipped:
// `current` maps a manifest's name to the version it announces now (absent when there is none yet). A manifest already
// announcing a newer version is left alone (`reason: "newer"`), so publishing an older release late never downgrades a
// channel's users; a prerelease is left out of a stable manifest that announces a stable version (`reason: "stable"`,
// see `stableTakesPrereleases`).
export function planManifestWrites(version, platform, current) {
  const { stable } = manifestNames(platform);
  const write = [];
  const skipped = [];
  for (const name of manifestFiles(platform)) {
    const announced = current[name];
    if (announced !== undefined && compareVersions(announced, version) > 0) skipped.push({ name, announced, reason: "newer" });
    else if (name === stable && isPrerelease(version) && !stableTakesPrereleases(announced)) skipped.push({ name, announced, reason: "stable" });
    else write.push(name);
  }
  return { write, skipped };
}

// The macOS release is one universal build, so one archive serves both Mac platforms of the manifest: the keys the
// updater looks a Mac up by. Windows is the x64 installer.
export const DARWIN_PLATFORMS = ["darwin-aarch64", "darwin-x86_64"];
export const WINDOWS_PLATFORM = "windows-x86_64";
const MANIFEST_PLATFORMS = { macos: DARWIN_PLATFORMS, windows: [WINDOWS_PLATFORM] };

export function tag(version) {
  return `desktop-v${version}`;
}

// The name the update archive and its signature are published under: unlike the bundler's own
// `Octoboard.app.tar.gz`, it carries the version, so an address never changes meaning.
export function archiveName(productName, version) {
  return `${productName}_${version}_universal.app.tar.gz`;
}

// The name Tauri gives the NSIS installer, which is published as it is: it carries the version already.
export function windowsInstallerName(productName, version) {
  return `${productName}_${version}_x64-setup.exe`;
}

export function releaseAssetUrl(repositoryUrl, version, name) {
  return `${repositoryUrl.replace(/\/+$/, "")}/releases/download/${tag(version)}/${name}`;
}

// `owner/repo` of a GitHub repository address, as `gh --repo` takes it.
export function repositorySlug(repositoryUrl) {
  const match = /^https:\/\/github\.com\/([^/]+\/[^/]+?)\/?$/.exec(repositoryUrl);
  if (!match) throw new Error(`"${repositoryUrl}" is not a GitHub repository address.`);
  return match[1];
}

// What the versioned release holds for one platform, by name: `files`, published as they are (the macOS disk image and
// the update archive, or the Windows installer), `update`, the one of them the manifest points to, and `signature`, the
// update file's signature. `dmgName` is the disk image's name, which only the macOS build knows. `publish-release.mjs`
// publishes under these names, and `releaseAssets` tells a release's assets apart by them.
export function releaseLayout(platform, { productName, version, dmgName }) {
  if (platform === "macos") {
    const archive = archiveName(productName, version);
    return { files: [dmgName, archive], update: archive, signature: `${archive}.sig` };
  }
  const installer = windowsInstallerName(productName, version);
  return { files: [installer], update: installer, signature: `${installer}.sig` };
}

// Every asset name of one platform in the versioned release: its `releaseLayout` files and the signature.
export function releaseAssets(platform, names) {
  const { files, signature } = releaseLayout(platform, names);
  return [...files, signature];
}

// The platform an asset of the versioned release belongs to, null when it is neither platform's.
function assetPlatform(name, productName, version) {
  if (name.endsWith(".dmg")) return "macos";
  return RELEASE_PLATFORMS.find((platform) => releaseAssets(platform, { productName, version, dmgName: "" }).includes(name)) ?? null;
}

// What publishing does to the versioned release, given the names of the assets it has now (null when there is no
// such release) and the assets of each selected platform (`assets`, a map from platform). A platform with all its
// assets there is an earlier run's, and resumes; one with none is added to the release; one with only some, or any
// asset that is neither platform's, means the release is not what this script made, and `conflict` says how. The
// platform not selected is not looked at, so a release carrying only the other one is not in the way.
export function planReleaseAssets({ platforms, assets, existing, productName, version }) {
  if (existing === null) return { create: true, add: [], resume: [], conflict: null };
  const other = existing.filter((name) => assetPlatform(name, productName, version) === null);
  if (other.length > 0) return { create: false, add: [], resume: [], conflict: `with other assets (${other.join(", ")})` };
  const add = [];
  const resume = [];
  for (const platform of platforms) {
    const present = assets[platform].filter((name) => existing.includes(name));
    if (present.length === 0) add.push(platform);
    else if (present.length === assets[platform].length) resume.push(platform);
    else return { create: false, add: [], resume: [], conflict: `with some of the ${platform} assets but not all` };
  }
  return { create: false, add, resume, conflict: null };
}

// The oldest version a daemon adopts process holders from is a constant of the daemon (`FLOOR`).
export const HOLDER_FLOOR_FILE = "apps/daemon/src/holder/compat.rs";

// The `FLOOR` the daemon names in the tree at `root`, or undefined when the file has no such constant.
export function readHolderFloor(root) {
  const file = readFileSync(path.join(root, HOLDER_FLOOR_FILE), "utf8");
  return /pub const FLOOR: &str = "([^"]+)";/.exec(file)?.[1];
}

// The manifest the updater reads for one platform: the version, its notes and date and, for each platform key the
// build serves, the update's address and the signature, which is the content of its `.sig` file. `update` is the
// archive serving both Macs (`macos`) or the installer (`windows`). A manifest carries only its own platform's
// entries: the other platform's has a manifest of its own, which an entry kept from an older release would
// contradict. `holderFloor` is the build's holder floor (`readHolderFloor`), which the running application reads from
// the manifest to tell the sessions an update restart would interrupt: the holders older than it.
export function buildManifest({ version, notes, pubDate, platform, update, holderFloor }) {
  assertVersion(version);
  assertVersion(holderFloor);
  const keys = MANIFEST_PLATFORMS[platform];
  if (!keys) throw new Error(`"${platform}" is not a release platform.`);
  if (!update?.url || !update?.signature) throw new Error(`The manifest needs the ${platform} update's address and signature.`);
  const entry = { signature: update.signature.trim(), url: update.url };
  return {
    version,
    notes,
    pub_date: pubDate,
    holder_floor: holderFloor,
    platforms: Object.fromEntries(keys.map((key) => [key, entry])),
  };
}
