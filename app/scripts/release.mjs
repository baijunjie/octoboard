#!/usr/bin/env node
// Builds the distributable macOS bundle: `tauri build`, whose own `beforeBuildCommand` (see
// `tauri.conf.json`) builds the daemon sidecar and the frontend first, then produces the `.app`
// and, per `bundle.targets`, the `.dmg` around it. Signing and notarization are driven by the
// `APPLE_*` environment variables the bundled Tauri CLI itself reads — this script does not pass
// credentials to it directly — but the CLI's own failure mode for a half-configured environment is
// to either skip signing silently or fail deep inside a bundling step, neither of which says what
// to export. So this script checks the environment itself first, with one actionable message,
// before spending the minutes a full build takes.
//
// It also *verifies* the result rather than trusting that signing/notarization happened just
// because the right variables were set: see `verify()` below.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const appDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const bundleDir = path.join(appDir, "src-tauri", "target", "release", "bundle");
const tauriBin = path.join(appDir, "node_modules", ".bin", "tauri");

// Three states, in increasing order of what Gatekeeper will accept:
//
// 1. No `APPLE_SIGNING_IDENTITY` at all — the only mode anyone can run on a machine with no
//    Apple Developer ID certificate installed. Produces an unsigned `.app`/`.dmg`; the
//    signing-dependent verifications below are skipped and that is announced, not silently
//    assumed.
// 2. `APPLE_SIGNING_IDENTITY` set but the notarization credential trio incomplete — refused here,
//    loudly, rather than handed to `tauri build` to fail on in whatever way it fails on it: a
//    signed-but-not-notarized `.dmg` still trips Gatekeeper on a downloaded copy, so half this
//    configuration is not a useful state to build in.
// 3. Both present — signed and notarized, stapled, the actual release artifact.
const signingIdentity = process.env.APPLE_SIGNING_IDENTITY;
const hasApplePasswordAuth =
  process.env.APPLE_ID && process.env.APPLE_PASSWORD && process.env.APPLE_TEAM_ID;
const hasAppleApiKeyAuth =
  process.env.APPLE_API_KEY && process.env.APPLE_API_ISSUER && process.env.APPLE_API_KEY_PATH;

let mode;
if (!signingIdentity) {
  mode = "unsigned";
  console.log(
    "APPLE_SIGNING_IDENTITY is not set — building unsigned. Gatekeeper will block this on " +
      "another machine; see the \"Release builds\" section of app/README.md for what to export " +
      "to produce a signed, notarized build."
  );
} else if (!hasApplePasswordAuth && !hasAppleApiKeyAuth) {
  // The Tauri CLI itself also accepts `APPLE_API_KEY` + `APPLE_API_ISSUER` without
  // `APPLE_API_KEY_PATH`, resolving the `.p8` from one of the well-known key directories — that is
  // not good enough here because `notarizeDiskImage` below passes `--key` straight to
  // `notarytool`, which needs an actual path, not a lookup to perform itself.
  throw new Error(
    "APPLE_SIGNING_IDENTITY is set but no complete notarization credential trio was found. Set " +
      "either APPLE_ID + APPLE_PASSWORD + APPLE_TEAM_ID or APPLE_API_KEY + APPLE_API_ISSUER + " +
      "APPLE_API_KEY_PATH (this script's own notarization step needs the key's path, not just " +
      "its id) — a signed-but-not-notarized build still fails Gatekeeper on a downloaded copy, " +
      "so this script refuses to produce one. See app/README.md."
  );
} else {
  mode = "notarized";
}

execFileSync(tauriBin, ["build"], { cwd: appDir, stdio: "inherit" });

verify();

// Confirms what the build actually produced instead of assuming it from the environment
// variables that went in — those only say what the Tauri CLI was *told* to do, not what it did.
function verify() {
  const app = findBundle(path.join(bundleDir, "macos"), ".app");
  const dmg = findBundle(path.join(bundleDir, "dmg"), ".dmg");
  console.log(`found app bundle: ${app}`);
  console.log(`found disk image: ${dmg}`);

  // `tauri build` signs the nested `octoboardd` sidecar itself, with the same identity and
  // hardened runtime it gives the bundle around it, so no sign-inside-out step is needed here.
  // This check stays because notarization rejects an app with any unsigned nested Mach-O, and
  // that would otherwise only surface as a notarization rejection minutes later, with nothing
  // pointing at which binary caused it.
  const sidecarPath = path.join(app, "Contents", "MacOS", "octoboardd");
  console.log(`checking the nested sidecar's signature (${sidecarPath}):`);
  runReporting("codesign", ["-dv", "--verbose=4", sidecarPath]);

  if (mode === "unsigned") {
    console.log("unsigned build — skipping codesign/spctl/stapler verification.");
    return;
  }

  console.log("verifying the app bundle's signature:");
  execFileSync("codesign", ["--verify", "--deep", "--strict", app], { stdio: "inherit" });
  console.log("checking Gatekeeper's own verdict on the app bundle:");
  execFileSync("spctl", ["-a", "-vvv", app], { stdio: "inherit" });

  notarizeDiskImage(dmg);
  console.log("validating the staple on the disk image:");
  execFileSync("xcrun", ["stapler", "validate", dmg], { stdio: "inherit" });
  console.log("checking Gatekeeper's own verdict on the disk image:");
  execFileSync(
    "spctl",
    ["-a", "-vvv", "-t", "open", "--context", "context:primary-signature", dmg],
    { stdio: "inherit" }
  );
}

// The Tauri CLI notarizes and staples the `.app` only — the `.dmg` it then builds around it comes
// out signed but unnotarized, and a *downloaded* one is therefore refused at mount time
// (`spctl`: "source=Unnotarized Developer ID") even though the app inside it is fine. So the disk
// image goes through notarization a second time, on its own. This submission is quick: the app
// inside is already notarized, so there is nothing new for Apple to scan.
//
// Apple ID first, matching the precedence `tauri build` itself uses to notarize the `.app` — a
// machine with both kinds of credentials set must notarize both artifacts under the same identity.
function notarizeDiskImage(dmg) {
  console.log("notarizing the disk image (the Tauri CLI only notarizes the .app inside it):");
  const credentials = hasApplePasswordAuth
    ? [
        "--apple-id",
        process.env.APPLE_ID,
        "--password",
        process.env.APPLE_PASSWORD,
        "--team-id",
        process.env.APPLE_TEAM_ID,
      ]
    : [
        "--key",
        process.env.APPLE_API_KEY_PATH,
        "--key-id",
        process.env.APPLE_API_KEY,
        "--issuer",
        process.env.APPLE_API_ISSUER,
      ];
  execFileSync("xcrun", ["notarytool", "submit", dmg, ...credentials, "--wait"], {
    stdio: "inherit",
  });
  console.log("stapling the notarization ticket to the disk image:");
  execFileSync("xcrun", ["stapler", "staple", dmg], { stdio: "inherit" });
}

// `codesign -dv` exits non-zero for an unsigned binary — expected and informative in unsigned
// mode, not a build failure — so this reports its output instead of letting a thrown error from
// `execFileSync` stop the script.
function runReporting(command, args) {
  try {
    execFileSync(command, args, { stdio: "inherit" });
  } catch {
    // Non-zero exit already printed its own diagnostic (codesign writes to stderr even when it
    // is only reporting "not signed at all"); nothing more to add.
  }
}

// Tauri's bundle output directory holds one file per target's extension, but the `.dmg` one is
// named with the product version — after a version bump without a clean rebuild, both the old and
// the new disk image can be sitting there together, and this would silently pick whichever
// `readdirSync` happens to return first.
function findBundle(dir, extension) {
  if (!existsSync(dir)) {
    throw new Error(`${dir} does not exist — did \`tauri build\` run its "${extension}" target?`);
  }
  const matches = readdirSync(dir).filter((name) => name.endsWith(extension));
  if (matches.length === 0) throw new Error(`no ${extension} file found in ${dir}`);
  if (matches.length > 1) {
    throw new Error(
      `found ${matches.length} "${extension}" files in ${dir} (${matches.join(", ")}) — clean ` +
        "src-tauri/target/release/bundle and rebuild so only the current version's is there."
    );
  }
  return path.join(dir, matches[0]);
}
