#!/usr/bin/env node
// Builds the app for local verification: the same `tauri build` as `pnpm release`, but in a child
// environment with every `APPLE_*` variable removed, so the result can never be Developer ID-signed
// or submitted to Apple's notarization service, whatever the calling shell carries. Extra arguments
// go to `tauri build` (`pnpm build:app --bundles app` skips the `.dmg`).
//
// The environment is filtered here, in JavaScript, and handed to `execFileSync` as an explicit
// `env`: no shell parses it, so no quoting or word-splitting rule can turn the stripping into a
// no-op. (Stripping with `UNSET=$(...)` and `env $UNSET ...` did exactly that in zsh, which does not
// split an unquoted parameter, and a "verification" build was signed and notarized.)
//
// It then *asserts* the outcome instead of trusting the stripping: a bundle that carries a
// Developer ID authority fails the script.
import { execFileSync, spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import path from "node:path";
import { appDir, bundleDir, findBundle, tauriBin } from "./bundle.mjs";

// Arguments that move the output away from `target/release/bundle` (or produce none) are refused up
// front, for a fast and actionable failure; the check on the bundle's age below is what catches the
// forms this does not list. `-d` and `-t` also match the clustered and attached forms (`-dv`,
// `-taarch64-...`); only the CLI's boolean short flags can precede them in a cluster, so an attached
// value elsewhere (`-fdefault`) is left alone rather than refused for the wrong reason.
const args = process.argv.slice(2);
const relocating = args.find(
  (arg) => /^--(debug|target|out-dir|no-bundle)(=|$)/.test(arg) || /^-[vhV]*[dt]/.test(arg)
);
if (relocating) {
  throw new Error(
    `build:app only supports arguments that do not move target/release/bundle; got ${relocating}.`
  );
}

const env = Object.fromEntries(
  Object.entries(process.env).filter(([name]) => !name.startsWith("APPLE_"))
);

const startedAt = Date.now();
execFileSync(tauriBin, ["build", ...args], { cwd: appDir, env, stdio: "inherit" });

const app = findBundle(path.join(bundleDir, "macos"), ".app");
assertProducedSince(app, startedAt);
assertNoDeveloperId(app, spawnSync("codesign", ["-dv", "--verbose=4", app], { encoding: "utf8" }));
console.log(`${app} carries no Developer ID authority and is for local verification only.`);

// An argument this script does not know to refuse (the Tauri CLI has many) can leave the build's
// output elsewhere, or produce none, with an `.app` from an earlier build still sitting here; the
// Developer ID assertion would then pass on a bundle this build did not make.
function assertProducedSince(app, startedAt) {
  if (statSync(app).mtimeMs < startedAt) {
    throw new Error(
      `${app} predates this build, so this build did not produce it; check the arguments given.`
    );
  }
}

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
