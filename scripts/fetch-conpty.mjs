#!/usr/bin/env node
// Fetches the bundled ConPTY the Windows daemon runs its pseudoconsoles on: `conpty.dll` and the
// `OpenConsole.exe` it starts, from the pinned `Microsoft.Windows.Console.ConPTY` NuGet package, and puts
// them beside each other in each target directory: `node scripts/fetch-conpty.mjs [--arch x64|arm64]
// [--into <dir>]...`. The daemon looks for `conpty.dll` beside its executable and decides from the files
// it finds there which host it runs on, so the directories are the ones its binaries are in: by default
// `target/debug/deps` (the test binaries) and `target/debug` (the dev daemon). The architecture is the
// machine's own unless given.
//
// `--installer` lays out what the Windows installer ships instead, in `target/conpty-bundle/` (the files
// `tauri.windows.conf.json` lists as `bundle.resources`): the x64 `conpty.dll` and an `OpenConsole.exe` for
// each architecture in its own subdirectory, from which `conpty.dll` starts the one matching the machine.
//
// The package is checked against its pinned SHA-256 before anything is extracted, and kept under `target/`
// once verified, so a rerun needs no network. A `.nupkg` is a zip, extracted with the system `tar`, which has to
// read zips: bsdtar, the `tar` of macOS and of Windows 10 and later. GNU tar, the usual `tar` on Linux, does not.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

export const VERSION = "1.25.260930003";
export const PACKAGE_URL = `https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/${VERSION}/microsoft.windows.console.conpty.${VERSION}.nupkg`;
export const PACKAGE_SHA256 = "02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee";
export const ARCHITECTURES = ["x64", "arm64"];
const DEFAULT_INTO = ["target/debug/deps", "target/debug"];
export const INSTALLER_DIR = "target/conpty-bundle";

// The machine's architecture, not the one an emulated Node runs as: an x64 Node on an ARM64 Windows
// machine reports x64 in `process.arch` and the machine's in `PROCESSOR_ARCHITEW6432`.
export function machineArchitecture(env = process.env, nodeArch = process.arch) {
  const reported = (env.PROCESSOR_ARCHITEW6432 ?? env.PROCESSOR_ARCHITECTURE ?? "").toUpperCase();
  if (reported === "ARM64") return "arm64";
  if (reported === "AMD64") return "x64";
  return nodeArch === "arm64" ? "arm64" : "x64";
}

export function parseArguments(argv, defaultArch = machineArchitecture()) {
  const options = { arch: defaultArch, into: [], installer: false };
  let placed = false;
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === "--installer") {
      options.installer = true;
      continue;
    }
    placed = true;
    if (flag !== "--arch" && flag !== "--into") throw new Error(`Unknown argument ${flag}. ${USAGE}`);
    const value = argv[++i];
    if (value === undefined) throw new Error(`${flag} needs a value. ${USAGE}`);
    if (flag === "--into") options.into.push(value);
    else options.arch = value;
  }
  if (options.installer && placed) throw new Error(`--installer takes neither --arch nor --into. ${USAGE}`);
  if (!ARCHITECTURES.includes(options.arch)) throw new Error(`--arch is one of ${ARCHITECTURES.join(", ")}. ${USAGE}`);
  return options;
}

const USAGE = "Usage: node scripts/fetch-conpty.mjs [--arch x64|arm64] [--into <dir>]... | --installer";

// Where the two files are inside the package.
export function packageFiles(arch) {
  return {
    dll: `runtimes/win-${arch}/native/conpty.dll`,
    host: `build/native/runtimes/${arch}/OpenConsole.exe`,
  };
}

// The installer's files: where each is in the package and where it goes in `INSTALLER_DIR`.
export function installerLayout() {
  return [
    { source: packageFiles("x64").dll, target: "conpty.dll" },
    ...ARCHITECTURES.map((arch) => ({ source: packageFiles(arch).host, target: `${arch}/OpenConsole.exe` })),
  ];
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// The package's bytes, from `cacheFile` when it holds the pinned package and else downloaded and kept
// there once they do. Bytes that are not the pinned package are refused and kept nowhere.
export async function verifiedPackage({ cacheFile, url = PACKAGE_URL, expected = PACKAGE_SHA256, download = downloadBytes }) {
  if (existsSync(cacheFile)) {
    const cached = readFileSync(cacheFile);
    if (sha256(cached) === expected) return cached;
  }
  const bytes = await download(url);
  const actual = sha256(bytes);
  if (actual !== expected) throw new Error(`${url} is not the pinned package: its SHA-256 is ${actual}, expected ${expected}`);
  mkdirSync(path.dirname(cacheFile), { recursive: true });
  writeFileSync(cacheFile, bytes);
  return bytes;
}

async function downloadBytes(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Downloading ${url} failed: ${response.status} ${response.statusText}`);
  return Buffer.from(await response.arrayBuffer());
}

// `tar` reads a zip on Windows 10 and later and on macOS; the package is one.
function extract(archive, dir) {
  const result = spawnSync("tar", ["-xf", archive, "-C", dir], { encoding: "utf8" });
  if (result.error || result.status !== 0) {
    throw new Error(`Extracting ${archive} failed: ${result.error?.message ?? result.stderr}`);
  }
}

export async function fetchConpty({ root = repoRoot, arch, into = [], installer = false, download, expected } = {}) {
  const cacheDir = path.join(root, "target", "conpty");
  const cacheFile = path.join(cacheDir, `Microsoft.Windows.Console.ConPTY.${VERSION}.nupkg`);
  const bytes = await verifiedPackage({ cacheFile, download, expected });
  // Each file to take from the package and every path it is copied to, one list for both layouts.
  const copies = installer
    ? installerLayout().map(({ source, target }) => ({ source, targets: [path.resolve(root, INSTALLER_DIR, target)] }))
    : [["dll", "conpty.dll"], ["host", "OpenConsole.exe"]].map(([kind, name]) => ({
      source: packageFiles(arch)[kind],
      targets: (into.length ? into : DEFAULT_INTO).map((dir) => path.resolve(root, dir, name)),
    }));
  const work = mkdtempSync(path.join(cacheDir, "extract-"));
  try {
    extract(cacheFile, work);
    for (const { source, targets } of copies) {
      const from = path.join(work, source);
      if (!existsSync(from)) throw new Error(`The package has no ${source}`);
      for (const target of targets) {
        mkdirSync(path.dirname(target), { recursive: true });
        copyFileSync(from, target);
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  const placed = installer ? [path.resolve(root, INSTALLER_DIR)] : [...new Set(copies[0].targets.map((target) => path.dirname(target)))];
  return { bytes: bytes.length, targets: placed };
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { arch, into, installer } = parseArguments(process.argv.slice(2));
    const { targets } = await fetchConpty({ arch, into, installer });
    console.log(`ConPTY ${VERSION} (${installer ? "installer layout" : arch}) placed in ${targets.join(", ")}.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
