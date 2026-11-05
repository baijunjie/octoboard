// What the Windows release needs beyond the shared build pieces: the signing mode and its Tauri configuration, the
// Authenticode checks with `signtool`, the look inside the built installer, and the files `publish-release.mjs` takes
// from a Windows build. The parts that touch no tool are tested by `windows-release.test.mjs`.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { installerLayout, INSTALLER_DIR } from "../../../scripts/fetch-conpty.mjs";
import { repoRoot } from "./bundle.mjs";
import { windowsInstallerName } from "./release-manifest.mjs";

// How the installer and the executables in it are code-signed (`WINDOWS_SIGNING`):
// - `none`: not at all; Windows SmartScreen warns on the download.
// - `command`: `tauri build` signs every file it bundles by calling `WINDOWS_SIGN_COMMAND` once per file.
// - `signpath`: SignPath signs only files uploaded as GitHub artifacts, and every request is approved by hand, so
//   the release is built in stages with a submission after the first and after the second (see `STAGES`).
export const SIGNING_MODES = ["none", "command", "signpath"];

// The stages of a `signpath` release, run one by one by `release-windows.mjs --stage`: `build` compiles without
// bundling and leaves our own executables to submit, `bundle` takes them back signed and bundles the installer
// from them, `finish` takes the installer back signed and checks the result.
export const STAGES = ["build", "bundle", "finish"];

// Where `--stage build` and `--stage bundle` leave the files to submit to SignPath.
export const submissionDir = path.join(repoRoot, "target", "signpath");

// Microsoft's own files the installer carries (see `scripts/fetch-conpty.mjs`). They keep Microsoft's signature, so
// no mode signs them again.
const MICROSOFT_FILES = ["conpty.dll", "openconsole.exe"];

export function isMicrosoftFile(file) {
  return MICROSOFT_FILES.includes(path.win32.basename(file).toLowerCase());
}

// The signing mode `env` asks for. A variable that is unset or empty is `none`: CI passes an unset repository
// variable as an empty string.
export function signingMode(env) {
  const mode = (env.WINDOWS_SIGNING ?? "").trim() || "none";
  if (!SIGNING_MODES.includes(mode)) {
    throw new Error(`WINDOWS_SIGNING is "${mode}"; it is one of ${SIGNING_MODES.join(", ")}.`);
  }
  if (mode === "command" && !(env.WINDOWS_SIGN_COMMAND ?? "").trim()) {
    throw new Error(
      "WINDOWS_SIGNING=command needs WINDOWS_SIGN_COMMAND: the command line that signs one file, with %1 where the " +
        "file's path goes. See apps/desktop/scripts/README.md.",
    );
  }
  return mode;
}

// Checks the `--stage` and `--signed` options against the mode and returns the stage (null outside `signpath`).
export function checkStage(mode, stage, signed) {
  if (mode !== "signpath") {
    if (stage !== undefined || signed !== undefined) {
      throw new Error(`--stage and --signed belong to WINDOWS_SIGNING=signpath, and the mode is ${mode}.`);
    }
    return null;
  }
  if (!STAGES.includes(stage)) throw new Error(`WINDOWS_SIGNING=signpath needs --stage ${STAGES.join("|")}.`);
  if (stage !== "build" && signed === undefined) {
    throw new Error(`--stage ${stage} needs --signed <directory>, where SignPath's signed files are.`);
  }
  if (stage === "build" && signed !== undefined) throw new Error("--stage build takes no --signed directory.");
  return stage;
}

// The arguments that make `tauri build` call `signFile` (a script, run by `process.execPath`) for every file it
// signs. They go on the command line, not into `tauri.windows.conf.json`: the command is the signer's own business.
// `signFile` reads it from `WINDOWS_SIGN_COMMAND` rather than taking it as an argument, because Tauri replaces
// every `%1` in the arguments, the command's own among them.
export function signConfigArgs(mode, signFile) {
  if (mode !== "command") return [];
  const signCommand = { cmd: process.execPath, args: [signFile, "%1"] };
  return ["--config", JSON.stringify({ bundle: { windows: { signCommand } } })];
}

// Splits a command line into its arguments at whitespace; a double-quoted part is one argument (or part of one) with
// the quotes removed. There are no escapes, as in Tauri's own string notation for a sign command.
export function splitCommandLine(line) {
  const args = [];
  let current = null;
  let quoted = false;
  for (const char of line) {
    if (char === '"') {
      quoted = !quoted;
      current ??= "";
    } else if (!quoted && /\s/.test(char)) {
      if (current !== null) args.push(current);
      current = null;
    } else {
      current = (current ?? "") + char;
    }
  }
  if (quoted) throw new Error(`The command line has an unclosed double quote: ${line}`);
  if (current !== null) args.push(current);
  return args;
}

// The program and arguments that sign `file` with the command line `line`, `%1` replaced by the file's path; null
// for a file no mode signs. A command line without `%1` would sign nothing in particular and is refused.
export function signInvocation(line, file) {
  if (isMicrosoftFile(file)) return null;
  const args = splitCommandLine(line);
  if (args.length === 0 || !args.some((arg) => arg.includes("%1"))) {
    throw new Error(`WINDOWS_SIGN_COMMAND needs %1 where the file's path goes, and has none: ${line}`);
  }
  const [cmd, ...rest] = args.map((arg) => arg.replaceAll("%1", file));
  return { cmd, args: rest };
}

// The first certificate `signtool verify /v` lists, the one that signed the file, or null.
export function signerOf(report) {
  const match = /Signing Certificate Chain:[\s\S]*?Issued to: (.+)/.exec(report);
  return match ? match[1].trim() : null;
}

// The path of `signtool.exe`: on the PATH, else the newest Windows SDK's x64 one; null when there is none.
export function findSigntool(env = process.env) {
  for (const dir of (env.PATH ?? env.Path ?? "").split(path.delimiter)) {
    if (dir && existsSync(path.join(dir, "signtool.exe"))) return path.join(dir, "signtool.exe");
  }
  const kits = path.join(env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Windows Kits", "10", "bin");
  if (!existsSync(kits)) return null;
  const versions = readdirSync(kits).filter((name) => /^\d/.test(name)).sort((a, b) => b.localeCompare(a, "en", { numeric: true }));
  for (const version of versions) {
    const candidate = path.join(kits, version, "x64", "signtool.exe");
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

// The path of `7z.exe`, which opens an NSIS installer: on the PATH, else in its usual folder; null when absent.
export function findSevenZip(env = process.env) {
  for (const dir of (env.PATH ?? env.Path ?? "").split(path.delimiter)) {
    if (dir && existsSync(path.join(dir, "7z.exe"))) return path.join(dir, "7z.exe");
  }
  const candidate = path.join(env.ProgramFiles ?? "C:\\Program Files", "7-Zip", "7z.exe");
  return existsSync(candidate) ? candidate : null;
}

// What `signtool verify /pa /v` says of `file`: whether it is signed and trusted, and the report.
export function authenticode(signtool, file) {
  const result = spawnSync(signtool, ["verify", "/pa", "/v", file], { encoding: "utf8" });
  if (result.error) throw new Error(`could not run ${signtool}: ${result.error.message}`);
  const report = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  return { signed: result.status === 0, report, signer: signerOf(report) };
}

// Throws unless every file is signed and trusted.
export function assertSigned(signtool, files) {
  const unsigned = files.filter((file) => !authenticode(signtool, file).signed);
  if (unsigned.length > 0) {
    throw new Error(`Not Authenticode-signed (signtool verify /pa fails): ${unsigned.join(", ")}.`);
  }
}

// Prints whether `file` is signed and by whom; never fails.
export function reportSignature(signtool, file) {
  if (!signtool) return console.log(`${path.basename(file)}: signature not checked (no signtool.exe found)`);
  const { signed, signer } = authenticode(signtool, file);
  console.log(`${path.basename(file)}: ${signed ? `signed, ${signer ?? "signer unknown"}` : "not signed"}`);
}

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

function listFiles(dir) {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.join(entry.parentPath, entry.name));
}

// The files of `listing` whose path ends with `tail` (a relative path with `/`), whatever the case and separators.
export function filesEndingWith(listing, tail) {
  const wanted = `/${tail.toLowerCase()}`;
  return listing.filter((file) => `/${file.replaceAll("\\", "/").toLowerCase()}`.endsWith(wanted));
}

// The ConPTY files whose contents differ from the package's: `listing` holds the installer's extracted files, and
// `fetched` is the directory `fetch-conpty.mjs --installer` filled. A file the installer lacks counts as different.
export function conptyMismatches(listing, fetched) {
  const mismatches = [];
  for (const { target } of installerLayout()) {
    const found = filesEndingWith(listing, target);
    if (found.length !== 1 || sha256(found[0]) !== sha256(path.join(fetched, target))) mismatches.push(target);
  }
  return mismatches;
}

// Looks inside the installer, extracted with 7-Zip into a directory of its own: which of our executables carry a
// signature (all must, unless the mode is `none`), whether the ConPTY files are Microsoft's, byte for byte, and
// with Microsoft's signature (reported only). NSIS builds the uninstaller itself while it compiles the installer, so
// it is not among the files and no mode signs it.
export function verifyInstaller({ setup, mode, signtool }) {
  const sevenZip = findSevenZip();
  if (!sevenZip) {
    if (mode !== "none") throw new Error("Cannot look inside the installer to check what is signed: 7z.exe was not found. Install 7-Zip.");
    return console.log("7z.exe not found: the installer's files were not checked.");
  }
  const work = mkdtempSync(path.join(os.tmpdir(), "octoboard-installer-"));
  try {
    const extracted = spawnSync(sevenZip, ["x", "-y", `-o${work}`, setup], { encoding: "utf8" });
    if (extracted.error || extracted.status !== 0) {
      throw new Error(`Extracting ${setup} failed: ${extracted.error?.message ?? extracted.stdout}`);
    }
    const listing = listFiles(work);
    const own = ["Octoboard.exe", "octoboardd.exe"].map((name) => {
      const found = filesEndingWith(listing, name);
      if (found.length !== 1) throw new Error(`Expected one ${name} in the installer, found ${found.length}.`);
      return found[0];
    });
    if (mode !== "none") assertSigned(signtool, own);
    for (const file of own) reportSignature(signtool, file);
    const mismatches = conptyMismatches(listing, path.resolve(repoRoot, INSTALLER_DIR));
    if (mismatches.length > 0) {
      throw new Error(`The installer's ${mismatches.join(", ")} is not the package's file; Microsoft's ConPTY files must go in untouched.`);
    }
    for (const { target } of installerLayout()) reportSignature(signtool, filesEndingWith(listing, target)[0]);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

// Removes the installers and signatures of earlier builds from `dir`. Their names carry the version, which a rebuild
// of the same version does not change, so one left over would be taken for the new build's.
export function removeInstallers(dir) {
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    if (name.endsWith("-setup.exe") || name.endsWith("-setup.exe.sig")) rmSync(path.join(dir, name), { force: true });
  }
}

// The installer and its signature in `dir`, a Windows build's output copied beside the Mac one: the installer named
// for this version and the signature present and not older than it. A few seconds of slack, since downloading a
// workflow artifact writes the two files one after the other.
export function windowsReleaseFiles(dir, productName, version) {
  const installer = path.join(dir, windowsInstallerName(productName, version));
  const signature = `${installer}.sig`;
  for (const file of [installer, signature]) {
    if (!existsSync(file)) throw new Error(`${file} does not exist. The Windows build of ${version} makes ${path.basename(installer)} and its .sig.`);
  }
  if (statSync(signature).mtimeMs + 5000 < statSync(installer).mtimeMs) {
    throw new Error(`${signature} is older than ${installer}, so it is left over from an earlier build. Sign the update again.`);
  }
  return { installer, signature };
}

export const signFileScript = path.join(path.dirname(fileURLToPath(import.meta.url)), "sign-file.mjs");
