// The prerequisites of `pnpm check:windows`, the `cargo xwin` cross-check for `x86_64-pc-windows-msvc`, checked
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
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const WINDOWS_TARGET = "x86_64-pc-windows-msvc";
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

// Every missing prerequisite, each with how to provide it; empty when the check can run.
export function windowsToolchainProblems({ env = windowsCheckEnv(), run = defaultRun, exists = existsSync, cacheDir = xwinCacheDir(env) } = {}) {
  const problems = [];
  if (run("cargo", ["xwin", "--version"], env) === null) problems.push("cargo-xwin is not installed: run `cargo install --locked cargo-xwin`.");
  const targets = run("rustup", ["target", "list", "--installed"], env);
  if (!targets?.split("\n").map((line) => line.trim()).includes(WINDOWS_TARGET)) {
    problems.push(`The ${WINDOWS_TARGET} Rust target is not installed: run \`rustup target add ${WINDOWS_TARGET}\`.`);
  }
  const clang = run("clang", ["--version"], env);
  if (clang === null || /^Apple /.test(clang)) problems.push("The first clang on the PATH is not LLVM's: run `brew install llvm`, or put another LLVM installation's bin directory first on the PATH.");
  if (!env.XWIN_ACCEPT_LICENSE && !exists(path.join(cacheDir, "xwin", "DONE"))) {
    problems.push(`The MSVC CRT and Windows SDK have not been downloaded to ${cacheDir}: run \`XWIN_ACCEPT_LICENSE=1 pnpm check:windows\` once to accept their license and download them.`);
  }
  return problems;
}
