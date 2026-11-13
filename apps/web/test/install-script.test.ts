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

// `public/install.sh` is the script behind `curl -fsSL https://octoboard.dev/install.sh | sh`. These tests run it
// with the host's own shell and tools, under a PATH of their own that holds a `uname` posing as a Linux machine and
// only the tools the script uses, so the host is not asked for a `systemctl`, `minisign` or `wget` it has or lacks.
// The manifests and archives are `file://` files made here, so nothing touches the network.

const installScript = fileURLToPath(new URL("../public/install.sh", import.meta.url));
const tauriConfig = fileURLToPath(new URL("../../desktop/src-tauri/tauri.conf.json", import.meta.url));
const appConfig = fileURLToPath(new URL("../../../config/app.json", import.meta.url));
const exampleUnit = fileURLToPath(new URL("../../daemon/dist/systemd/octoboardd.service", import.meta.url));

type Arch = "x86_64" | "aarch64";
const ARCHES: Arch[] = ["x86_64", "aarch64"];

// The tools the script (and the stubs here) call by name; `curl`, `wget`, `systemctl` and `minisign` are added per test.
const BASE_TOOLS = ["awk", "base64", "cat", "chmod", "cmp", "cp", "cut", "grep", "head", "id", "mkdir", "mktemp", "mv", "rm", "sed", "tar"];
const HASH_TOOLS = ["sha256sum", "shasum"];
const SYSTEM_DIRECTORIES = ["/usr/bin", "/bin"];

function systemTool(name: string): string | undefined {
  return SYSTEM_DIRECTORIES.map((directory) => path.join(directory, name)).find((file) => existsSync(file));
}

const missingTools = [...BASE_TOOLS, "curl"].filter((name) => systemTool(name) === undefined);
if (HASH_TOOLS.every((name) => systemTool(name) === undefined)) missingTools.push(HASH_TOOLS.join(" or "));
const skip = missingTools.length > 0 ? `the host lacks ${missingTools.join(", ")}` : false;

type Shims = { curl?: boolean; wget?: boolean; systemctl?: boolean; minisign?: boolean; hashTools?: boolean };

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
  const tools = [...BASE_TOOLS];
  if (shims.curl !== false) tools.push("curl");
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
  if (shims.wget === true) {
    // Handles `wget -q -O <file> <file:// address>`, the one form the script uses.
    writeExecutable(path.join(world.shim, "wget"), [
      'echo "wget $*" >> "$SHIM_LOG"',
      'out=""; url=""',
      "while [ $# -gt 0 ]; do",
      '  case "$1" in -O) out=$2; shift ;; -q) ;; *) url=$1 ;; esac',
      "  shift",
      "done",
      'cp "${url#file://}" "$out" 2> /dev/null || exit 8',
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
  if (shims.minisign === true) {
    writeExecutable(path.join(world.shim, "minisign"), [
      'echo "minisign $*" >> "$SHIM_LOG"',
      "while [ $# -gt 0 ]; do",
      '  if [ "$1" = -x ]; then cat "$2" > "$SHIM_LOG.minisig"; fi',
      "  shift",
      "done",
      'exit "${MINISIGN_EXIT:-0}"',
    ]);
  }
  return world;
}

type Published = Record<Arch, { file: string; sha256: string; marker: string }>;
type PublishOptions = {
  version?: string;
  manifest?: string;
  notes?: string;
  signatureText?: string;
  entry?: (arch: Arch, entry: Record<string, string>) => Record<string, string>;
};

// Makes the archive of each architecture (an `octoboardd_<version>_linux_<arch>/` folder holding a stub `octoboardd`,
// the unit file and the license) and a pretty-printed manifest pointing to them, in the served directory.
function publish(world: World, options: PublishOptions = {}): Published {
  const { version = "1.2.3", manifest = "stable-linux.json", notes = "Fixes.", signatureText = "minisign signature text" } = options;
  const published = {} as Published;
  const platforms: Record<string, Record<string, string>> = {};
  for (const arch of ARCHES) {
    const folder = `octoboardd_${version}_linux_${arch}`;
    const staging = path.join(world.root, "staging", `${version}-${arch}`);
    mkdirSync(path.join(staging, folder), { recursive: true });
    const marker = `octoboardd stub ${version} ${arch}`;
    writeExecutable(path.join(staging, folder, "octoboardd"), [
      'if [ "$1" = status ]; then exit "${STUB_STATUS_EXIT:-1}"; fi',
      `echo "${marker}"`,
    ]);
    writeFileSync(path.join(staging, folder, "octoboardd.service"), "[Unit]\nDescription=Octoboard daemon\n\n[Service]\nExecStart=%h/.local/bin/octoboardd start\n\n[Install]\nWantedBy=default.target\n");
    writeFileSync(path.join(staging, folder, "LICENSE"), "license\n");
    const file = path.join(world.served, `octoboardd_${version}_linux_${arch}.tar.gz`);
    const tar = spawnSync("tar", ["-czf", file, "-C", staging, folder], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
    assert.equal(tar.status, 0, String(tar.stderr));
    const sha256 = createHash("sha256").update(readFileSync(file)).digest("hex");
    published[arch] = { file, sha256, marker };
    const entry = { url: `file://${file}`, sha256, signature: Buffer.from(signatureText).toString("base64") };
    platforms[`linux-${arch}`] = options.entry?.(arch, entry) ?? entry;
  }
  const text = JSON.stringify({ version, notes, pub_date: "2026-11-01T00:00:00.000Z", holder_floor: "0.1.0", platforms }, null, 2);
  writeFileSync(path.join(world.served, manifest), `${text}\n`);
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
  writeFileSync(path.join(world.served, "stable-linux.json"), `${JSON.stringify({ notes: rest.notes, ...rest, version }, null, 2)}\n`);
  const result = run(world, ["--bin-dir", world.binDir], { FAKE_UNAME_M: "aarch64" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Installed octoboardd 2\.0\.0 /);
  assert.equal(output(installed(world)), published.aarch64.marker);
});

test("a manifest that cannot be read, names no version or has no build for this machine is refused", { skip }, (t) => {
  let world = makeWorld(t);
  assertRefused(run(world, []), world, /could not read /);

  world = makeWorld(t);
  writeFileSync(path.join(world.served, "stable-linux.json"), '{\n  "notes": "none"\n}\n');
  assertRefused(run(world, []), world, /names no version/);

  world = makeWorld(t);
  publish(world);
  const manifest = JSON.parse(readFileSync(path.join(world.served, "stable-linux.json"), "utf8"));
  delete manifest.platforms["linux-aarch64"];
  writeFileSync(path.join(world.served, "stable-linux.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  assertRefused(run(world, [], { FAKE_UNAME_M: "aarch64" }), world, /no linux-aarch64 build of 1\.2\.3/);
});

test("a download whose SHA-256 differs from the manifest's is refused, and the installed binary is left alone", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world, { entry: (_arch, entry) => ({ ...entry, sha256: "0".repeat(64) }) });
  mkdirSync(world.binDir, { recursive: true });
  writeExecutable(installed(world), ['echo "the old binary"']);
  const result = run(world, []);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not match its SHA-256; nothing was installed/);
  assert.equal(output(installed(world)), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
  assert.deepEqual(readdirSync(world.tmp), []);
});

test("a download that fails, or comes by a protocol other than https or file, installs nothing", { skip }, (t) => {
  let world = makeWorld(t);
  publish(world, { entry: (_arch, entry) => ({ ...entry, url: `file://${world.served}/missing.tar.gz` }) });
  assertRefused(run(world, []), world, /could not download /);

  world = makeWorld(t);
  publish(world, { entry: (_arch, entry) => ({ ...entry, url: "http://127.0.0.1:9/octoboardd.tar.gz" }) });
  assertRefused(run(world, []), world, /http:\/\/127\.0\.0\.1:9\/octoboardd\.tar\.gz is neither an https nor a file address/);
});

test("wget downloads when curl is not installed, and with neither the install is refused", { skip }, (t) => {
  let world = makeWorld(t, { curl: false, wget: true });
  const published = publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.match(logged(world), /^wget /m);

  world = makeWorld(t, { curl: false });
  publish(world);
  assertRefused(run(world, []), world, /neither curl nor wget is installed/);
});

test("without minisign the SHA-256 alone gates the install", { skip }, (t) => {
  const world = makeWorld(t);
  publish(world);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /Signature verified/);
});

test("with minisign the signature is checked with the embedded key against the manifest's base64 of the signature file", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  const signatureText = "untrusted comment: signature\nRUQsignatureline\ntrusted comment: t\nglobalsignature\n";
  const published = publish(world, { signatureText });
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Signature verified/);
  assert.equal(output(installed(world)), published.x86_64.marker);
  const embedded = /^update_pubkey="([^"]+)"$/m.exec(readFileSync(installScript, "utf8"))?.[1];
  const call = logged(world).split("\n").find((line) => line.startsWith("minisign "));
  assert.ok(call, "minisign ran");
  assert.ok(call.includes(` -P ${embedded} `), call);
  assert.equal(readFileSync(`${world.log}.minisig`, "utf8"), signatureText);
});

test("OCTOBOARD_UPDATE_PUBKEY replaces the key a signature is checked with", { skip }, (t) => {
  const world = makeWorld(t, { minisign: true });
  publish(world);
  const result = run(world, [], { OCTOBOARD_UPDATE_PUBKEY: "RWTestKeyForTheSignatureCheck" });
  assert.equal(result.status, 0, result.stderr);
  assert.ok(logged(world).includes(" -P RWTestKeyForTheSignatureCheck "), logged(world));
});

test("with minisign a signature that does not verify, or a manifest without one, is refused", { skip }, (t) => {
  let world = makeWorld(t, { minisign: true });
  publish(world);
  assertRefused(run(world, [], { MINISIGN_EXIT: "1" }), world, /signature does not verify; nothing was installed/);
  assert.deepEqual(readdirSync(world.tmp), []);

  world = makeWorld(t, { minisign: true });
  publish(world, { entry: (_arch, entry) => ({ url: entry.url, sha256: entry.sha256 }) });
  assertRefused(run(world, []), world, /no signature for the linux-x86_64 build/);
});

test("the binary replaces the old one by rename, so a process running the old file keeps it", { skip }, (t) => {
  const world = makeWorld(t);
  const published = publish(world);
  mkdirSync(world.binDir, { recursive: true });
  writeExecutable(installed(world), ['echo "the old binary"']);
  const running = path.join(world.root, "running-old-binary");
  linkSync(installed(world), running);
  const result = run(world, []);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(output(installed(world)), published.x86_64.marker);
  assert.equal(output(running), "the old binary");
  assert.deepEqual(readdirSync(world.binDir), ["octoboardd"]);
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
  mkdirSync(world.binDir, { recursive: true });
  writeExecutable(installed(world), ['echo "old"']);
  let result = run(world, [], { STUB_STATUS_EXIT: "0" });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /octoboardd restart/);

  world = makeWorld(t);
  publish(world);
  result = run(world, [], { STUB_STATUS_EXIT: "1" });
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /octoboardd restart/);
});

test("--help prints the usage and exits 0 without reading anything", { skip }, (t) => {
  for (const flag of ["--help", "-h"]) {
    const world = makeWorld(t);
    const result = run(world, [flag]);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Usage: install\.sh/);
    assert.match(result.stdout, /--service/);
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
