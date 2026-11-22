// The prerequisites of `pnpm check:windows`, the `cargo xwin` cross-check for both Windows targets, checked
// before it starts, so a missing tool fails with how to install it instead of a compiler error deep in a build
// script. Release verification runs that check unconditionally, so this has to name every missing piece at once.
//
// clang/LLVM: `cargo xwin` links its own `clang-cl` to the first `clang` on the PATH, and Apple's clang cannot
// build for MSVC, so Homebrew's LLVM, when installed, is moved to the front of the PATH, and the first `clang`
// must be LLVM's. The probes run in the repository root, where `rust-toolchain.toml` picks the pinned toolchain.
//
// The MSVC CRT and Windows SDK license: `cargo xwin` downloads them on first use, and only then asks for
// `XWIN_ACCEPT_LICENSE=1`. Accepting that license is the user's act, so this never sets it: it passes when
// the variable is set or when the download is already in the cache (which holds a `DONE` marker once complete).
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

// The Windows targets the release builds, by the architecture the installer's name carries; the check covers each.
export const WINDOWS_TARGETS = { x64: "x86_64-pc-windows-msvc", arm64: "aarch64-pc-windows-msvc" };
const HOMEBREW_LLVM = ["/opt/homebrew/opt/llvm/bin", "/usr/local/opt/llvm/bin"];
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const defaultRun = (cmd, args, env) => {
  const result = spawnSync(cmd, args, { cwd: repoRoot, encoding: "utf8", env });
  return result.status === 0 ? result.stdout : null;
};

// The cache `cargo xwin` uses: `XWIN_CACHE_DIR`, else the platform cache directory's `cargo-xwin`.
export function xwinCacheDir(env = process.env, platform = process.platform, home = os.homedir()) {
  if (env.XWIN_CACHE_DIR) return env.XWIN_CACHE_DIR;
  if (platform === "darwin") return path.join(home, "Library", "Caches", "cargo-xwin");
  if (platform === "win32") return path.join(env.LOCALAPPDATA ?? path.join(home, "AppData", "Local"), "cargo-xwin");
  return path.join(env.XDG_CACHE_HOME || path.join(home, ".cache"), "cargo-xwin");
}

// The environment to run `cargo xwin` in: the caller's, with Homebrew's LLVM first on the PATH when present.
export function windowsCheckEnv(env = process.env, exists = existsSync) {
  const entries = (env.PATH ?? "").split(path.delimiter).filter(Boolean);
  const llvm = HOMEBREW_LLVM.find((dir) => exists(path.join(dir, "clang")));
  if (!llvm || entries[0] === llvm) return env;
  return { ...env, PATH: [llvm, ...entries.filter((entry) => entry !== llvm)].join(path.delimiter) };
}

// `ring` compiles its C for `aarch64-pc-windows-msvc` with a compiler named `clang`, whatever `CC` names, yet with the
// flags cargo-xwin gives `clang-cl` for that target, so the GNU driver reads `/imsvc <dir>` as an input file and the
// build script fails. cargo-xwin's other backend, `clang`, is no way out: it puts the MSVC headers ahead of clang's own,
// so C using NEON (zstd) links against MSVC's intrinsic stubs and fails. The shim this writes into `dir`, put first on
// the PATH for that target, is the `clang` both find: run as `clang-cl` (cargo-xwin links its `clang-cl` to the first
// `clang` on the PATH) it is `realClang` in its `cl` mode; run as `clang` it passes `/imsvc <dir>` on as
// `-idirafter <dir>`, which is what `/imsvc` means, and everything else as it is. Returns `dir`.
export function writeArm64ClangShim(dir, realClang) {
  mkdirSync(dir, { recursive: true });
  const quoted = `'${realClang.replaceAll("'", "'\\''")}'`;
  const script = [
    "#!/bin/sh",
    "# Written by scripts/lib/windows-toolchain.mjs (writeArm64ClangShim) for the arm64 Windows cross-check.",
    `real=${quoted}`,
    'case "$(basename "$0")" in clang-cl*) exec "$real" --driver-mode=cl "$@" ;; esac',
    "n=$#",
    'while [ "$n" -gt 0 ]; do',
    "  arg=$1; shift; n=$((n - 1))",
    '  if [ "$arg" = /imsvc ] && [ "$n" -gt 0 ]; then',
    '    set -- "$@" -idirafter "$1"; shift; n=$((n - 1))',
    "  else",
    '    set -- "$@" "$arg"',
    "  fi",
    "done",
    'exec "$real" "$@"',
    "",
  ].join("\n");
  const shim = path.join(dir, "clang");
  writeFileSync(shim, script);
  chmodSync(shim, 0o755);
  return dir;
}

// The first `clang` on the PATH of `env`, null when there is none.
export function clangOnPath(env, exists = existsSync) {
  for (const entry of (env.PATH ?? "").split(path.delimiter).filter(Boolean)) {
    if (exists(path.join(entry, "clang"))) return path.join(entry, "clang");
  }
  return null;
}

// Every missing prerequisite for checking `targets` (both Windows targets unless narrowed), each with how to provide
// it; empty when the check can run.
export function windowsToolchainProblems({
  env = windowsCheckEnv(),
  run = defaultRun,
  exists = existsSync,
  cacheDir = xwinCacheDir(env),
  targets = Object.values(WINDOWS_TARGETS),
} = {}) {
  const problems = [];
  if (run("cargo", ["xwin", "--version"], env) === null) problems.push("cargo-xwin is not installed: run `cargo install --locked cargo-xwin`.");
  const installed = run("rustup", ["target", "list", "--installed"], env)?.split("\n").map((line) => line.trim()) ?? [];
  for (const target of targets) {
    if (!installed.includes(target)) problems.push(`The ${target} Rust target is not installed: run \`rustup target add ${target}\`.`);
  }
  const clang = run("clang", ["--version"], env);
  if (clang === null || /^Apple /.test(clang)) problems.push("The first clang on the PATH is not LLVM's: run `brew install llvm`, or put another LLVM installation's bin directory first on the PATH.");
  if (!env.XWIN_ACCEPT_LICENSE && !exists(path.join(cacheDir, "xwin", "DONE"))) {
    problems.push(`The MSVC CRT and Windows SDK have not been downloaded to ${cacheDir}: run \`XWIN_ACCEPT_LICENSE=1 pnpm check:windows\` once to accept their license and download them.`);
  }
  return problems;
}
