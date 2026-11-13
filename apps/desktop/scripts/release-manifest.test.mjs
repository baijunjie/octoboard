import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import {
  archiveName,
  assertVersion,
  buildManifest,
  compareVersions,
  dmgName,
  HOLDER_FLOOR_FILE,
  isPrerelease,
  LINUX_ARCHES,
  LINUX_PLATFORMS,
  linuxArchiveName,
  manifestFiles,
  manifestNames,
  manifestVersion,
  parsePlatforms,
  planManifestWrites,
  planReleaseAssets,
  platformsTrailer,
  readHolderFloor,
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
  assert.deepEqual(manifestFiles("linux"), ["beta-linux.json", "stable-linux.json"]);
  assert.deepEqual(manifestNames("windows"), { beta: "beta-windows.json", stable: "stable-windows.json" });
  assert.deepEqual(manifestNames("linux"), { beta: "beta-linux.json", stable: "stable-linux.json" });
  assert.throws(() => manifestFiles("freebsd"), /not a release platform/);
});

test("the original manifest names stay the Mac's, holding the darwin entries, and Windows has names of its own", () => {
  // Released macOS applications read `stable.json` and `beta.json` (`src-tauri/src/updater/macos.rs`), for good.
  const update = { url: "https://example.test/mac.app.tar.gz", signature: "bWFj" };
  const entries = { "darwin-aarch64": update, "darwin-x86_64": update };
  const manifest = buildManifest({ version: "1.0.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", platform: "macos", entries, holderFloor: "0.1.0" });
  assert.deepEqual(manifestFiles("macos"), ["beta.json", "stable.json"]);
  assert.deepEqual(Object.keys(manifest.platforms), ["darwin-aarch64", "darwin-x86_64"]);
  for (const platform of ["windows", "linux"]) {
    for (const name of manifestFiles(platform)) assert.ok(!["beta.json", "stable.json"].includes(name), name);
  }
});

test("a platform selection is parsed in the order of the platforms, and nothing else is accepted", () => {
  assert.deepEqual(parsePlatforms(undefined), ["macos", "windows", "linux"]);
  assert.deepEqual(parsePlatforms("macos"), ["macos"]);
  assert.deepEqual(parsePlatforms("windows"), ["windows"]);
  assert.deepEqual(parsePlatforms("linux"), ["linux"]);
  assert.deepEqual(parsePlatforms("windows,macos"), ["macos", "windows"]);
  assert.deepEqual(parsePlatforms("linux,macos"), ["macos", "linux"]);
  assert.deepEqual(parsePlatforms("linux,windows,macos"), ["macos", "windows", "linux"]);
  for (const selection of ["", "freebsd", "macos,", "macos,macos", "macos windows", "MACOS", "both", "linux,linux"]) {
    assert.throws(() => parsePlatforms(selection), /not a platform selection/, selection);
  }
});

test("a tag message carries the selected platforms on its last line, and no line means all of them", () => {
  const all = ["macos", "windows", "linux"];
  assert.equal(platformsTrailer(all), null);
  assert.equal(platformsTrailer(["windows"]), "Platforms: windows");
  assert.equal(platformsTrailer(["linux"]), "Platforms: linux");
  assert.equal(platformsTrailer(["macos", "windows"]), "Platforms: macos,windows");
  assert.equal(platformsTrailer(["windows", "linux"]), "Platforms: windows,linux");
  assert.deepEqual(readTagMessage("## Fixes\n\n- A fix.\n\nPlatforms: macos\n"), { notes: "## Fixes\n\n- A fix.", platforms: ["macos"] });
  assert.deepEqual(readTagMessage("Notes\n\nPlatforms: macos,linux"), { notes: "Notes", platforms: ["macos", "linux"] });
  assert.deepEqual(readTagMessage("Notes\n\nPlatforms: macos,windows,linux"), { notes: "Notes", platforms: all });
  assert.deepEqual(readTagMessage("Notes\n"), { notes: "Notes", platforms: all });
  assert.deepEqual(readTagMessage("Platforms: macos handled earlier\nNotes"), { notes: "Platforms: macos handled earlier\nNotes", platforms: all });
  assert.throws(() => readTagMessage("Notes\n\nPlatforms: freebsd\n"), /not a platform selection/);
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

test("the disk image and the Linux archives are published under names carrying the version", () => {
  assert.equal(dmgName("Octoboard", "0.2.0-beta.1"), "Octoboard_0.2.0-beta.1_universal.dmg");
  assert.deepEqual(LINUX_ARCHES, ["x86_64", "aarch64"]);
  assert.deepEqual(LINUX_PLATFORMS, ["linux-x86_64", "linux-aarch64"]);
  assert.equal(linuxArchiveName("0.2.0", "x86_64"), "octoboardd_0.2.0_linux_x86_64.tar.gz");
  assert.equal(linuxArchiveName("0.2.0-beta.1", "aarch64"), "octoboardd_0.2.0-beta.1_linux_aarch64.tar.gz");
  assert.throws(() => linuxArchiveName("0.2.0", "armv7"), /not a Linux release architecture/);
});

test("a Mac manifest names the archive for both Macs and nothing else", () => {
  const update = { url: "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/a.app.tar.gz", signature: "c2lnbmF0dXJl\n" };
  const entries = { "darwin-aarch64": update, "darwin-x86_64": update };
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", platform: "macos", entries, holderFloor: "0.1.0" });
  const entry = { signature: "c2lnbmF0dXJl", url: update.url };
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    holder_floor: "0.1.0",
    platforms: { "darwin-aarch64": entry, "darwin-x86_64": entry },
  });
});

test("a Windows manifest names the installer for windows-x86_64 and nothing else", () => {
  const update = { url: "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0/a-setup.exe", signature: "d2luZG93cw==\n" };
  const entries = { "windows-x86_64": update };
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", platform: "windows", entries, holderFloor: "0.0.0" });
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    holder_floor: "0.0.0",
    platforms: { "windows-x86_64": { signature: "d2luZG93cw==", url: update.url } },
  });
});

test("a Linux manifest names each architecture's archive with its signature and SHA-256, and nothing else", () => {
  const base = "https://github.com/baijunjie/octoboard/releases/download/desktop-v0.2.0";
  const entries = {
    "linux-x86_64": { url: `${base}/octoboardd_0.2.0_linux_x86_64.tar.gz`, signature: "eDg2\n", sha256: "a".repeat(64) },
    "linux-aarch64": { url: `${base}/octoboardd_0.2.0_linux_aarch64.tar.gz`, signature: "YWFyY2g2NA==\n", sha256: "b".repeat(64) },
  };
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", platform: "linux", entries, holderFloor: "0.1.0" });
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    holder_floor: "0.1.0",
    platforms: {
      "linux-x86_64": { signature: "eDg2", url: entries["linux-x86_64"].url, sha256: "a".repeat(64) },
      "linux-aarch64": { signature: "YWFyY2g2NA==", url: entries["linux-aarch64"].url, sha256: "b".repeat(64) },
    },
  });
});

test("only the Linux manifest carries a SHA-256, which must be 64 lowercase hex digits there", () => {
  const base = { version: "0.2.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", holderFloor: "0.1.0" };
  const linuxEntries = (sha256) => ({
    "linux-x86_64": { url: "https://example.test/x", signature: "c2ln", sha256: "c".repeat(64) },
    "linux-aarch64": { url: "https://example.test/a", signature: "c2ln", sha256 },
  });
  for (const sha256 of [undefined, "", "c".repeat(63), "c".repeat(65), "C".repeat(64), "g".repeat(64)]) {
    assert.throws(() => buildManifest({ ...base, platform: "linux", entries: linuxEntries(sha256) }), /linux-aarch64 archive's SHA-256/, String(sha256));
  }
  const macEntry = { url: "https://example.test/m", signature: "c2ln", sha256: "c".repeat(64) };
  const mac = buildManifest({ ...base, platform: "macos", entries: { "darwin-aarch64": macEntry, "darwin-x86_64": macEntry } });
  for (const entry of Object.values(mac.platforms)) assert.deepEqual(entry, { signature: "c2ln", url: "https://example.test/m" });
  const windows = buildManifest({ ...base, platform: "windows", entries: { "windows-x86_64": macEntry } });
  assert.deepEqual(windows.platforms["windows-x86_64"], { signature: "c2ln", url: "https://example.test/m" });
});

test("a manifest without an update's address or signature, with keys of another platform, or for an unknown platform, is refused", () => {
  const base = { version: "0.2.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", holderFloor: "0.1.0" };
  const update = { url: "https://example.test/a", signature: "c2ln" };
  const linuxUpdate = { ...update, sha256: "d".repeat(64) };
  assert.throws(() => buildManifest({ ...base, platform: "windows" }), /windows-x86_64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "windows", entries: {} }), /windows-x86_64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "macos", entries: { "darwin-aarch64": update, "darwin-x86_64": { url: update.url, signature: "" } } }), /darwin-x86_64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "macos", entries: { "darwin-aarch64": update, "darwin-x86_64": { signature: "c2ln" } } }), /darwin-x86_64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "macos", entries: { "darwin-aarch64": update } }), /darwin-x86_64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "linux", entries: { "linux-x86_64": linuxUpdate } }), /linux-aarch64 update/);
  assert.throws(() => buildManifest({ ...base, platform: "windows", entries: { "windows-x86_64": update, "linux-x86_64": linuxUpdate } }), /linux-x86_64/);
  assert.throws(() => buildManifest({ ...base, platform: "freebsd", entries: { "windows-x86_64": update } }), /not a release platform/);
});

test("a manifest carries the build's holder floor, which must be a version", () => {
  const update = { url: "https://example.test/a", signature: "c2ln" };
  const base = { version: "0.2.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", platform: "windows", entries: { "windows-x86_64": update } };
  assert.equal(buildManifest({ ...base, holderFloor: "0.1.0" }).holder_floor, "0.1.0");
  assert.throws(() => buildManifest(base), /not a semantic version/);
  assert.throws(() => buildManifest({ ...base, holderFloor: "latest" }), /not a semantic version/);
});

test("the holder floor is read from the daemon's constant, and a tree without one has none", () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "octoboard-floor-"));
  try {
    mkdirSync(path.join(root, "apps/daemon/src/holder"), { recursive: true });
    const file = path.join(root, HOLDER_FLOOR_FILE);
    writeFileSync(file, 'pub const FLOOR: &str = "0.1.0";\n');
    assert.equal(readHolderFloor(root), "0.1.0");
    writeFileSync(file, "// no constant\n");
    assert.equal(readHolderFloor(root), undefined);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

const layoutInput = { productName: "Octoboard", version: "0.2.0" };
const linuxArchives = ["octoboardd_0.2.0_linux_x86_64.tar.gz", "octoboardd_0.2.0_linux_aarch64.tar.gz"];

test("each platform's release assets are its files and the update files' signatures", () => {
  assert.deepEqual(releaseAssets("macos", layoutInput), [
    "Octoboard_0.2.0_universal.dmg",
    "Octoboard_0.2.0_universal.app.tar.gz",
    "Octoboard_0.2.0_universal.app.tar.gz.sig",
  ]);
  assert.deepEqual(releaseAssets("windows", layoutInput), ["Octoboard_0.2.0_x64-setup.exe", "Octoboard_0.2.0_x64-setup.exe.sig"]);
  assert.deepEqual(releaseAssets("linux", layoutInput), [
    ...linuxArchives.flatMap((archive) => [archive, `${archive}.sha256`]),
    ...linuxArchives.map((archive) => `${archive}.sig`),
  ]);
});

test("what is published is named by the release layout: the files, and the updates the manifest points to with their keys and signatures", () => {
  assert.deepEqual(releaseLayout("macos", layoutInput), {
    files: ["Octoboard_0.2.0_universal.dmg", "Octoboard_0.2.0_universal.app.tar.gz"],
    updates: [
      {
        name: "Octoboard_0.2.0_universal.app.tar.gz",
        keys: ["darwin-aarch64", "darwin-x86_64"],
        signature: "Octoboard_0.2.0_universal.app.tar.gz.sig",
      },
    ],
  });
  assert.deepEqual(releaseLayout("windows", layoutInput), {
    files: ["Octoboard_0.2.0_x64-setup.exe"],
    updates: [{ name: "Octoboard_0.2.0_x64-setup.exe", keys: ["windows-x86_64"], signature: "Octoboard_0.2.0_x64-setup.exe.sig" }],
  });
  assert.deepEqual(releaseLayout("linux", layoutInput), {
    files: [
      "octoboardd_0.2.0_linux_x86_64.tar.gz",
      "octoboardd_0.2.0_linux_x86_64.tar.gz.sha256",
      "octoboardd_0.2.0_linux_aarch64.tar.gz",
      "octoboardd_0.2.0_linux_aarch64.tar.gz.sha256",
    ],
    updates: [
      { name: "octoboardd_0.2.0_linux_x86_64.tar.gz", keys: ["linux-x86_64"], signature: "octoboardd_0.2.0_linux_x86_64.tar.gz.sig" },
      { name: "octoboardd_0.2.0_linux_aarch64.tar.gz", keys: ["linux-aarch64"], signature: "octoboardd_0.2.0_linux_aarch64.tar.gz.sig" },
    ],
  });
  assert.throws(() => releaseLayout("freebsd", layoutInput), /not a release platform/);
  for (const platform of ["macos", "windows", "linux"]) {
    const { files, updates } = releaseLayout(platform, layoutInput);
    for (const update of updates) assert.ok(files.includes(update.name), update.name);
    assert.deepEqual(releaseAssets(platform, layoutInput), [...files, ...updates.map((update) => update.signature)]);
  }
});

test("the keys of a platform's updates are exactly the keys its manifest takes", () => {
  const entry = { url: "https://example.test/a", signature: "c2ln", sha256: "e".repeat(64) };
  for (const platform of ["macos", "windows", "linux"]) {
    const keys = releaseLayout(platform, layoutInput).updates.flatMap((update) => update.keys);
    const entries = Object.fromEntries(keys.map((key) => [key, entry]));
    const manifest = buildManifest({ version: "0.2.0", notes: "", pubDate: "2026-11-01T00:00:00.000Z", platform, entries, holderFloor: "0.1.0" });
    assert.deepEqual(Object.keys(manifest.platforms), keys, platform);
  }
});

test("the versioned release is created, extended with a missing platform or resumed, never mixed up", () => {
  const macos = releaseAssets("macos", layoutInput);
  const windows = releaseAssets("windows", layoutInput);
  const linux = releaseAssets("linux", layoutInput);
  const all = { platforms: ["macos", "windows", "linux"], assets: { macos, windows, linux }, ...layoutInput };
  const noConflict = { conflict: null };
  assert.deepEqual(planReleaseAssets({ ...all, existing: null }), { create: true, add: [], resume: [], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...all, existing: [...macos, ...windows, ...linux] }), {
    create: false,
    add: [],
    resume: ["macos", "windows", "linux"],
    ...noConflict,
  });
  assert.deepEqual(planReleaseAssets({ ...all, existing: macos }), { create: false, add: ["windows", "linux"], resume: ["macos"], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...all, existing: [...macos, ...linux] }), { create: false, add: ["windows"], resume: ["macos", "linux"], ...noConflict });
  // A platform that is not selected is not looked at: the other ones' release is not in the way.
  const windowsOnly = { platforms: ["windows"], assets: { windows }, ...layoutInput };
  assert.deepEqual(planReleaseAssets({ ...windowsOnly, existing: macos }), { create: false, add: ["windows"], resume: [], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...windowsOnly, existing: [...macos, ...windows] }), { create: false, add: [], resume: ["windows"], ...noConflict });
  const linuxOnly = { platforms: ["linux"], assets: { linux }, ...layoutInput };
  assert.deepEqual(planReleaseAssets({ ...linuxOnly, existing: [...macos, ...windows] }), { create: false, add: ["linux"], resume: [], ...noConflict });
  assert.deepEqual(planReleaseAssets({ ...linuxOnly, existing: linux }), { create: false, add: [], resume: ["linux"], ...noConflict });
  assert.match(planReleaseAssets({ ...all, existing: [...macos, windows[0]] }).conflict, /some of the windows assets/);
  assert.match(planReleaseAssets({ ...all, existing: [...macos, ...windows, linux[0]] }).conflict, /some of the linux assets/);
  assert.match(planReleaseAssets({ ...all, existing: [...macos, "notes.txt"] }).conflict, /other assets \(notes.txt\)/);
  assert.match(planReleaseAssets({ ...all, existing: [windows[0], "x.tar.gz"] }).conflict, /other assets/);
});

test("an asset belonging to no platform is a conflict, a disk image under another name than the bundler's included", () => {
  const macos = releaseAssets("macos", layoutInput);
  const input = { platforms: ["macos"], assets: { macos }, ...layoutInput };
  assert.match(planReleaseAssets({ ...input, existing: [...macos, "Octoboard_0.2.0_aarch64.dmg"] }).conflict, /other assets \(Octoboard_0.2.0_aarch64.dmg\)/);
  assert.match(planReleaseAssets({ ...input, existing: ["Octoboard.dmg"] }).conflict, /other assets/);
  assert.match(planReleaseAssets({ ...input, existing: ["octoboardd_0.2.0_linux_riscv64.tar.gz"] }).conflict, /other assets/);
  // Another version's assets are no platform's here either.
  assert.match(planReleaseAssets({ ...input, existing: releaseAssets("macos", { ...layoutInput, version: "0.1.0" }) }).conflict, /other assets/);
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

test("the Linux manifests move on their own, by the same rules", () => {
  const current = { "beta.json": "0.3.0", "stable.json": "0.3.0", "beta-windows.json": "0.3.0", "stable-windows.json": "0.3.0" };
  assert.deepEqual(planManifestWrites("0.2.0", "linux", current), { write: ["beta-linux.json", "stable-linux.json"], skipped: [] });
  assert.deepEqual(planManifestWrites("0.2.0", "linux", { ...current, "stable-linux.json": "0.2.1" }), {
    write: ["beta-linux.json"],
    skipped: [{ name: "stable-linux.json", announced: "0.2.1", reason: "newer" }],
  });
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", "linux", { "beta-linux.json": "0.1.0", "stable-linux.json": "0.1.0" }), {
    write: ["beta-linux.json"],
    skipped: [{ name: "stable-linux.json", announced: "0.1.0", reason: "stable" }],
  });
  assert.deepEqual(planManifestWrites("0.1.0-beta.1", "linux", {}), { write: ["beta-linux.json", "stable-linux.json"], skipped: [] });
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
