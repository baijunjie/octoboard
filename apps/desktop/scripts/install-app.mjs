#!/usr/bin/env node
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, statSync } from "node:fs";
import path from "node:path";
import { loadEnv } from "../../../scripts/env.mjs";
import { appDir, assertLayeredIcon, bundleDir, findBundle, tauriBuild, withoutUpdaterArtifacts } from "./bundle.mjs";

if (process.platform !== "darwin") throw new Error("build:install requires macOS.");
if (process.argv.length > 2) throw new Error("build:install takes no arguments; configure .env.install.local.");
loadEnv(".env.secret", ".env.install.local");

const installDir = process.env.OCTOBOARD_INSTALL_DIR;
const identity = process.env.APPLE_SIGNING_IDENTITY;
if (!installDir || !path.isAbsolute(installDir)) {
  throw new Error("Set OCTOBOARD_INSTALL_DIR to an absolute directory in .env.install.local.");
}
if (!identity || identity === "-") {
  throw new Error("Set APPLE_SIGNING_IDENTITY to a keychain certificate identity in .env.secret.");
}
const identities = execFileSync("security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" });
if (!identities.includes(`"${identity}"`)) throw new Error(`No valid signing identity found: ${identity}`);

// Local installation must never inherit notarization credentials, including from the calling shell.
const env = withoutUpdaterArtifacts(Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("APPLE_"))));
env.APPLE_SIGNING_IDENTITY = identity;
const config = JSON.parse(readFileSync(path.join(appDir, "src-tauri", "tauri.conf.json"), "utf8"));
const startedAt = Date.now();
tauriBuild(["--bundles", "app"], env);
const app = findBundle(path.join(bundleDir, "macos"), ".app");
if (statSync(app).mtimeMs < startedAt) throw new Error(`${app} predates this build.`);
assertLayeredIcon(app);
verify(app);

// Replacing a running shell also replaces its sidecar; leave quitting and ending sessions to the user.
const running = execFileSync("lsappinfo", ["find", `bundleid=${config.identifier}`], { encoding: "utf8" }).trim();
if (running) throw new Error("Build complete. Quit Octoboard before running build:install again; active sessions will end when you quit.");

mkdirSync(installDir, { recursive: true });
const destination = path.join(realpathSync(installDir), path.basename(app));
if (destination === realpathSync(app)) throw new Error("The install destination must differ from the build output.");
if (existsSync(destination) || isSymlink(destination)) {
  if (isSymlink(destination) || !lstatSync(destination).isDirectory()) {
    throw new Error(`Refusing to replace a symlink or non-directory: ${destination}`);
  }
  const installedId = execFileSync("/usr/libexec/PlistBuddy", ["-c", "Print :CFBundleIdentifier", path.join(destination, "Contents", "Info.plist")], { encoding: "utf8" }).trim();
  if (installedId !== config.identifier) throw new Error(`Refusing to replace another application: ${destination}`);
}

// Stage on the destination volume, keeping the old bundle available until the copy verifies.
const staging = mkdtempSync(path.join(path.dirname(destination), ".octoboard-install-"));
const stagedApp = path.join(staging, path.basename(app));
const backup = path.join(staging, "previous.app");
try {
  execFileSync("ditto", [app, stagedApp], { stdio: "inherit" });
  verify(stagedApp);
  if (existsSync(destination)) renameSync(destination, backup);
  try {
    renameSync(stagedApp, destination);
  } catch (error) {
    if (existsSync(backup)) renameSync(backup, destination);
    throw error;
  }
} finally {
  // A failed rollback must leave the backup available for recovery.
  if (existsSync(backup) && !existsSync(destination)) {
    console.error(`Previous application preserved at ${backup}`);
  } else {
    rmSync(staging, { recursive: true, force: true });
  }
}
console.log(`Installed ${destination} (signed locally; no notarization submitted).`);

function isSymlink(file) {
  try {
    return lstatSync(file).isSymbolicLink();
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

function verify(bundle) {
  execFileSync("codesign", ["--verify", "--deep", "--strict", bundle], { stdio: "inherit" });
  // codesign writes its display report to stderr even on success.
  const report = spawnSync("codesign", ["-dv", "--verbose=4", bundle], { encoding: "utf8" });
  if (report.error) throw report.error;
  if (report.status !== 0 || !report.stderr.split(/\r?\n/).includes(`Authority=${identity}`)) {
    throw new Error(`The bundle does not carry the configured signing identity: ${bundle}\n${report.stderr}`);
  }
}
