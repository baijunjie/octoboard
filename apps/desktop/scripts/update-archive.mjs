// The update archive the in-app update downloads: `<name>.app.tar.gz`, packed from the built `.app` and signed with
// the update key. `tauri build` does neither (`createUpdaterArtifacts` is off in `tauri.conf.json`), so that it never
// holds the key; the release scripts call these after it.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import { appDir, tauriBin } from "./bundle.mjs";

// The system `tar` (bsdtar) by path: its flags below are bsdtar's, and a GNU `tar` earlier on the PATH rejects them.
const TAR = "/usr/bin/tar";

// Packs the `.app` into the update archive next to it, `<name>.app.tar.gz`, with the `.app` directory as its one root
// entry, which is the shape the updater unpacks. Extended attributes, ACLs and file flags are left out by the flags
// (`COPYFILE_DISABLE` alone only stops the `._` entries; bsdtar still writes xattrs as pax headers). Entries of other
// kinds than file, directory and symlink are refused: bsdtar writes a hard link for files sharing an inode, and the
// updater's unpack resolves a hard link against its working directory and would fail. Returns the archive's path.
export function createUpdateArchive(app) {
  const archive = `${app}.tar.gz`;
  const packFlags = ["--no-xattrs", "--no-acls", "--no-fflags", "--no-mac-metadata"];
  execFileSync(TAR, [...packFlags, "-czf", archive, "-C", path.dirname(app), path.basename(app)], {
    env: { ...process.env, COPYFILE_DISABLE: "1" },
    stdio: "inherit",
  });
  const listing = execFileSync(TAR, ["-tvzf", archive], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  // The first character of a `-tv` line is the entry's type; a hard link is `h`.
  const unsupported = listing.split("\n").filter((line) => line !== "" && !"-dl".includes(line[0]));
  if (unsupported.length > 0) {
    rmSync(archive, { force: true });
    throw new Error(
      `${app} holds entries the updater cannot unpack (only files, directories and symlinks are allowed):\n` +
        unsupported.slice(0, 5).join("\n"),
    );
  }
  return archive;
}

// Signs the update archive with the update key from `TAURI_SIGNING_PRIVATE_KEY` (the key file's path or the key
// itself) and, when the key has a password, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, by `tauri signer sign`: the one step
// that reads the key. It writes `<archive>.sig` and returns its path. For a key made with an empty password the
// variable is set but empty, which the CLI counts as given (checked: it signs without prompting).
export function signUpdateArchive(archive, version) {
  const key = process.env.TAURI_SIGNING_PRIVATE_KEY;
  const signature = `${archive}.sig`;
  rmSync(signature, { force: true });
  const env = {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
  };
  const keyArgs = existsSync(key) ? ["--private-key-path", key] : [];
  if (keyArgs.length > 0) delete env.TAURI_SIGNING_PRIVATE_KEY;
  execFileSync(tauriBin, ["signer", "sign", ...keyArgs, "--app-version", version, archive], {
    cwd: appDir,
    env,
    stdio: "inherit",
  });
  // The signature was removed above, so its existence means this run wrote it.
  if (!existsSync(signature)) throw new Error(`\`tauri signer sign\` did not write ${signature}.`);
  return signature;
}

// Removes the update archives and signatures of earlier builds from `dir`. Their names carry no version, and an
// unsigned build makes none, so one left over would be taken for the new build's.
export function removeUpdateArchives(dir) {
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    if (name.includes(".app.tar.gz")) rmSync(path.join(dir, name), { force: true });
  }
}
