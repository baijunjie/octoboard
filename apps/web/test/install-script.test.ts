import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  linkSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
// The release's own modules write the manifests and name the archives, so the fixtures here are the ones the
// release publishes, not a hand-built copy of their format.
import { buildManifest, LINUX_ARCHES, linuxArchiveName } from "../../desktop/scripts/release-manifest.mjs";

// `public/install.sh` is the script behind `curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh`.
// These tests run it with the host's own shell and tools, under a PATH of their own that holds a `uname` posing as a
// Linux machine, an `id` that can pose as another user and only the tools the script uses, so the host is not asked
// for a `systemctl`, `minisign`, `flock` or `wget` it has or lacks. `curl` and `wget` are shims that serve the files
// made here and never touch the network.

const installScript = fileURLToPath(new URL("../public/install.sh", import.meta.url));
const terminalDemo = fileURLToPath(new URL("../app/components/TerminalDemo.vue", import.meta.url));
const tauriConfig = fileURLToPath(new URL("../../desktop/src-tauri/tauri.conf.json", import.meta.url));
const appConfig = fileURLToPath(new URL("../../../config/app.json", import.meta.url));
const exampleUnit = fileURLToPath(new URL("../../daemon/dist/systemd/octoboardd.service", import.meta.url));
// A signed archive the daemon's own tests use, with the key that signed it.
const signedFixtures = fileURLToPath(new URL("../../daemon/testdata/self_update/", import.meta.url));

type Arch = "x86_64" | "aarch64";
const ARCHES = LINUX_ARCHES as Arch[];

// The tools the script (and the stubs here) call by name; `curl`, `wget`, `systemctl`, `loginctl`, `flock` and
// `minisign` are added per test, and `id` is replaced by a stub that calls the real one.
const BASE_TOOLS = ["awk", "base64", "cat", "chmod", "cmp", "cp", "cut", "grep", "head", "id", "ln", "ls", "mkdir", "mktemp", "mv", "rm", "sed", "tar"];
const HASH_TOOLS = ["sha256sum", "shasum"];
const SYSTEM_DIRECTORIES = ["/usr/bin", "/bin"];
// Where a minisign the host has might be (the shim's PATH holds only what a test puts in it).
const MINISIGN_DIRECTORIES = [...SYSTEM_DIRECTORIES, "/usr/local/bin", "/opt/homebrew/bin"];

function systemTool(name: string, directories = SYSTEM_DIRECTORIES): string | undefined {
  return directories.map((directory) => path.join(directory, name)).find((file) => existsSync(file));
}

const missingTools = [...BASE_TOOLS].filter((name) => systemTool(name) === undefined);
if (HASH_TOOLS.every((name) => systemTool(name) === undefined)) missingTools.push(HASH_TOOLS.join(" or "));
// The script refuses to run as root without --bin-dir, which the tests do not pass, so a root runner cannot run them.
const runsAsRoot = process.getuid?.() === 0;
const skip = missingTools.length > 0 ? `the host lacks ${missingTools.join(", ")}` : runsAsRoot ? "install.sh refuses to run as root without --bin-dir" : false;
const realMinisign = systemTool("minisign", MINISIGN_DIRECTORIES);
const skipRealMinisign = skip !== false ? skip : realMinisign === undefined ? "minisign is not installed on the host" : false;
const realCurl = systemTool("curl");
const skipRealCurl = skip !== false ? skip : realCurl === undefined ? "curl is not installed on the host" : false;

type Shims = {
  // `true` (the default) is a shim that serves the files made here; "real" is the host's curl (for file:// addresses only).
  curl?: boolean | "real";
  wget?: boolean;
  systemctl?: boolean;
  loginctl?: boolean;
  flock?: boolean;
  // `true` is a shim that emulates `minisign -V`; "real" is the host's minisign.
  minisign?: boolean | "real";
  hashTools?: boolean;
};

type World = {
  root: string;
  shim: string;
  home: string;
  tmp: string;
  served: string;
  binDir: string;
  log: string;
};

function writeExecutable(file: string, lines: string[]): void {
  writeFileSync(file, `#!/bin/sh\n${lines.join("\n")}\n`);
  chmodSync(file, 0o755);
}

function makeWorld(t: test.TestContext, shims: Shims = {}): World {
  const root = realpathSync(mkdtempSync(path.join(os.tmpdir(), "install-script-")));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const world: World = {
    root,
    shim: path.join(root, "shim"),
    home: path.join(root, "home"),
    tmp: path.join(root, "tmp"),
    served: path.join(root, "served"),
    binDir: path.join(root, "home", ".local", "bin"),
    log: path.join(root, "shim.log"),
  };
  for (const directory of [world.shim, world.home, world.tmp, world.served]) mkdirSync(directory, { recursive: true });
  const tools = BASE_TOOLS.filter((name) => name !== "id");
  if (shims.hashTools !== false) tools.push(...HASH_TOOLS);
  for (const name of tools) {
    const file = systemTool(name);
    if (file !== undefined) symlinkSync(file, path.join(world.shim, name));
  }
  writeExecutable(path.join(world.shim, "uname"), [
    'case "$1" in',
    '  -s) printf "%s\\n" "${FAKE_UNAME_S:-Linux}" ;;',
    '  -m) printf "%s\\n" "${FAKE_UNAME_M:-x86_64}" ;;',
    '  *) printf "Linux\\n" ;;',
    "esac",
  ]);
  // The real `id`, unless FAKE_UID / FAKE_USER pose as another user.
  const realId = systemTool("id") ?? "id";
  writeExecutable(path.join(world.shim, "id"), [
    'case "$1" in',
    `  -u) if [ -n "\${FAKE_UID:-}" ]; then echo "$FAKE_UID"; else exec ${realId} -u; fi ;;`,
    `  -un) if [ -n "\${FAKE_USER:-}" ]; then echo "$FAKE_USER"; else exec ${realId} -un; fi ;;`,
    `  *) exec ${realId} "$@" ;;`,
    "esac",
  ]);
  if (shims.curl === "real") {
    const file = systemTool("curl");
    if (file !== undefined) symlinkSync(file, path.join(world.shim, "curl"));
  } else if (shims.curl !== false) {
    // Logs what it is called with and honors --proto the way curl does (a file:// address is refused unless the
    // protocols include file). A file:// address is copied; an https one is served from the directory of the
    // files made here by its last path segment; anything else fails as curl fails on a protocol it may not use.
    writeExecutable(path.join(world.shim, "curl"), [
      'echo "curl $*" >> "$SHIM_LOG.curl"',
      'proto=""; out=""; url=""',
      "while [ $# -gt 0 ]; do",
      "  case \"$1\" in",
      "    --proto) proto=$2; shift ;;",
      "    --proto-redir) shift ;;",
      "    -o) out=$2; shift ;;",
      "    -*) ;;",
      "    *) url=$1 ;;",
      "  esac",
      "  shift",
      "done",
      'case "$url" in',
      '  file://*) case "$proto" in *file*) ;; *) exit 1 ;; esac; cp "${url#file://}" "$out" 2> /dev/null || exit 37 ;;',
      '  https://*) cp "$SHIM_SERVED/${url##*/}" "$out" 2> /dev/null || exit 22 ;;',
      "  *) exit 1 ;;",
      "esac",
    ]);
  }
  if (shims.wget === true) {
    // Answers `--help` with or without GNU wget's --https-only (WGET_NO_HTTPS_ONLY), and handles
    // `wget -q [--https-only] -O <file> <address>`, the one form the script downloads with: a file:// address is
    // copied, an https one is served from the directory of the files made here.
    writeExecutable(path.join(world.shim, "wget"), [
      'if [ "${1:-}" = --help ]; then',
      '  echo "GNU Wget 1.21"',
      '  if [ -z "${WGET_NO_HTTPS_ONLY:-}" ]; then echo "      --https-only   only follow secure HTTPS links"; fi',
      "  exit 0",
      "fi",
      'echo "wget $*" >> "$SHIM_LOG"',
      'out=""; url=""',
      "while [ $# -gt 0 ]; do",
      '  case "$1" in -O) out=$2; shift ;; -*) ;; *) url=$1 ;; esac',
      "  shift",
      "done",
      'case "$url" in',
      '  file://*) cp "${url#file://}" "$out" 2> /dev/null || exit 8 ;;',
      '  https://*) cp "$SHIM_SERVED/${url##*/}" "$out" 2> /dev/null || exit 8 ;;',
      "  *) exit 1 ;;",
      "esac",
    ]);
  }
  if (shims.systemctl === true) {
    // Nothing is active, so a service gets started.
    writeExecutable(path.join(world.shim, "systemctl"), [
      'echo "systemctl $*" >> "$SHIM_LOG"',
      'case "$*" in *is-active*) exit 3 ;; esac',
      "exit 0",
    ]);
  }
  if (shims.loginctl === true) {
    // `show-user` says FAKE_LINGER (default: no); `enable-linger` exits LINGER_ENABLE_EXIT (default: succeeds).
    writeExecutable(path.join(world.shim, "loginctl"), [
      'echo "loginctl $*" >> "$SHIM_LOG"',
      'case "$*" in',
      '  *show-user*) echo "${FAKE_LINGER:-no}" ;;',
      '  *enable-linger*) exit "${LINGER_ENABLE_EXIT:-0}" ;;',
      "esac",
    ]);
  }
  if (shims.flock === true) {
    // `flock -n 9` on the descriptor the script opened: fails when descriptor 9 is not open, else exits FLOCK_EXIT.
    writeExecutable(path.join(world.shim, "flock"), [
      'echo "flock $*" >> "$SHIM_LOG"',
      "{ true >&9; } 2> /dev/null || exit 66",
      'exit "${FLOCK_EXIT:-0}"',
    ]);
  }
  if (shims.minisign === "real") {
    if (realMinisign !== undefined) symlinkSync(realMinisign, path.join(world.shim, "minisign"));
  } else if (shims.minisign === true) {
    // Emulates `minisign -V` as it prints without -q: "Signature and comment signature verified", then the trusted
    // comment, tab-separated fields among which `file:<name of the signed file>`. The name is the -m file's own
    // unless MINISIGN_FILE says another; MINISIGN_TRUSTED_COMMENT replaces the whole comment; MINISIGN_EXIT fails it.
    writeExecutable(path.join(world.shim, "minisign"), [
      'echo "minisign $*" >> "$SHIM_LOG"',
      'quiet=false; file=""; sig=""',
      "while [ $# -gt 0 ]; do",
      '  case "$1" in -q) quiet=true ;; -m) file=$2; shift ;; -x) sig=$2; shift ;; -P) shift ;; esac',
      "  shift",
      "done",
      'cat "$sig" > "$SHIM_LOG.minisig" || exit 1',
      '[ -f "$file" ] || exit 1',
      'if [ "${MINISIGN_EXIT:-0}" != 0 ]; then echo "Signature verification failed" >&2; exit "$MINISIGN_EXIT"; fi',
      'if [ "$quiet" = false ]; then',
      '  echo "Signature and comment signature verified"',
      '  if [ -n "${MINISIGN_TRUSTED_COMMENT:-}" ]; then',
      '    printf "Trusted comment: %s\\n" "$MINISIGN_TRUSTED_COMMENT"',
      "  else",
      '    printf "Trusted comment: timestamp:1\\tfile:%s\\tversion:0\\n" "${MINISIGN_FILE:-${file##*/}}"',
      "  fi",
      "fi",
    ]);
  }
  return world;
}

type Published = Record<Arch, { file: string; name: string; sha256: string; marker: string }>;
type Entry = { url: string; signature?: string; sha256: string };
type PublishOptions = {
  version?: string;
  manifest?: string;
  notes?: string;
  signatureText?: string;
  // Replaces the manifest's entry of an architecture after the release's own `buildManifest` made it (which refuses
  // an entry without a signature), to publish an odd one.
  entry?: (arch: Arch, entry: Entry) => Entry;
};

function writeManifest(world: World, manifest: unknown, name = "stable-linux.json"): void {
  // As publish-release.mjs writes it.
  writeFileSync(path.join(world.served, name), `${JSON.stringify(manifest, null, 2)}\n`);
}

const PUB_DATE = "2026-11-01T00:00:00.000Z";
const HOLDER_FLOOR = "0.1.0";

// Makes the archive of each architecture (an `octoboardd_<version>_linux_<arch>/` folder holding a stub `octoboardd`,
// the unit file and the license, named as the release names it) and the manifest `buildManifest` writes for them,
// pointing to them, in the served directory.
function publish(world: World, options: PublishOptions = {}): Published {
  const { version = "1.2.3", manifest = "stable-linux.json", notes = "Fixes.", signatureText = "minisign signature text" } = options;
  const published = {} as Published;
  const entries: Record<string, Entry> = {};
  for (const arch of ARCHES) {
    const name = linuxArchiveName(version, arch);
    const folder = name.replace(/\.tar\.gz$/, "");
    const staging = path.join(world.root, "staging", `${version}-${arch}`);
    mkdirSync(path.join(staging, folder), { recursive: true });
    const marker = `octoboardd stub ${version} ${arch}`;
    // Answers `--version` and `status` as the daemon does (`status` exits STUB_STATUS_EXIT, default 1: not running),
    // and `verify-archive` as a build that predates it does, with an exit of 2.
    writeExecutable(path.join(staging, folder, "octoboardd"), [
      'case "$1" in',
      `  --version) echo "octoboardd ${version}"; exit 0 ;;`,
      '  status) exit "${STUB_STATUS_EXIT:-1}" ;;',
      "  verify-archive) exit 2 ;;",
      "esac",
      `echo "${marker}"`,
    ]);
    writeFileSync(path.join(staging, folder, "octoboardd.service"), "[Unit]\nDescription=Octoboard daemon\n\n[Service]\nExecStart=%h/.local/bin/octoboardd start\n\n[Install]\nWantedBy=default.target\n");
    writeFileSync(path.join(staging, folder, "LICENSE"), "license\n");
    const file = path.join(world.served, name);
    const tar = spawnSync("tar", ["-czf", file, "-C", staging, folder], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
    assert.equal(tar.status, 0, String(tar.stderr));
    const sha256 = createHash("sha256").update(readFileSync(file)).digest("hex");
    published[arch] = { file, name, sha256, marker };
    entries[`linux-${arch}`] = { url: `file://${file}`, sha256, signature: Buffer.from(signatureText).toString("base64") };
  }
  const built = buildManifest({ version, notes, pubDate: PUB_DATE, platform: "linux", entries, holderFloor: HOLDER_FLOOR });
  if (options.entry) {
    for (const arch of ARCHES) built.platforms[`linux-${arch}`] = options.entry(arch, built.platforms[`linux-${arch}`]);
  }
  writeManifest(world, built, manifest);
  return published;
}

function run(world: World, args: string[], env: Record<string, string> = {}) {
  return spawnSync("/bin/sh", [installScript, ...args], {
    cwd: world.root,
    encoding: "utf8",
    env: {
      PATH: world.shim,
      HOME: world.home,
      TMPDIR: world.tmp,
      SHIM_LOG: world.log,
      SHIM_SERVED: world.served,
      OCTOBOARD_MANIFEST_BASE: `file://${world.served}`,
      ...env,
    },
  });
}

function installed(world: World): string {
  return path.join(world.binDir, "octoboardd");
}

function output(binary: string): string {
  return spawnSync(binary, [], { encoding: "utf8" }).stdout.trim();
}

function assertRefused(result: ReturnType<typeof run>, world: World, message: RegExp) {
  assert.notEqual(result.status, 0, `${result.stdout}${result.stderr}`);
  assert.match(result.stderr, message);
  assert.equal(existsSync(installed(world)), false, "nothing is installed");
}

function logged(world: World): string {
  return existsSync(world.log) ? readFileSync(world.log, "utf8") : "";
}

// The calls the curl shim got, one per line.
function curlCalls(world: World): string[] {
  const file = `${world.log}.curl`;
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter((line) => line !== "") : [];
}

type OldBinary = {
  // What running it with no argument prints.
  text?: string;
  // The version `--version` prints (as `octoboardd <version>`); without it, `--version` exits 2 and prints nothing.
  version?: string;
  // `--version`'s first line as it is, for a build that does not print a version.
  versionLine?: string;
  // How it answers `verify-archive`: "ok" (understands --help; otherwise exits VERIFY_EXIT, default 0, after logging
  // what it was given and saving the signature file) or "old" (a build that predates it, which exits 2).
  verify?: "ok" | "old";
};

// An octoboardd already in the install directory, as an earlier install or upgrade left it.
function writeOldBinary(world: World, options: OldBinary = {}): void {
  const { text = "the old binary", version, versionLine, verify = "old" } = options;
  mkdirSync(world.binDir, { recursive: true });
  const line = versionLine ?? (version === undefined ? undefined : `octoboardd ${version}`);
  writeExecutable(installed(world), [
    'case "$1" in',
    line === undefined ? "  --version) exit 2 ;;" : `  --version) echo "${line}"; exit 0 ;;`,
    '  status) exit "${STUB_STATUS_EXIT:-1}" ;;',
    verify === "ok"
      ? '  verify-archive) if [ "${2:-}" = --help ]; then exit 0; fi; echo "installed verify-archive $2 $3" >> "$SHIM_LOG"; cat "$3" > "$SHIM_LOG.sig"; exit "${VERIFY_EXIT:-0}" ;;'
      : "  verify-archive) exit 2 ;;",
    "esac",
    `echo "${text}"`,
  ]);
}

test("the key the script checks signatures with is the update key the desktop application trusts", () => {
  const script = readFileSync(installScript, "utf8");
  const embedded = /^update_pubkey="([^"]+)"$/m.exec(script)?.[1];
  assert.ok(embedded, "install.sh has an update_pubkey line");
  const config = JSON.parse(readFileSync(tauriConfig, "utf8"));
  const decoded = Buffer.from(config.plugins.updater.pubkey, "base64").toString("utf8");
  // A minisign public key file: an `untrusted comment:` line, then the key.
  const keyLine = decoded.trim().split("\n").at(-1);
  assert.equal(embedded, keyLine);
});

test("the script's and the example unit's default port is the configured server port", () => {
  const { defaultServerPort } = JSON.parse(readFileSync(appConfig, "utf8"));
  const script = readFileSync(installScript, "utf8");
  assert.equal(/^default_port=(\d+)$/m.exec(script)?.[1], String(defaultServerPort));
  assert.deepEqual(script.match(/\b\d{4,5}\b/g)?.filter((n) => n !== "65535") ?? [], [String(defaultServerPort)]);
  const unit = readFileSync(exampleUnit, "utf8");
  assert.match(unit, new RegExp(`^ExecStart=\\S+ --port ${defaultServerPort} --service-managed$`, "m"));
});

test("the script reads the manifests from the repository's updater release", () => {
  const script = readFileSync(installScript, "utf8");
  const embedded = /^manifest_base_default="([^"]+)"$/m.exec(script)?.[1];
  assert.ok(embedded, "install.sh has a manifest_base_default line");
  const { repositoryUrl } = JSON.parse(readFileSync(appConfig, "utf8"));
  assert.equal(embedded, `${repositoryUrl.replace(/\/+$/, "")}/releases/download/updater`);
});

test("it installs the build for this machine and leaves nothing else behind", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Installed octoboardd 1\.2\.3 to /);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.ok((statSync(installed(world)).mode & 0o111) !== 0, "the binary is executable");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
  assert.deepEqual(readdirSync(world.tmp), []);
  assert.equal(logged(world), "");
});

test("the architecture comes from uname -m, and an unsupported one is refused", { skip }, (t) => {
  for (const [machine, arch] of [
    ["x86_64", "x86_64"],
    ["amd64", "x86_64"],
    ["aarch64", "aarch64"],
    ["arm64", "aarch64"],
  ] as const) {
    const world = makeWorld(t);
    const published = publish(world);
    const result = run(world, [], { FAKE_UNAME_M: machine });
    assert.equal(result.status, 0, `${machine}: ${result.stderr}`);
    assert.equal(output(installed(world)), published[arch].marker, machine);
  }
  for (const machine of ["armv7l", "i686", "riscv64"]) {
    const world = makeWorld(t);
    publish(world);
    assertRefused(run(world, [], { FAKE_UNAME_M: machine }), world, new RegExp(`no octoboardd build for ${machine}`));
  }
});

test("it is for Linux only", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  assertRefused(run(world, [], { FAKE_UNAME_S: "Darwin" }), world, /Linux only; this is Darwin/);
});

test("the stable manifest is read by default and the beta one with --beta", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world, { version: "1.0.0", manifest: "stable-linux.json" });
  publish(world, { version: "1.1.0-beta.2", manifest: "beta-linux.json" });
  const stable = run(world, []);
  assert.equal(stable.status, 0, stable.stderr);
  assert.match(output(installed(world)), /stub 1\.0\.0 x86_64/);
  const beta = run(world, ["--beta"]);
  assert.equal(beta.status, 0, beta.stderr);
  assert.match(output(installed(world)), /stub 1\.1\.0-beta\.2 x86_64/);
});

test("the version and the build's fields come from the manifest's own fields, not from text in the notes", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world, { version: "2.0.0", notes: 'Mentions "version": "9.9.9" and\n"url": "file:///nowhere" in passing.' });
  // Notes ahead of the version, as a hand-edited manifest might have them.
  const manifest = JSON.parse(readFileSync(path.join(world.served, "stable-linux.json"), "utf8"));
  const { version, ...rest } = manifest;
  writeManifest(world, { notes: rest.notes, ...rest, version });
  const result = run(world, ["--bin-dir", world.binDir], { FAKE_UNAME_M: "aarch64" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Installed octoboardd 2\.0\.0 /);
  assert.equal(output(installed(world)), published.aarch64.marker);
});

test("a manifest that cannot be read, names no version or has no build for this machine is refused", { skip }, (t) => {
  let world = makeWorld(t);
  assertRefused(run(world, []), world, /could not read /);

  world = makeWorld(t);
  writeManifest(world, { notes: "none" });
  assertRefused(run(world, []), world, /names no version/);

  world = makeWorld(t);
  publish(world);
  const manifest = JSON.parse(readFileSync(path.join(world.served, "stable-linux.json"), "utf8"));
  delete manifest.platforms["linux-aarch64"];
  writeManifest(world, manifest);
  assertRefused(run(world, [], { FAKE_UNAME_M: "aarch64" }), world, /no linux-aarch64 build of 1\.2\.3/);
});

test("a download whose SHA-256 differs from the manifest's is refused, and the installed binary is left alone", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world, { entry: (_arch, entry) => ({ ...entry, sha256: "0".repeat(64) }) });
  writeOldBinary(world);
  const result = run(world, []);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not match its SHA-256; nothing was installed/);
  assert.equal(output(installed(world)), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("a download that fails, or comes by a protocol other than https, installs nothing", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world, { entry: (_arch, entry) => ({ ...entry, url: `file://${world.served}/missing.tar.gz` }) });
  assertRefused(run(world, []), world, /could not download /);

  for (const address of ["http://127.0.0.1:9/octoboardd.tar.gz", "ftp://127.0.0.1:9/octoboardd.tar.gz", "127.0.0.1/octoboardd.tar.gz"]) {
    world = makeWorld(t);
    publish(world, { entry: (_arch, entry) => ({ ...entry, url: address }) });
    assertRefused(run(world, []), world, new RegExp(`${address.replace(/[./]/g, "\\$&")} is not an https address; nothing was downloaded from it`));
    assert.equal(curlCalls(world).some((call) => call.includes(address)), false, `${address} is not fetched`);
  }
});

test("a manifest address that is not https is refused before anything is fetched", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  assertRefused(
    run(world, [], { OCTOBOARD_MANIFEST_BASE: "http://mirror.example/updater" }),
    world,
    /http:\/\/mirror\.example\/updater\/stable-linux\.json is not an https address; nothing was downloaded from it/,
  );
  assert.deepEqual(curlCalls(world), []);
});

test("curl is told to take https only, redirects included, and TLS 1.2 or later, with file addresses allowed only for a mirror", { skip }, (t) => {
  // OCTOBOARD_MANIFEST_BASE set (as in every test here): file:// addresses are allowed, and only then.
  let world = makeWorld(t);
  publish(world);
  assert.equal(run(world, []).status, 0);
  let calls = curlCalls(world);
  assert.equal(calls.length, 2, "the manifest and the archive");
  for (const call of calls) assert.match(call, /--proto =https,file --proto-redir =https --tlsv1\.2 /, call);

  // Not set: the release's own address, over https alone. The https shim serves the files made here by name.
  world = makeWorld(t);
  const published = publish(world, {
    entry: (arch, entry) => ({ ...entry, url: `https://downloads.example/${linuxArchiveName("1.2.3", arch)}` }),
  });
  const result = run(world, [], { OCTOBOARD_MANIFEST_BASE: "" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  calls = curlCalls(world);
  assert.equal(calls.length, 2);
  assert.match(calls[0], /https:\/\/\S+\/stable-linux\.json$/);
  for (const call of calls) {
    assert.match(call, /--proto =https --proto-redir =https --tlsv1\.2 /, call);
    assert.equal(call.includes("=https,file"), false, call);
  }
});

test("a file address for the archive is refused unless the manifests come from a mirror", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  // The release's own manifest address (served by the shim), whose archive entry points to a local file.
  const result = run(world, [], { OCTOBOARD_MANIFEST_BASE: "" });
  assertRefused(result, world, /file:\/\/\S+octoboardd_1\.2\.3_linux_x86_64\.tar\.gz is not an https address; nothing was downloaded from it/);
  assert.equal(curlCalls(world).length, 1, "only the manifest was fetched");
});

test("curl accepts the options the script gives it", { skip: skipRealCurl }, (t) => {
  const world = makeWorld(t, { curl: "real" });
  const published = publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
});

test("wget downloads when curl is not installed, with --https-only where it has it, and with neither the install is refused", { skip }, (t) => {
  let world = makeWorld(t, { curl: false, wget: true });
  const published = publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  const calls = logged(world).split("\n").filter((line) => line.startsWith("wget "));
  assert.equal(calls.length, 2, "the manifest and the archive");
  for (const call of calls) assert.match(call, /^wget -q --https-only -O /, call);

  // A wget that does not have the option (BusyBox's) is not given it.
  world = makeWorld(t, { curl: false, wget: true });
  publish(world);
  const plain = run(world, [], { WGET_NO_HTTPS_ONLY: "1" });
  assert.equal(plain.status, 0, plain.stderr);
  assert.match(logged(world), /^wget /m);
  assert.doesNotMatch(logged(world), /--https-only/);

  world = makeWorld(t, { curl: false });
  publish(world);
  assertRefused(run(world, []), world, /neither curl nor wget is installed/);
});

test("without minisign, and no installed octoboardd to check it, the SHA-256 alone gates the install, with one warning", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /Signature verified/);
  assert.equal(result.stderr.match(/warning:/g)?.length, 1, result.stderr);
  assert.match(result.stderr, /^octoboardd install: warning: the download's signature was not checked/m);
  assert.ok(existsSync(installed(world)));
});

test("--require-signature stops, installing nothing, when the signature cannot be checked", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world);
  assertRefused(run(world, ["--require-signature"]), world, /the download's signature cannot be checked/);
  assert.deepEqual(readdirSync(world.tmp), []);

  // Nor does a build that predates `verify-archive` make it checkable.
  world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { version: "1.0.0", verify: "old" });
  let result = run(world, ["--require-signature"]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /the download's signature cannot be checked/);
  assert.equal(output(installed(world)), "the old binary");

  // With a check that passes it goes on.
  world = makeWorld(t, { minisign: true });
  publish(world);
  result = run(world, ["--require-signature"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Signature verified\./);
  assert.doesNotMatch(result.stderr, /warning/);
});

test("with minisign the signature is checked with the embedded key against the manifest's base64 of the signature file", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  const signatureText = "untrusted comment: signature\nRUQsignatureline\ntrusted comment: t\nglobalsignature\n";
  const published = publish(world, { signatureText });
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^Signature verified\.$/m);
  assert.doesNotMatch(result.stderr, /warning/);
  assert.equal(output(installed(world)), published.x86_64.marker);
  const embedded = /^update_pubkey="([^"]+)"$/m.exec(readFileSync(installScript, "utf8"))?.[1];
  const call = logged(world).split("\n").find((line) => line.startsWith("minisign "));
  assert.ok(call, "minisign ran");
  assert.ok(call.includes(` -P ${embedded} `), call);
  assert.ok(call.includes(` -m ${world.tmp}`), call);
  assert.ok(call.includes(`/${published.x86_64.name} `), "it checks the archive under the name the release gave it");
  assert.equal(readFileSync(`${world.log}.minisig`, "utf8"), signatureText);
});

test("minisign is not run with -q, since the trusted comment it prints is what names the signed file", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  publish(world);
  assert.equal(run(world, []).status, 0);
  const call = logged(world).split("\n").find((line) => line.startsWith("minisign "));
  assert.ok(call);
  assert.match(call, /^minisign -V /);
  assert.doesNotMatch(call, /(^| )-[a-zA-Z]*[qQ]/);
});

test("OCTOBOARD_UPDATE_PUBKEY replaces the key a signature is checked with", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  publish(world);
  const result = run(world, [], { OCTOBOARD_UPDATE_PUBKEY: "RWTestKeyForTheSignatureCheck" });
  assert.equal(result.status, 0, result.stderr);
  assert.ok(logged(world).includes(" -P RWTestKeyForTheSignatureCheck "), logged(world));
});

test("a signature that does not verify is refused, and the installed binary is left alone", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  publish(world);
  writeOldBinary(world);
  const result = run(world, [], { MINISIGN_EXIT: "1" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /signature does not verify; nothing was installed/);
  assert.equal(output(installed(world)), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("a signature of another file than the archive is refused, so an older release's archive cannot stand in for it", { skip }, (t) => {
  const name = "octoboardd_1.2.3_linux_x86_64.tar.gz";
  const trusted = [
    // The same key signed an older release's archive.
    { MINISIGN_FILE: "octoboardd_1.0.0_linux_x86_64.tar.gz" },
    { MINISIGN_FILE: "octoboardd_1.2.3_linux_aarch64.tar.gz" },
    // A field only has to contain the name, not be it.
    { MINISIGN_FILE: `x${name}` },
    { MINISIGN_FILE: `${name}.old` },
    // No file field, or the name in another field.
    { MINISIGN_TRUSTED_COMMENT: "timestamp:1\tversion:1.2.3" },
    { MINISIGN_TRUSTED_COMMENT: `timestamp:1\tnote:file:${name}` },
    { MINISIGN_TRUSTED_COMMENT: `timestamp:1 file:${name}` },
  ];
  for (const env of trusted) {
    const world = makeWorld(t, { minisign: true });
    publish(world);
    writeOldBinary(world);
    const result = run(world, [], env);
    assert.notEqual(result.status, 0, JSON.stringify(env));
    assert.match(result.stderr, new RegExp(`the signature is not of ${name.replace(/\./g, "\\.")}; nothing was installed`), JSON.stringify(env));
    assert.equal(output(installed(world)), "the old binary", JSON.stringify(env));
    assert.deepEqual(readdirSync(world.tmp), [], JSON.stringify(env));
  }
});

test("a signature field among others in the trusted comment is found wherever it stands", { skip }, (t) => {
  const name = "octoboardd_1.2.3_linux_x86_64.tar.gz";
  for (const comment of [`file:${name}`, `file:${name}\ttimestamp:1`, `timestamp:1\tversion:1.2.3\tfile:${name}`]) {
    const world = makeWorld(t, { minisign: true });
    publish(world);
    const result = run(world, [], { MINISIGN_TRUSTED_COMMENT: comment });
    assert.equal(result.status, 0, `${comment}: ${result.stderr}`);
  }
});

test("a manifest entry without a signature is refused, with minisign or without", { skip }, (t) => {
  for (const shims of [{ minisign: true }, {}]) {
    const world = makeWorld(t, shims);
    publish(world, { entry: (_arch, entry) => ({ url: entry.url, sha256: entry.sha256 }) });
    assertRefused(run(world, []), world, /has no signature for the linux-x86_64 build/);
    assertRefused(run(world, ["--require-signature"]), world, /has no signature for the linux-x86_64 build/);
    assert.deepEqual(readdirSync(world.tmp), []);
  }
});

test("the installed octoboardd checks the signature where minisign is not installed", { skip }, (t) => {
  const signatureText = "untrusted comment: signature\nRUQsignatureline\n";
  const world = makeWorld(t);
  const published = publish(world, { signatureText });
  writeOldBinary(world, { version: "1.0.0", verify: "ok" });
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^Signature verified \(by the installed octoboardd\)\.$/m);
  assert.doesNotMatch(result.stderr, /warning/);
  assert.equal(output(installed(world)), published.x86_64.marker);
  // It is handed the archive under the release's name and the manifest's signature value, as it is.
  assert.match(logged(world), new RegExp(`^installed verify-archive \\S+/${published.x86_64.name} \\S+$`, "m"));
  assert.equal(readFileSync(`${world.log}.sig`, "utf8"), Buffer.from(signatureText).toString("base64"));
});

test("a signature the installed octoboardd rejects is refused, and the installed binary is left alone", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { version: "1.0.0", verify: "ok" });
  const result = run(world, [], { VERIFY_EXIT: "1" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not verify \(checked by the installed octoboardd\); nothing was installed/);
  assert.equal(output(installed(world)), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("an installed octoboardd that predates verify-archive leaves the check undone, with a warning", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  writeOldBinary(world, { version: "1.0.0", verify: "old" });
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /warning: the download's signature was not checked/);
  assert.doesNotMatch(result.stdout, /Signature verified/);
  assert.equal(output(installed(world)), published.x86_64.marker);
});

test("OCTOBOARD_UPDATE_PUBKEY keeps the installed octoboardd, which knows only the release key, from being asked", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { version: "1.0.0", verify: "ok" });
  const result = run(world, [], { OCTOBOARD_UPDATE_PUBKEY: "RWTestKeyForTheSignatureCheck" });
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(logged(world), /installed verify-archive/);
  assert.match(result.stderr, /warning: the download's signature was not checked/);
});

test("minisign is preferred to the installed octoboardd where both can check", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  publish(world);
  writeOldBinary(world, { version: "1.0.0", verify: "ok" });
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^Signature verified\.$/m);
  assert.doesNotMatch(logged(world), /installed verify-archive/);
});

// Real minisign against the signed archive the daemon's tests use (octoboardd_9.9.9_linux_x86_64.tar.gz, whose
// `.sig` file holds the manifest's `signature` value, and the key that signed it), so the output the script parses is
// the real tool's, not the shim's.
type SignedFixture = { version?: string; bytes?: Buffer };

function publishSigned(world: World, { version = "9.9.9", bytes }: SignedFixture = {}): string {
  const fixture = path.join(signedFixtures, "octoboardd_9.9.9_linux_x86_64.tar.gz");
  const content = bytes ?? readFileSync(fixture);
  const file = path.join(world.served, linuxArchiveName(version, "x86_64"));
  writeFileSync(file, content);
  const sha256 = createHash("sha256").update(content).digest("hex");
  const signature = readFileSync(`${fixture}.sig`, "utf8");
  const entry = { url: `file://${file}`, sha256, signature };
  const manifest = buildManifest({
    version,
    notes: "Test.",
    pubDate: PUB_DATE,
    platform: "linux",
    entries: { "linux-x86_64": entry, "linux-aarch64": entry },
    holderFloor: HOLDER_FLOOR,
  });
  writeManifest(world, manifest);
  return file;
}

function signedKey(): string {
  return readFileSync(path.join(signedFixtures, "test-release-key.txt"), "utf8").trim().split("\n").at(-1) as string;
}

test("real minisign verifies the release's signed archive", { skip: skipRealMinisign }, (t) => {
  const world = makeWorld(t, { minisign: "real" });
  publishSigned(world);
  const result = run(world, [], { OCTOBOARD_UPDATE_PUBKEY: signedKey() });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^Signature verified\.$/m);
  assert.match(result.stdout, /Installed octoboardd 9\.9\.9 to /);
  assert.ok(existsSync(installed(world)));
});

test("real minisign: the signed archive under another version's name is refused", { skip: skipRealMinisign }, (t) => {
  const world = makeWorld(t, { minisign: "real" });
  // The same bytes, which the signature covers, announced as 9.9.8: the signature names 9.9.9's file.
  publishSigned(world, { version: "9.9.8" });
  const result = run(world, [], { OCTOBOARD_UPDATE_PUBKEY: signedKey() });
  assertRefused(result, world, /the signature is not of octoboardd_9\.9\.8_linux_x86_64\.tar\.gz; nothing was installed/);
});

test("real minisign: an archive altered after it was signed is refused, even with a SHA-256 to match", { skip: skipRealMinisign }, (t) => {
  const world = makeWorld(t, { minisign: "real" });
  const original = readFileSync(path.join(signedFixtures, "octoboardd_9.9.9_linux_x86_64.tar.gz"));
  publishSigned(world, { bytes: Buffer.concat([original, Buffer.from("altered")]) });
  assertRefused(run(world, [], { OCTOBOARD_UPDATE_PUBKEY: signedKey() }), world, /the download's signature does not verify; nothing was installed/);
});

test("real minisign: the release key is not the test key, so the signed archive is refused under the embedded key", { skip: skipRealMinisign }, (t) => {
  const world = makeWorld(t, { minisign: "real" });
  publishSigned(world);
  assertRefused(run(world, []), world, /the download's signature does not verify; nothing was installed/);
});

test("an installed beta newer than the stable release takes the beta channel by default, and --stable the stable one", { skip }, (t) => {
  const prepare = () => {
    const world = makeWorld(t);
    publish(world, { version: "0.3.0", manifest: "stable-linux.json" });
    publish(world, { version: "0.4.0-beta.3", manifest: "beta-linux.json" });
    writeOldBinary(world, { version: "0.4.0-beta.2" });
    return world;
  };

  let world = prepare();
  let result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /The installed octoboardd 0\.4\.0-beta\.2 is a beta newer than the stable release 0\.3\.0, so this installs the newest beta \(--stable takes the stable release\)\./,
  );
  assert.equal(result.stdout.split("\n").filter((line) => line.includes("is a beta")).length, 1, "said in one line");
  assert.match(output(installed(world)), /stub 0\.4\.0-beta\.3 x86_64/);
  // The stable manifest is read first, to find that its release is older.
  const manifests = curlCalls(world).filter((call) => call.endsWith("-linux.json"));
  assert.equal(manifests.length, 2, manifests.join("\n"));
  assert.ok(manifests[0].endsWith("/stable-linux.json") && manifests[1].endsWith("/beta-linux.json"), manifests.join("\n"));

  // --stable would take 0.3.0, which is older than the beta installed: refused before anything is downloaded.
  world = prepare();
  result = run(world, ["--stable"]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /octoboardd 0\.4\.0-beta\.2 is installed, which is newer than 0\.3\.0, the newest stable release; nothing was installed/);
  assert.match(result.stderr, /--allow-downgrade/);
  assert.doesNotMatch(result.stdout, /is a beta/);
  assert.equal(output(installed(world)), "the old binary");
  assert.equal(curlCalls(world).some((call) => call.includes(".tar.gz")), false, "nothing is downloaded");

  world = prepare();
  result = run(world, ["--stable", "--allow-downgrade"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(output(installed(world)), /stub 0\.3\.0 x86_64/);

  // --beta reads the beta manifest alone, and says nothing of a choice.
  world = prepare();
  result = run(world, ["--beta"]);
  assert.equal(result.status, 0, result.stderr);
  assert.match(output(installed(world)), /stub 0\.4\.0-beta\.3 x86_64/);
  assert.doesNotMatch(result.stdout, /is a beta/);
  assert.equal(curlCalls(world).some((call) => call.endsWith("/stable-linux.json")), false);
});

test("an installed beta stays on the stable channel when the stable release is not older than it", { skip }, (t) => {
  // The stable manifest announces prereleases until the first stable release, the same one as installed or a newer
  // one; or a stable release newer than the beta. None of these moves the installation to the beta channel.
  for (const stable of ["0.4.0-beta.2", "0.4.0-beta.3", "0.4.0"]) {
    const world = makeWorld(t);
    publish(world, { version: stable, manifest: "stable-linux.json" });
    publish(world, { version: "0.5.0-beta.1", manifest: "beta-linux.json" });
    writeOldBinary(world, { version: "0.4.0-beta.2" });
    const result = run(world, []);
    assert.equal(result.status, 0, `${stable}: ${result.stderr}`);
    assert.match(output(installed(world)), new RegExp(`stub ${stable.replace(/\./g, "\\.")} x86_64`), stable);
    assert.doesNotMatch(result.stdout, /is a beta/, stable);
    assert.equal(curlCalls(world).some((call) => call.endsWith("/beta-linux.json")), false, stable);
  }
});

test("an older release than the installed one is refused unless --allow-downgrade, by semantic versioning's order", { skip }, (t) => {
  // [installed, announced, the channel's option, refused]
  const cases: [string, string, string[], boolean][] = [
    ["1.0.0", "0.9.0", [], true],
    ["1.0.0", "1.0.0", [], false],
    ["1.0.0", "1.0.1", [], false],
    ["0.9.0", "0.10.0", [], false],
    ["0.10.0", "0.9.0", [], true],
    ["1.0.0", "1.0.0-beta.2", ["--beta"], true],
    ["1.0.0-beta.1", "1.0.0", ["--stable"], false],
    ["0.4.0-beta.2", "0.4.0-beta.10", [], false],
    ["0.4.0-beta.10", "0.4.0-beta.2", ["--stable"], true],
    ["0.4.0-beta.2", "0.4.0-beta.2", [], false],
    ["0.4.0-alpha", "0.4.0-beta", ["--beta"], false],
    ["0.4.0-beta", "0.4.0-alpha", ["--beta"], true],
  ];
  for (const [have, announced, options, refused] of cases) {
    const label = `${have} installed, ${announced} announced`;
    const world = makeWorld(t);
    publish(world, { version: announced, manifest: options.includes("--beta") ? "beta-linux.json" : "stable-linux.json" });
    writeOldBinary(world, { version: have });
    const result = run(world, options);
    if (refused) {
      assert.notEqual(result.status, 0, label);
      assert.match(result.stderr, new RegExp(`octoboardd ${have.replace(/\./g, "\\.")} is installed, which is newer than ${announced.replace(/\./g, "\\.")}`), label);
      assert.equal(output(installed(world)), "the old binary", label);
      assert.equal(curlCalls(world).some((call) => call.includes(".tar.gz")), false, `${label}: nothing is downloaded`);
      const allowed = run(world, [...options, "--allow-downgrade"]);
      assert.equal(allowed.status, 0, `${label}: ${allowed.stderr}`);
      assert.match(output(installed(world)), new RegExp(`stub ${announced.replace(/\./g, "\\.")} x86_64`), label);
    } else {
      assert.equal(result.status, 0, `${label}: ${result.stderr}`);
      assert.match(output(installed(world)), new RegExp(`stub ${announced.replace(/\./g, "\\.")} x86_64`), label);
    }
  }
});

test("an installed octoboardd that does not say its version sets no guard", { skip }, (t) => {
  // Absent: `--version` exits 2 with nothing; garbled: the last word of its first line is no version.
  for (const old of [{}, { versionLine: "octoboardd garbled" }, { versionLine: "" }]) {
    const world = makeWorld(t);
    const published = publish(world, { version: "0.1.0" });
    writeOldBinary(world, old);
    const result = run(world, []);
    assert.equal(result.status, 0, `${JSON.stringify(old)}: ${result.stderr}`);
    assert.equal(output(installed(world)), published.x86_64.marker);
    assert.doesNotMatch(result.stdout, /is a beta/);
  }
});

test("run as root without --bin-dir, the install is refused before anything is fetched", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  const result = run(world, [], { FAKE_UID: "0" });
  assertRefused(result, world, /running as root/);
  assert.deepEqual(curlCalls(world), []);
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("a HOME that is not the running user's is refused, with --bin-dir too", { skip }, (t) => {
  const uid = process.getuid?.() ?? 0;
  const world = makeWorld(t);
  publish(world);
  // Another user's uid than the one that owns HOME.
  let result = run(world, [], { FAKE_UID: String(uid + 1) });
  assertRefused(result, world, new RegExp(`${world.home.replace(/[./]/g, "\\$&")} is not the home of the user running this \\(uid ${uid + 1}\\)`));
  assert.deepEqual(curlCalls(world), []);

  // Root's, as under `sudo` keeping the invoking user's HOME: --bin-dir lifts the root refusal, not this one.
  result = run(world, ["--bin-dir", world.binDir], { FAKE_UID: "0" });
  assertRefused(result, world, /is not the home of the user running this \(uid 0\)/);
  assert.doesNotMatch(result.stderr, /running as root/);

  // A HOME that someone else owns (the root directory belongs to root).
  result = run(world, [], { HOME: "/" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /\/ is not the home of the user running this/);
  assert.equal(existsSync(installed(world)), false);
  assert.deepEqual(curlCalls(world), []);
});

test("the next steps say octoboardd where the PATH finds the installed binary, and its full path where it does not", { skip }, (t) => {
  // Not on the PATH (the PATH is the shim directory): the full path, and the advice to put the directory there.
  let world = makeWorld(t);
  publish(world);
  let result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, new RegExp(`Start it with \`${installed(world)} start\``));
  assert.match(result.stdout, new RegExp(`then run \`${installed(world)} address\`: it prints the address`));
  assert.match(result.stdout, /is not on your PATH/);

  // First on the PATH (with the shim, which the script needs): the name.
  world = makeWorld(t);
  publish(world);
  result = run(world, [], { PATH: `${world.binDir}:${world.shim}` });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Start it with `octoboardd start`/);
  assert.match(result.stdout, /then run `octoboardd address`: it prints the address/);
  assert.doesNotMatch(result.stdout, /is not on your PATH/);

  // Another octoboardd ahead of it on the PATH is not the one installed: the full path again.
  world = makeWorld(t);
  publish(world);
  const decoy = path.join(world.root, "decoy");
  mkdirSync(decoy);
  writeExecutable(path.join(decoy, "octoboardd"), ["exit 0"]);
  result = run(world, [], { PATH: `${decoy}:${world.binDir}:${world.shim}` });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, new RegExp(`Start it with \`${installed(world)} start\``));
});

test("the restart advice names the command the same way", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { version: "1.0.0" });
  let result = run(world, [], { STUB_STATUS_EXIT: "0" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, new RegExp(`Run \`${installed(world)} restart\` to move it to 1\\.2\\.3`));

  world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { version: "1.0.0" });
  result = run(world, [], { STUB_STATUS_EXIT: "0", PATH: `${world.binDir}:${world.shim}` });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Run `octoboardd restart` to move it to 1\.2\.3/);
});

test("an upgrade keeps the replaced binary as octoboardd.previous, a fresh install keeps none", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world);
  let result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);

  world = makeWorld(t);
  const upgraded = publish(world);
  writeOldBinary(world, { version: "1.0.0" });
  // The file a running daemon would be executing, by another name.
  const running = path.join(world.root, "running-old-binary");
  linkSync(installed(world), running);
  // An earlier `.previous` is replaced by it.
  writeFileSync(path.join(world.binDir, "octoboardd.previous"), "an older build\n");
  result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), upgraded.x86_64.marker);
  assert.deepEqual(readdirSync(world.binDir).sort(), ["octoboardd", "octoboardd.previous"]);
  const previous = path.join(world.binDir, "octoboardd.previous");
  assert.equal(output(previous), "the old binary");
  assert.equal(statSync(previous).ino, statSync(running).ino, "the same file as the one replaced, not a copy");
  assert.notEqual(statSync(previous).ino, statSync(installed(world)).ino);
});

test("an old binary that does not say its version is kept as octoboardd.previous too", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  writeOldBinary(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(path.join(world.binDir, "octoboardd.previous")), "the old binary");
});

test("a reinstall of the same build leaves the octoboardd.previous there is untouched", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  assert.equal(run(world, []).status, 0);
  const previous = path.join(world.binDir, "octoboardd.previous");
  assert.equal(existsSync(previous), false, "nothing to keep after a fresh install");
  writeExecutable(previous, ['echo "the build before"']);
  const before = statSync(previous).ino;
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.equal(output(previous), "the build before");
  assert.equal(statSync(previous).ino, before);
  assert.deepEqual(readdirSync(world.binDir).sort(), ["octoboardd", "octoboardd.previous"]);
});

test("the update lock is taken where flock is installed and the data directory exists", { skip }, (t) => {
  const world = makeWorld(t, { flock: true });
  const published = publish(world);
  mkdirSync(path.join(world.home, ".octoboard"));
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.ok(existsSync(path.join(world.home, ".octoboard", "update.lock")), "the lock file is there");
  // flock was run on the descriptor the script opened on it (the shim exits 66 when there is none).
  assert.match(logged(world), /^flock -n 9$/m);
});

test("an update under way, which holds the lock, stops the install before anything is replaced", { skip }, (t) => {
  const world = makeWorld(t, { flock: true });
  publish(world);
  mkdirSync(path.join(world.home, ".octoboard"));
  writeOldBinary(world, { version: "1.0.0" });
  const result = run(world, [], { FLOCK_EXIT: "1" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /an update is under way already .*nothing was installed/);
  assert.equal(output(installed(world)), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"], "neither a staged file nor octoboardd.previous is left");
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("the update lock is not taken without the data directory, and its directory is not made", { skip }, (t) => {
  const world = makeWorld(t, { flock: true });
  publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(logged(world), /^flock /m);
  assert.equal(existsSync(path.join(world.home, ".octoboard")), false);
});

test("without flock the install goes on without the lock", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  mkdirSync(path.join(world.home, ".octoboard"));
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(path.join(world.home, ".octoboard", "update.lock")), false);
});

test("the binary replaces the old one by rename, so a process running the old file keeps it", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  writeOldBinary(world);
  const running = path.join(world.root, "running-old-binary");
  linkSync(installed(world), running);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.equal(output(running), "the old binary");
  assert.deepEqual(readdirSync(world.binDir).sort(), ["octoboardd", "octoboardd.previous"]);
});

test("--bin-dir picks the directory, which is created when it is missing", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  const binDir = path.join(world.root, "opt", "tools", "bin");
  const result = run(world, ["--bin-dir", binDir]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(path.join(binDir, "octoboardd")), published.x86_64.marker);
  assert.equal(existsSync(installed(world)), false);
  assert.match(result.stdout, /is not on your PATH/);
});

test("when a daemon answers octoboardd status after the install, it says to run octoboardd restart", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world);
  writeOldBinary(world, { text: "old" });
  let result = run(world, [], { STUB_STATUS_EXIT: "0" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /octoboardd restart/);

  world = makeWorld(t);
  publish(world);
  result = run(world, [], { STUB_STATUS_EXIT: "1" });
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /octoboardd restart/);
});

test("with --service, lingering is turned on for the user where it is off, and advised where that is refused", { skip }, (t) => {
  // Turned on: the user's services then start at boot and survive logout.
  let world = makeWorld(t, { systemctl: true, loginctl: true });
  publish(world);
  let result = run(world, ["--service"], { FAKE_USER: "tester", FAKE_LINGER: "no" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Turned lingering on for tester, so the service starts at boot and keeps running after you log out\./);
  assert.doesNotMatch(result.stdout, /loginctl enable-linger/);
  assert.match(logged(world), /^loginctl --no-ask-password enable-linger tester$/m);

  // Refused (polkit says no, or there is no logind): the advice.
  world = makeWorld(t, { systemctl: true, loginctl: true });
  publish(world);
  result = run(world, ["--service"], { FAKE_USER: "tester", FAKE_LINGER: "no", LINGER_ENABLE_EXIT: "1" });
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /Turned lingering on/);
  assert.match(result.stdout, /Turn it on once with `loginctl enable-linger tester` \(it may need sudo\)\./);

  // Already on: nothing is said, and nothing is changed.
  world = makeWorld(t, { systemctl: true, loginctl: true });
  publish(world);
  result = run(world, ["--service"], { FAKE_USER: "tester", FAKE_LINGER: "yes" });
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /ling/i);
  assert.doesNotMatch(logged(world), /enable-linger/);
});

test("without --service, lingering is left alone", { skip }, (t) => {
  const world = makeWorld(t, { systemctl: true, loginctl: true });
  publish(world);
  assert.equal(run(world, []).status, 0);
  assert.doesNotMatch(logged(world), /loginctl/);
});

// The website's terminal demo (app/components/TerminalDemo.vue) plays the script's output, so a line the script says
// differently has to be changed there too. The demo's lines are read from the component's source, so this holds the two
// together whatever the wording.
const DEMO_BINARY = "/home/you/.local/bin/octoboardd";

// The text of a string or template literal of the component's source, with `${port}` filled in.
function literalText(literal: string, port: number): string {
  if (literal.startsWith('"')) return JSON.parse(literal);
  const inner = literal.slice(1, -1).replaceAll("\\`", "`").replaceAll("${port}", String(port));
  assert.ok(!inner.includes("${") && !inner.includes("\\"), `${literal} uses more than the template's \${port} and escaped backticks`);
  return inner;
}

// The `kind: "out"` lines of window 0 between the install command and the next command, which starts the daemon.
function demoInstallOutput(port: number): string[] {
  const source = readFileSync(terminalDemo, "utf8");
  const lines = source.slice(source.indexOf("const lines: Line[] = ["));
  const commands = [...lines.matchAll(/window: 0,\s*kind: "cmd"/g)].map((match) => match.index as number);
  assert.ok(commands.length >= 2, "the demo has the install command and one after it");
  const between = lines.slice(commands[0], commands[1]);
  return [...between.matchAll(/window: 0,\s*kind: "out",\s*text: ("(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`)/g)].map((match) => literalText(match[1], port));
}

test("the terminal demo's install output is lines the script prints, in order", { skip }, (t) => {
  const { defaultServerPort } = JSON.parse(readFileSync(appConfig, "utf8"));
  const demo = demoInstallOutput(defaultServerPort);
  assert.ok(demo.length >= 3, `the demo prints the download, the install and the next step: ${JSON.stringify(demo)}`);

  const world = makeWorld(t);
  publish(world, { version: "0.1.0" });
  // The install directory first on the PATH, as after the install the demo's user has it, so the script names the
  // command, not its path (the shim directory stays for the tools it uses).
  const result = run(world, [], { PATH: `${world.binDir}:${world.shim}` });
  assert.equal(result.status, 0, result.stderr);
  // The demo is of a user's home, not of this test's.
  const printed = result.stdout.replaceAll(installed(world), DEMO_BINARY).split("\n");
  let from = 0;
  for (const line of demo) {
    const at = printed.indexOf(line, from);
    assert.notEqual(at, -1, `the script does not print the demo's line after line ${from}: ${line}\n--- it printed ---\n${result.stdout}`);
    from = at + 1;
  }
});

test("--help prints the usage and exits 0 without reading anything", { skip }, (t) => {
  for (const flag of ["--help", "-h"]) {
    const world = makeWorld(t);
    const result = run(world, [flag]);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Usage: install\.sh/);
    assert.match(result.stdout, /--service/);
    assert.match(result.stdout, /--stable/);
    assert.match(result.stdout, /--allow-downgrade/);
    assert.match(result.stdout, /--require-signature/);
    assert.equal(existsSync(world.binDir), false);
  }
});

test("an unknown option is refused", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  assertRefused(run(world, ["--frobnicate"]), world, /unknown option --frobnicate/);
});

test("--port goes with --service, and must be a port number, before anything is downloaded", { skip }, (t) => {
  // No manifest is served, so a request for one would fail with another message than these.
  const world = makeWorld(t, { systemctl: true });
  assertRefused(run(world, ["--port", "8080"]), world, /--port is the port of the service, so it goes with --service/);
  assertRefused(run(world, ["--port"]), world, /--port needs a port number/);
  for (const port of ["0", "65536", "99999", "abc", "-1", "80a"]) {
    assertRefused(run(world, ["--service", "--port", port]), world, /is not a port number/);
  }
});

test("--service without systemctl is refused before anything is downloaded", { skip }, (t) => {
  const world = makeWorld(t);
  assertRefused(run(world, ["--service"]), world, /systemctl is not installed/);
  assert.equal(logged(world), "");
});

test("--service installs the unit with the binary's path and the port, enables it and starts it", { skip }, (t) => {
  for (const [args, port] of [
    [["--service"], String(JSON.parse(readFileSync(appConfig, "utf8")).defaultServerPort)],
    [["--service", "--port", "1"], "1"],
    [["--service", "--port", "65535"], "65535"],
  ] as const) {
    const world = makeWorld(t, { systemctl: true });
    publish(world);
    const result = run(world, args);
    assert.equal(result.status, 0, `${port}: ${result.stderr}`);
    const unit = readFileSync(path.join(world.home, ".config", "systemd", "user", "octoboardd.service"), "utf8");
    const execStart = unit.split("\n").find((line) => line.startsWith("ExecStart="));
    assert.ok(execStart?.startsWith(`ExecStart=${installed(world)} `), execStart);
    assert.ok(execStart?.includes(`--port ${port}`), execStart);
    assert.match(unit, /^WantedBy=default\.target$/m);
    const calls = logged(world);
    assert.match(calls, /^systemctl --user enable octoboardd\.service$/m);
    assert.match(calls, /^systemctl --user start octoboardd\.service$/m);
  }
});
