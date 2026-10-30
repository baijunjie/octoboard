#!/usr/bin/env node
// Builds the app for local verification: the same `tauri build` as `pnpm release`, but in a child
// environment with every `APPLE_*` variable removed, so the result can never be Developer ID-signed
// or submitted to Apple's notarization service, whatever the calling shell carries, and one that
// makes no update archive. Extra arguments go to `tauri build` (`pnpm build:app --bundles app`
// skips the `.dmg`).
//
// It then *asserts* the outcome instead of trusting the stripping: a bundle that carries a
// Developer ID authority fails the script.
import { spawnSync } from "node:child_process";
import path from "node:path";
import {
  assertLayeredIcon,
  assertProducedSince,
  bundleDir,
  findBundle,
  tauriBuild,
  withoutAppleVariables,
  withoutUpdaterArtifacts,
} from "./bundle.mjs";

// Arguments that move the output away from `target/release/bundle` (or produce none) are refused up
// front, for a fast and actionable failure; the check on the bundle's age below is what catches the
// forms this does not list. `-d` and `-t` also match the clustered and attached forms (`-dv`,
// `-taarch64-...`); only the CLI's boolean short flags can precede them in a cluster, so an attached
// value elsewhere (`-fdefault`) is left alone rather than refused for the wrong reason.
const args = process.argv.slice(2);
const relocating = args.find(
  (arg) => /^--(debug|target|out-dir|no-bundle)(=|$)/.test(arg) || /^-[vhV]*[dt]/.test(arg),
);
if (relocating) {
  throw new Error(
    `build:app only supports arguments that do not move target/release/bundle; got ${relocating}.`,
  );
}

const startedAt = Date.now();
// The update signing key goes too, with the update archive that needs it (`withoutUpdaterArtifacts`).
tauriBuild(...withoutUpdaterArtifacts(args, withoutAppleVariables(process.env)));

const app = findBundle(path.join(bundleDir, "macos"), ".app");
assertProducedSince(app, startedAt);
// Only a warning: a local verification build is still useful without the layered icon.
try {
  assertLayeredIcon(app);
} catch (error) {
  console.warn(`warning: ${error.message}`);
}
assertNoDeveloperId(app, spawnSync("codesign", ["-dv", "--verbose=4", app], { encoding: "utf8" }));
console.log(`${app} carries no Developer ID authority and is for local verification only.`);

// `codesign -dv` reports on stderr, and exits non-zero for a bundle with no signature at all, which
// is a fine outcome here. What is not fine is the check never having run (no `codesign` to spawn, no
// Command Line Tools, an unreadable path) or having been cut short: that also leaves no Developer ID
// authority in the output, so the absence of one only counts once the output is a complete report.
function assertNoDeveloperId(app, result) {
  if (result.error) throw new Error(`could not run codesign on ${app}: ${result.error.message}`);
  if (result.signal) throw new Error(`codesign on ${app} was killed by ${result.signal}`);
  const report = result.stderr ?? "";
  if (!/code object is not signed at all|^Signature=|^CodeDirectory/m.test(report)) {
    throw new Error(`codesign gave no signature report for ${app}:\n${report}${result.stdout ?? ""}`);
  }
  if (/^Authority=Developer ID/m.test(report)) {
    throw new Error(`${app} is Developer ID-signed despite the APPLE_* stripping:\n${report}`);
  }
}
