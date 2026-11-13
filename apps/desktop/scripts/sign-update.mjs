#!/usr/bin/env node
// Signs the update artifact `pnpm release --no-sign-update` left, on macOS the archive next to the universal `.app`, on
// Windows the NSIS installer and on Linux each of this version's daemon archives in `target/linux-release/`, with the
// update key (`TAURI_SIGNING_PRIVATE_KEY`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for a key with a password; see
// `.env.secret.example`), and checks that a fresh `.sig` came out. It runs on its own so that a build by hand can leave
// the key out of the build's environment; `pnpm release` without the flag does the same itself. The release workflow
// does not run it: its `sign` job signs the builds' files on a runner of its own, which runs none of the workspace's
// code (`.github/workflows/release.yml`).
import path from "node:path";
import { loadEnv } from "../../../scripts/lib/env.mjs";
import {
  appVersion,
  findBundle,
  packageVersion,
  universalBundleDir as bundleDir,
  windowsBundleDir,
} from "./bundle.mjs";
import { builtLinuxArchives, linuxReleaseDir } from "./linux-release.mjs";
import { signUpdateArchive } from "./update-archive.mjs";

if (process.argv.length > 2) throw new Error("sign-update takes no arguments.");
loadEnv(".env.secret");
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "TAURI_SIGNING_PRIVATE_KEY is not set. Set it to the path of the update key file (or the key itself) in " +
      ".env.secret. See .env.secret.example and apps/desktop/scripts/README.md.",
  );
}

if (process.platform === "linux") {
  const version = packageVersion();
  const archives = builtLinuxArchives(linuxReleaseDir, version);
  if (archives.length === 0) throw new Error(`${linuxReleaseDir} holds no ${version} archive. Run \`pnpm release --no-sign-update\` first.`);
  for (const { archive } of archives) console.log(`signed ${signUpdateArchive(archive, version)}`);
} else if (process.platform === "win32") {
  const setup = findBundle(path.join(windowsBundleDir, "nsis"), "-setup.exe");
  console.log(`signed ${signUpdateArchive(setup, packageVersion())}`);
} else {
  const macosDir = path.join(bundleDir, "macos");
  const app = findBundle(macosDir, ".app");
  const archive = findBundle(macosDir, ".app.tar.gz");
  console.log(`signed ${signUpdateArchive(archive, appVersion(app))}`);
}
