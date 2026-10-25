// What `publish-release.mjs` decides without touching GitHub: which manifests a version goes into, the names
// and addresses of its assets, and the manifest itself, in the static format of Tauri's updater. Kept apart from
// the script so `release-manifest.test.mjs` can check it.

// The release that holds the manifests, under one fixed tag whatever version they announce. The application
// reads them from there (`UpdateChannel` in `src-tauri/src/updater.rs`).
export const MANIFEST_RELEASE_TAG = "updater";

// Semver, with a prerelease suffix (`-beta.1`) and build metadata optional.
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

export function assertVersion(version) {
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new Error(`"${version}" is not a semantic version (for example 0.2.0 or 0.2.0-beta.1).`);
  }
}

// A version with a prerelease suffix is not offered on the stable channel.
export function isPrerelease(version) {
  assertVersion(version);
  return version.includes("-");
}

// The manifest files a version is written to: every release updates the beta channel, so beta users are offered
// stable releases too, and only a release without a prerelease suffix also updates the stable channel.
export function manifestFiles(version) {
  return isPrerelease(version) ? ["beta.json"] : ["beta.json", "stable.json"];
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

// Splits the manifests `version` goes into by whether writing them would move a channel backwards: `current` maps a
// manifest's name to the version it announces now (absent when there is none yet). A manifest already announcing a
// newer version is left alone, so publishing an older release late never downgrades a channel's users.
export function planManifestWrites(version, current) {
  const write = [];
  const skipped = [];
  for (const name of manifestFiles(version)) {
    const announced = current[name];
    if (announced !== undefined && compareVersions(announced, version) > 0) skipped.push({ name, announced });
    else write.push(name);
  }
  return { write, skipped };
}

// The release is one universal build, so one archive serves both of the manifest's platforms: the keys the
// updater looks a Mac up by.
export const PLATFORMS = ["darwin-aarch64", "darwin-x86_64"];

export function tag(version) {
  return `v${version}`;
}

// The name the update archive and its signature are published under: unlike the bundler's own
// `Octoboard.app.tar.gz`, it carries the version, so an address never changes meaning.
export function archiveName(productName, version) {
  return `${productName}_${version}_universal.app.tar.gz`;
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

// The manifest the updater reads: the version, its notes and date and, for each platform, the archive's address and
// the signature, which is the content of its `.sig` file.
export function buildManifest({ version, notes, pubDate, url, signature }) {
  assertVersion(version);
  const entry = { signature: signature.trim(), url };
  return { version, notes, pub_date: pubDate, platforms: Object.fromEntries(PLATFORMS.map((platform) => [platform, entry])) };
}
