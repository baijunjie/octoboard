#!/usr/bin/env node
// Builds the distributable macOS bundle: `tauri build`, whose own `beforeBuildCommand` (see
// `tauri.conf.json`) builds the daemon sidecar and the frontend first, then produces the `.app`
// and, per `bundle.targets`, the `.dmg` around it. Signing and notarization are driven by the
// `APPLE_*` environment variables the bundled Tauri CLI itself reads — this script does not pass
// credentials to it directly — but the CLI's own failure mode for a half-configured environment is
// to either skip signing silently or fail deep inside a bundling step, neither of which says what
// to set. So this script checks the environment itself first, with one actionable message, before
// spending the minutes a full build takes. The credentials come from the gitignored `.env.secret`
// (see `.env.secret.example`), loaded below, or from the environment it is started in.
//
// It also *verifies* the result rather than trusting that signing/notarization happened just
// because the right variables were set: see `verify()` below.
import { execFileSync } from "node:child_process";
import path from "node:path";
import { loadEnv } from "../../../scripts/env.mjs";
import { appDir, bundleDir, findBundle, tauriBin } from "./bundle.mjs";

// The only script that reads `.env.secret`, and nothing else: a `VITE_` entry in `.env` would
// reach the bundle `tauri build` makes. The variables are then in the Tauri CLI's inherited
// environment.
loadEnv(".env.secret");

// Three states, in increasing order of what Gatekeeper will accept:
//
// 1. No `APPLE_SIGNING_IDENTITY` at all — the only mode anyone can run on a machine with no
//    Apple Developer ID certificate installed. Produces an unsigned `.app`/`.dmg`; the
//    signing-dependent verifications below are skipped and that is announced, not silently
//    assumed.
// 2. `APPLE_SIGNING_IDENTITY` set but the App Store Connect API key trio incomplete — refused here,
//    loudly, rather than handed to `tauri build` to fail on in whatever way it fails on it: a
//    signed-but-not-notarized `.dmg` still trips Gatekeeper on a downloaded copy, so half this
//    configuration is not a useful state to build in.
// 3. Both present — signed and notarized, stapled, the actual release artifact.
const signingIdentity = process.env.APPLE_SIGNING_IDENTITY;
const hasAppleApiKeyAuth =
  process.env.APPLE_API_KEY && process.env.APPLE_API_ISSUER && process.env.APPLE_API_KEY_PATH;

// The Tauri CLI reads `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` as a notarization credential
// too, and which set it prefers is undocumented. This project notarizes through the API key only
// (`notarizeDiskImage` below uses it for the `.dmg`), so with an Apple ID present the `.app` could
// come out notarized under it and the `.dmg` under the key: one build, two identities. Only the
// first two are refused — `APPLE_TEAM_ID` cannot select that route on its own, so its presence is
// harmless. Refused rather than dropped from the child environment, so a credential somebody set on
// purpose is never discarded silently.
//
// Presence is what counts here, not a non-empty value: the CLI reads these through Rust, which
// reports an exported-but-empty `APPLE_ID=` as present, so it would reach the CLI as a credential
// that a truthiness test had waved through. The loader instead treats an empty value as unset, so a
// template's `KEY=` cannot become an empty credential. The two rules answer different questions and
// must not be unified.
const appleIdVariables = ["APPLE_ID", "APPLE_PASSWORD"].filter((name) =>
  Object.hasOwn(process.env, name)
);
if (appleIdVariables.length > 0) {
  throw new Error(
    `${appleIdVariables.join(" and ")} found in the environment. This project notarizes ` +
      "through the App Store Connect API key only (APPLE_API_KEY + APPLE_API_ISSUER + " +
      "APPLE_API_KEY_PATH), and a second credential set could notarize the .app and the .dmg " +
      "under different identities. Unset APPLE_ID and APPLE_PASSWORD, or remove them from " +
      ".env.secret, for `pnpm release`. See apps/desktop/README.md."
  );
}

let mode;
if (!signingIdentity) {
  mode = "unsigned";
  console.log(
    "APPLE_SIGNING_IDENTITY is not set — building unsigned. Gatekeeper will block this on " +
      "another machine; see the \"Release builds\" section of apps/desktop/README.md for what to " +
      "set to produce a signed, notarized build, and .env.secret.example for the credentials."
  );
} else if (!hasAppleApiKeyAuth) {
  // The Tauri CLI itself also accepts `APPLE_API_KEY` + `APPLE_API_ISSUER` without
  // `APPLE_API_KEY_PATH`, resolving the `.p8` from one of the well-known key directories — that is
  // not good enough here because `notarizeDiskImage` below passes `--key` straight to
  // `notarytool`, which needs an actual path, not a lookup to perform itself.
  throw new Error(
    "APPLE_SIGNING_IDENTITY is set but the App Store Connect API key is incomplete. Set " +
      "APPLE_API_KEY + APPLE_API_ISSUER + APPLE_API_KEY_PATH (this script's own notarization step " +
      "needs the key's path, not just its id) — a signed-but-not-notarized build still fails " +
      "Gatekeeper on a downloaded copy, so this script refuses to produce one. See " +
      "apps/desktop/README.md."
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
function notarizeDiskImage(dmg) {
  console.log("notarizing the disk image (the Tauri CLI only notarizes the .app inside it):");
  const credentials = [
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
