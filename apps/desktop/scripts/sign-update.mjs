#!/usr/bin/env node
// Signs the update archive `pnpm release --no-sign-update` left next to the universal `.app`, with the update key
// (`TAURI_SIGNING_PRIVATE_KEY`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for a key with a password; see
// `.env.secret.example`), and checks that a fresh `.sig` came out. It runs on its own so that the release build never
// holds the key: the release workflow gives it to this step only, so no build step has it in its environment. `pnpm
// release` without the flag does the same itself.
import path from "node:path";
import { loadEnv } from "../../../scripts/lib/env.mjs";
import { appVersion, findBundle, universalBundleDir as bundleDir } from "./bundle.mjs";
import { signUpdateArchive } from "./update-archive.mjs";

if (process.argv.length > 2) throw new Error("sign-update takes no arguments.");
loadEnv(".env.secret");
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "TAURI_SIGNING_PRIVATE_KEY is not set. Set it to the path of the update key file (or the key itself) in " +
      ".env.secret. See .env.secret.example and apps/desktop/scripts/README.md.",
  );
}

const macosDir = path.join(bundleDir, "macos");
const app = findBundle(macosDir, ".app");
const archive = findBundle(macosDir, ".app.tar.gz");
console.log(`signed ${signUpdateArchive(archive, appVersion(app))}`);
