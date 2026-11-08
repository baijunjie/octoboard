import assert from "node:assert/strict";
import { test } from "node:test";

import {
  archiveName,
  assertVersion,
  buildManifest,
  compareVersions,
  isPrerelease,
  manifestFiles,
  manifestNames,
  manifestVersion,
  parsePlatforms,
  planManifestWrites,
  planReleaseAssets,
  platformsTrailer,
  readTagMessage,
  releaseAssets,
  releaseLayout,
  releaseAssetUrl,
  repositorySlug,
  stableTakesPrereleases,
  windowsInstallerName,
} from "./release-manifest.mjs";

test("a version with a prerelease suffix is a prerelease, one without is not", () => {
  assert.equal(isPrerelease("0.2.0-beta.1"), true);
  assert.equal(isPrerelease("1.0.0-rc.2+build.5"), true);
  assert.equal(isPrerelease("0.2.0"), false);
  assert.equal(isPrerelease("1.0.0+build.5"), false);
});

test("something that is not a semantic version is refused", () => {
  for (const version of ["1.0", "v1.0.0", "1.0.0-", "", undefined, "1.0.0 beta"]) {
    assert.throws(() => assertVersion(version), /not a semantic version/, String(version));
  }
});

test("each platform has a beta and a stable manifest", () => {
  assert.deepEqual(manifestFiles("macos"), ["beta.json", "stable.json"]);
  assert.deepEqual(manifestFiles("windows"), ["beta-windows.json", "stable-windows.json"]);
  assert.deepEqual(manifestNames("windows"), { beta: "beta-windows.json", stable: "stable-windows.json" });
  assert.throws(() => manifestFiles("linux"), /not a release platform/);
});

test("the original manifest names stay the Mac's, holding the darwin entries, and Windows has names of its own", () => {
  // Released macOS applications read `stable.json` and `beta.json` (`src-tauri/src/updater/macos.rs`), for good.
  const update = { url: "https://example.test/mac.app.tar.gz", signature: "bWFj" };
  const manifest = buildManifest({ version: "1.0.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", platform: "macos", update });
  assert.deepEqual(manifestFiles("macos"), ["beta.json", "stable.json"]);
  assert.deepEqual(Object.keys(manifest.platforms), ["darwin-aarch64", "darwin-x86_64"]);
  for (const name of manifestFiles("windows")) assert.ok(!["beta.json", "stable.json"].includes(name), name);
});

test("a platform selection is parsed in the order of the platforms, and nothing else is accepted", () => {
  assert.deepEqual(parsePlatforms(undefined), ["macos", "windows"]);
  assert.deepEqual(parsePlatforms("macos"), ["macos"]);
  assert.deepEqual(parsePlatforms("windows"), ["windows"]);
  assert.deepEqual(parsePlatforms("windows,macos"), ["macos", "windows"]);
  for (const selection of ["", "linux", "macos,", "macos,macos", "macos windows", "MACOS", "both"]) {
    assert.throws(() => parsePlatforms(selection), /not a platform selection/, selection);
  }
});

test("a tag message carries a single platform on its last line, and no line means both", () => {
  assert.equal(platformsTrailer(["macos", "windows"]), null);
  assert.equal(platformsTrailer(["windows"]), "Platforms: windows");
  assert.deepEqual(readTagMessage("## Fixes\n\n- A fix.\n\nPlatforms: macos\n"), { notes: "## Fixes\n\n- A fix.", platforms: ["macos"] });
  assert.deepEqual(readTagMessage("Notes\n\nPlatforms: macos,windows"), { notes: "Notes", platforms: ["macos", "windows"] });
  assert.deepEqual(readTagMessage("Notes\n"), { notes: "Notes", platforms: ["macos", "windows"] });
  assert.deepEqual(readTagMessage("Platforms: macos handled earlier\nNotes"), { notes: "Platforms: macos handled earlier\nNotes", platforms: ["macos", "windows"] });
  assert.throws(() => readTagMessage("Notes\n\nPlatforms: linux\n"), /not a platform selection/);
});

test("the archive is published under a name carrying the version, for both architectures", () => {
  assert.equal(archiveName("Octoboard", "0.2.0-beta.1"), "Octoboard_0.2.0-beta.1_universal.app.tar.gz");
});

test("an asset's address is under the versioned release of the repository", () => {
  assert.equal(
    releaseAssetUrl("https://github.com/baijunjie/octoboard/", "0.2.0", "Octoboard_0.2.0_universal.app.tar.gz"),
    "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/Octoboard_0.2.0_universal.app.tar.gz",
  );
});

test("a repository address gives its owner and name", () => {
  assert.equal(repositorySlug("https://github.com/baijunjie/octoboard"), "baijunjie/octoboard");
  assert.throws(() => repositorySlug("https://example.test/a/b"), /not a GitHub repository/);
});

test("the Windows installer is published under the name the bundler gives it", () => {
  assert.equal(windowsInstallerName("Octoboard", "0.2.0-beta.1"), "Octoboard_0.2.0-beta.1_x64-setup.exe");
});

test("a Mac manifest names the archive for both Macs and nothing else", () => {
  const update = { url: "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/a.app.tar.gz", signature: "c2lnbmF0dXJl\n" };
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", platform: "macos", update });
  const entry = { signature: "c2lnbmF0dXJl", url: update.url };
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    platforms: { "darwin-aarch64": entry, "darwin-x86_64": entry },
  });
});

test("a Windows manifest names the installer for windows-x86_64 and nothing else", () => {
  const update = { url: "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/a-setup.exe", signature: "d2luZG93cw==\n" };
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", platform: "windows", update });
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    platforms: { "windows-x86_64": { signature: "d2luZG93cw==", url: update.url } },
  });
});

test("a manifest without its update's address or signature, or for another platform, is refused", () => {
  const base = { version: "0.2.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z" };
  const update = { url: "https://example.test/a", signature: "c2ln" };
  assert.throws(() => buildManifest({ ...base, platform: "windows" }), /windows update/);
  assert.throws(() => buildManifest({ ...base, platform: "macos", update: { url: update.url, signature: "" } }), /macos update/);
  assert.throws(() => buildManifest({ ...base, platform: "linux", update }), /not a release platform/);
});

test("each platform's release assets are its files and the update file's signature", () => {
  const input = { productName: "Octoboard", version: "0.2.0", dmgName: "Octoboard_0.2.0_universal.dmg" };
  assert.deepEqual(releaseAssets("macos", input), [
    "Octoboard_0.2.0_universal.dmg",
    "Octoboard_0.2.0_universal.app.tar.gz",
    "Octoboard_0.2.0_universal.app.tar.gz.sig",
  ]);
  assert.deepEqual(releaseAssets("windows", input), ["Octoboard_0.2.0_x64-setup.exe", "Octoboard_0.2.0_x64-setup.exe.sig"]);
});

test("what is published is named by the release layout: the update file among the files, its signature beside them", () => {
  const input = { productName: "Octoboard", version: "0.2.0", dmgName: "Octoboard_0.2.0_universal.dmg" };
  assert.deepEqual(releaseLayout("macos", input), {
    files: ["Octoboard_0.2.0_universal.dmg", "Octoboard_0.2.0_universal.app.tar.gz"],
    update: "Octoboard_0.2.0_universal.app.tar.gz",
    signature: "Octoboard_0.2.0_universal.app.tar.gz.sig",
  });
  assert.deepEqual(releaseLayout("windows", input), {
    files: ["Octoboard_0.2.0_x64-setup.exe"],
    update: "Octoboard_0.2.0_x64-setup.exe",
    signature: "Octoboard_0.2.0_x64-setup.exe.sig",
  });
  for (const platform of ["macos", "windows"]) {
    const { files, update, signature } = releaseLayout(platform, input);
    assert.ok(files.includes(update));
    assert.deepEqual(releaseAssets(platform, input), [...files, signature]);
  }
});

test("the versioned release is created, extended with a missing platform or resumed, never mixed up", () => {
  const input = { productName: "Octoboard", version: "0.2.0" };
  const macos = releaseAssets("macos", { ...input, dmgName: "Octoboard_0.2.0_universal.dmg" });
  const windows = releaseAssets("windows", input);
  const both = { platforms: ["macos", "windows"], assets: { macos, windows }, ...input };
  const noConflict = { conflict: null };
  assert.deepEqual(planReleaseAssets({ ...both, existing: null }), { create: true, add: [], resume: [], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...both, existing: [...macos, ...windows] }), { create: false, add: [], resume: ["macos", "windows"], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...both, existing: macos }), { create: false, add: ["windows"], resume: ["macos"], ...noConflict });
  // A platform that is not selected is not looked at: the other one's release is not in the way.
  const windowsOnly = { platforms: ["windows"], assets: { windows }, ...input };
  assert.deepEqual(planReleaseAssets({ ...windowsOnly, existing: macos }), { create: false, add: ["windows"], resume: [], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...windowsOnly, existing: [...macos, ...windows] }), { create: false, add: [], resume: ["windows"], ...noConflict });
  assert.match(planReleaseAssets({ ...both, existing: [...macos, windows[0]] }).conflict, /some of the windows assets/);
  assert.match(planReleaseAssets({ ...both, existing: [...macos, "notes.txt"] }).conflict, /other assets \(notes.txt\)/);
  assert.match(planReleaseAssets({ ...both, existing: [windows[0], "Octoboard_0.2.0_universal.dmg", "x.tar.gz"] }).conflict, /other assets/);
});

test("versions are ordered by semver, a prerelease before its release", () => {
  const ascending = [
    "0.9.0",
    "0.10.0",
    "1.0.0-alpha",
    "1.0.0-alpha.1",
    "1.0.0-alpha.beta",
    "1.0.0-beta",
    "1.0.0-beta.2",
    "1.0.0-beta.11",
    "1.0.0-rc.1",
    "1.0.0",
    "1.0.1",
  ];
  for (let i = 0; i < ascending.length; i += 1) {
    for (let j = 0; j < ascending.length; j += 1) {
      assert.equal(Math.sign(compareVersions(ascending[i], ascending[j])), Math.sign(i - j), `${ascending[i]} vs ${ascending[j]}`);
    }
  }
  assert.equal(compareVersions("1.0.0+build.1", "1.0.0+build.2"), 0);
  assert.equal(compareVersions("1.0.0-rc.1+a", "1.0.0-rc.1"), 0);
});

test("a manifest already announcing a newer version is not written", () => {
  assert.deepEqual(planManifestWrites("0.2.0", "macos", {}), { write: ["beta.json", "stable.json"], skipped: [] });
  assert.deepEqual(planManifestWrites("0.2.0", "macos", { "beta.json": "0.1.0", "stable.json": "0.2.0" }), {
    write: ["beta.json", "stable.json"],
    skipped: [],
  });
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", "macos", { "beta.json": "0.2.0-beta.2", "stable.json": "0.2.0-beta.2" }), {
    write: [],
    skipped: [
      { name: "beta.json", announced: "0.2.0-beta.2", reason: "newer" },
      { name: "stable.json", announced: "0.2.0-beta.2", reason: "newer" },
    ],
  });
  // A newer stable version is skipped as newer, whatever the bridge says.
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", "windows", { "stable-windows.json": "0.2.0" }), {
    write: ["beta-windows.json"],
    skipped: [{ name: "stable-windows.json", announced: "0.2.0", reason: "newer" }],
  });
  assert.deepEqual(planManifestWrites("0.2.0", "macos", { "beta.json": "0.3.0-beta.1", "stable.json": "0.1.5" }), {
    write: ["stable.json"],
    skipped: [{ name: "beta.json", announced: "0.3.0-beta.1", reason: "newer" }],
  });
});

test("each platform's manifests move on their own: the other platform's versions do not count", () => {
  const current = { "beta.json": "0.3.0", "stable.json": "0.3.0" };
  assert.deepEqual(planManifestWrites("0.2.0", "windows", current), { write: ["beta-windows.json", "stable-windows.json"], skipped: [] });
  assert.deepEqual(planManifestWrites("0.2.0", "windows", { ...current, "stable-windows.json": "0.2.1" }), {
    write: ["beta-windows.json"],
    skipped: [{ name: "stable-windows.json", announced: "0.2.1", reason: "newer" }],
  });
});

test("until a stable version is out, a prerelease goes to the stable manifest too, and after it no longer", () => {
  // The first betas follow Stable by default and read only the stable manifests, which hold nothing yet.
  assert.deepEqual(planManifestWrites("0.1.0-beta.4", "macos", { "beta.json": "0.1.0-beta.3" }), {
    write: ["beta.json", "stable.json"],
    skipped: [],
  });
  assert.deepEqual(planManifestWrites("0.1.0-beta.5", "windows", { "beta-windows.json": "0.1.0-beta.4", "stable-windows.json": "0.1.0-beta.4" }), {
    write: ["beta-windows.json", "stable-windows.json"],
    skipped: [],
  });
  // A later run of the same version resumes it.
  assert.deepEqual(planManifestWrites("0.1.0-beta.4", "macos", { "beta.json": "0.1.0-beta.4", "stable.json": "0.1.0-beta.4" }).write, [
    "beta.json",
    "stable.json",
  ]);
  // The first stable release replaces the prerelease, the usual ordering putting it after its betas.
  assert.deepEqual(planManifestWrites("0.1.0", "macos", { "beta.json": "0.1.0-beta.5", "stable.json": "0.1.0-beta.5" }).write, [
    "beta.json",
    "stable.json",
  ]);
  // From then on the stable manifest takes stable versions only, a prerelease newer than it included.
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", "macos", { "beta.json": "0.1.0", "stable.json": "0.1.0" }), {
    write: ["beta.json"],
    skipped: [{ name: "stable.json", announced: "0.1.0", reason: "stable" }],
  });
  assert.deepEqual(planManifestWrites("0.3.0-rc.1", "windows", { "stable-windows.json": "0.2.0+build.7" }), {
    write: ["beta-windows.json"],
    skipped: [{ name: "stable-windows.json", announced: "0.2.0+build.7", reason: "stable" }],
  });
  // A stable release older than a beta Stable already announces does not replace it: the bridge goes on until a
  // newer stable release.
  assert.deepEqual(planManifestWrites("0.1.0", "macos", { "beta.json": "0.2.0-beta.1", "stable.json": "0.2.0-beta.1" }), {
    write: [],
    skipped: [
      { name: "beta.json", announced: "0.2.0-beta.1", reason: "newer" },
      { name: "stable.json", announced: "0.2.0-beta.1", reason: "newer" },
    ],
  });
  assert.deepEqual(planManifestWrites("0.2.0-beta.2", "macos", { "stable.json": "0.2.0-beta.1" }).write, ["beta.json", "stable.json"]);
  // Each platform decides by its own stable manifest.
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", "windows", { "stable.json": "0.1.0", "stable-windows.json": "0.1.0-beta.5" }).write, [
    "beta-windows.json",
    "stable-windows.json",
  ]);
});

test("a stable manifest takes prereleases while it is missing or announces a prerelease", () => {
  assert.equal(stableTakesPrereleases(undefined), true);
  assert.equal(stableTakesPrereleases("0.1.0-beta.3"), true);
  assert.equal(stableTakesPrereleases("0.1.0"), false);
});

test("a manifest's version is read, and one without a semantic version is refused", () => {
  assert.equal(manifestVersion('{"version":"0.2.0-beta.1"}'), "0.2.0-beta.1");
  for (const text of ["", "{", "{}", '{"version":2}', '{"version":"latest"}']) {
    assert.throws(() => manifestVersion(text), Error, text);
  }
});
