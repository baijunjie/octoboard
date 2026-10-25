import assert from "node:assert/strict";
import { test } from "node:test";

import {
  archiveName,
  assertVersion,
  buildManifest,
  compareVersions,
  isPrerelease,
  manifestFiles,
  manifestVersion,
  planManifestWrites,
  releaseAssetUrl,
  repositorySlug,
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

test("every release updates the beta manifest and only a stable one also the stable manifest", () => {
  assert.deepEqual(manifestFiles("0.2.0-beta.1"), ["beta.json"]);
  assert.deepEqual(manifestFiles("0.2.0"), ["beta.json", "stable.json"]);
});

test("the archive is published under a name carrying the version, for both architectures", () => {
  assert.equal(archiveName("Octoboard", "0.2.0-beta.1"), "Octoboard_0.2.0-beta.1_universal.app.tar.gz");
});

test("an asset's address is under the versioned release of the repository", () => {
  assert.equal(
    releaseAssetUrl("https://github.com/baijunjie/octoboard/", "0.2.0", "Octoboard_0.2.0_universal.app.tar.gz"),
    "https://github.com/baijunjie/octoboard/releases/download/v0.2.0/Octoboard_0.2.0_universal.app.tar.gz",
  );
});

test("a repository address gives its owner and name", () => {
  assert.equal(repositorySlug("https://github.com/baijunjie/octoboard"), "baijunjie/octoboard");
  assert.throws(() => repositorySlug("https://example.test/a/b"), /not a GitHub repository/);
});

test("the manifest names the version, notes, date and the one signed archive for both platforms", () => {
  const url = "https://github.com/baijunjie/octoboard/releases/download/v0.2.0/a.app.tar.gz";
  const manifest = buildManifest({ version: "0.2.0", notes: "Fixes.", pubDate: "2026-11-01T00:00:00.000Z", url, signature: "c2lnbmF0dXJl\n" });
  const entry = { signature: "c2lnbmF0dXJl", url };
  assert.deepEqual(manifest, {
    version: "0.2.0",
    notes: "Fixes.",
    pub_date: "2026-11-01T00:00:00.000Z",
    platforms: { "darwin-aarch64": entry, "darwin-x86_64": entry },
  });
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
  assert.deepEqual(planManifestWrites("0.2.0", {}), { write: ["beta.json", "stable.json"], skipped: [] });
  assert.deepEqual(planManifestWrites("0.2.0", { "beta.json": "0.1.0", "stable.json": "0.2.0" }), {
    write: ["beta.json", "stable.json"],
    skipped: [],
  });
  assert.deepEqual(planManifestWrites("0.2.0-beta.1", { "beta.json": "0.2.0-beta.2" }), {
    write: [],
    skipped: [{ name: "beta.json", announced: "0.2.0-beta.2" }],
  });
  assert.deepEqual(planManifestWrites("0.2.0", { "beta.json": "0.3.0-beta.1", "stable.json": "0.1.5" }), {
    write: ["stable.json"],
    skipped: [{ name: "beta.json", announced: "0.3.0-beta.1" }],
  });
});

test("a manifest's version is read, and one without a semantic version is refused", () => {
  assert.equal(manifestVersion('{"version":"0.2.0-beta.1"}'), "0.2.0-beta.1");
  for (const text of ["", "{", "{}", '{"version":2}', '{"version":"latest"}']) {
    assert.throws(() => manifestVersion(text), Error, text);
  }
});
